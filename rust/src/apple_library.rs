//! Read-only, bounded inspection of the Apple cartridge library.

use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

use crate::apple_import::{ImportedCartridge, MAX_BYTES};

const MAX_CHILDREN: usize = 256;
const MAX_ENTRIES: usize = 64;
const READ_BUDGET: u64 = 16 * 1024 * 1024;

pub(crate) struct LibrarySnapshot {
    pub entries: Vec<ImportedCartridge>,
    pub skipped: usize,
    pub truncated: bool,
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn real_dir(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            Err(format!("Refusing symlink: {}", path.display()))
        }
        Ok(meta) if meta.is_dir() => Ok(true),
        Ok(_) => Err(format!("Expected directory: {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Cannot inspect {}: {error}", path.display())),
    }
}

fn inspect(root: &Path, digest: &str, budget: &mut u64) -> Result<ImportedCartridge, String> {
    let directory = root.join(digest);
    if !real_dir(&directory)? {
        return Err("Cartridge directory is missing".into());
    }
    let rom = directory.join("game.nes");
    let meta = fs::symlink_metadata(&rom).map_err(|e| format!("Cannot inspect game.nes: {e}"))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("game.nes is not a regular nonsymlink file".into());
    }
    if meta.len() > MAX_BYTES {
        return Err("Cartridge exceeds the 1 MiB limit".into());
    }
    if meta.len() > *budget {
        return Err("Library ROM read budget exceeded".into());
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    File::open(&rom)
        .map_err(|e| format!("Cannot open game.nes: {e}"))?
        .take((MAX_BYTES + 1).min(budget.saturating_add(1)))
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read game.nes: {e}"))?;
    let read = bytes.len() as u64;
    if read > *budget {
        *budget = 0;
        return Err("Library ROM read budget exceeded".into());
    }
    *budget -= read;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Cartridge exceeds the 1 MiB limit".into());
    }
    let mapper = crate::nes::validate(&bytes)?;
    if bytes[9] != 0 {
        return Err("The Apple raw-NES alpha currently supports NTSC timing only".into());
    }
    if crate::hash::digest(&bytes) != digest {
        return Err("Cartridge digest does not match its directory".into());
    }
    let saves = directory.join("saves");
    if !real_dir(&saves)? {
        return Err("Cartridge saves directory is missing".into());
    }
    Ok(ImportedCartridge {
        rom,
        saves,
        digest: digest.to_owned(),
        mapper,
    })
}

pub(crate) fn inspect_cartridge(root: &Path, digest: &str) -> Result<ImportedCartridge, String> {
    if !root.is_absolute() {
        return Err("Library path must be absolute".into());
    }
    if !valid_digest(digest) {
        return Err("Digest must be 64 lowercase hexadecimal characters".into());
    }
    if !real_dir(root)? {
        return Err("Library directory is missing".into());
    }
    let mut budget = READ_BUDGET;
    inspect(root, digest, &mut budget)
}

pub(crate) fn scan_library(root: &Path) -> Result<LibrarySnapshot, String> {
    if !root.is_absolute() {
        return Err("Library path must be absolute".into());
    }
    if !real_dir(root)? {
        return Ok(LibrarySnapshot {
            entries: Vec::new(),
            skipped: 0,
            truncated: false,
        });
    }
    let mut names = Vec::new();
    let mut truncated = false;
    for item in fs::read_dir(root).map_err(|e| format!("Cannot scan library: {e}"))? {
        let item = item.map_err(|e| format!("Cannot read library entry: {e}"))?;
        if names.len() == MAX_CHILDREN {
            truncated = true;
            break;
        }
        names.push(item.file_name());
    }
    names.sort();
    let mut entries = Vec::new();
    let mut skipped = 0;
    let mut budget = READ_BUDGET;
    for name in names {
        let Some(digest) = name.to_str() else {
            continue;
        };
        if !valid_digest(digest) {
            continue;
        }
        let child = root.join(digest);
        let child_meta = match fs::symlink_metadata(&child) {
            Ok(meta) => meta,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        if !child_meta.file_type().is_symlink() && !child_meta.is_dir() {
            skipped += 1;
            continue;
        }
        if entries.len() == MAX_ENTRIES {
            truncated = true;
            break;
        }
        match inspect(root, digest, &mut budget) {
            Ok(entry) => entries.push(entry),
            Err(error) if error == "Library ROM read budget exceeded" => {
                truncated = true;
                break;
            }
            Err(_) => skipped += 1,
        }
    }
    Ok(LibrarySnapshot {
        entries,
        skipped,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static ID: AtomicU64 = AtomicU64::new(0);
    struct TestRoot(std::path::PathBuf);
    impl TestRoot {
        fn new() -> Self {
            let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.build/apple-library-tests");
            fs::create_dir_all(&base).unwrap();
            let path = base.join(format!(
                "test-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn cartridge() -> Vec<u8> {
        let mut bytes = vec![0; 16 + 16384];
        bytes[..4].copy_from_slice(b"NES\x1a");
        bytes[4] = 1;
        bytes
    }
    fn publish(root: &TestRoot, bytes: &[u8]) -> ImportedCartridge {
        let source = root.0.join("source.nes");
        fs::write(&source, bytes).unwrap();
        crate::apple_import::import_cartridge(&source, &root.0.join("library")).unwrap()
    }

    #[test]
    fn missing_library_is_empty_and_not_created() {
        let root = TestRoot::new();
        let missing = root.0.join("missing");
        let result = scan_library(&missing).unwrap();
        assert!(result.entries.is_empty());
        assert!(!missing.exists());
    }

    #[test]
    fn reopens_valid_cartridge_stably_without_touching_saves() {
        let root = TestRoot::new();
        let imported = publish(&root, &cartridge());
        fs::write(imported.saves.join("save.dat"), b"owned save").unwrap();
        let result = scan_library(&root.0.join("library")).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].rom, imported.rom);
        let opened = inspect_cartridge(&root.0.join("library"), &imported.digest).unwrap();
        assert_eq!(opened.rom, imported.rom);
        assert_eq!(
            fs::read(opened.saves.join("save.dat")).unwrap(),
            b"owned save"
        );
    }

    #[test]
    fn skips_corrupt_rom_and_rejects_bad_digests() {
        let root = TestRoot::new();
        let imported = publish(&root, &cartridge());
        fs::write(&imported.rom, b"truncated").unwrap();
        let result = scan_library(&root.0.join("library")).unwrap();
        assert_eq!(result.skipped, 1);
        for digest in ["../bad", &imported.digest.to_uppercase(), "abc"] {
            assert!(inspect_cartridge(&root.0.join("library"), digest).is_err());
        }
    }

    #[test]
    fn deterministic_order_and_child_cap_are_reported() {
        let root = TestRoot::new();
        let library = root.0.join("library");
        fs::create_dir(&library).unwrap();
        for n in 0..257 {
            fs::create_dir(library.join(format!("{:064x}", n + 1))).unwrap();
        }
        let result = scan_library(&library).unwrap();
        assert!(result.truncated);
        assert_eq!(result.skipped, 256);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_root_and_skips_symlink_rom() {
        use std::os::unix::fs::symlink;
        let root = TestRoot::new();
        let imported = publish(&root, &cartridge());
        let library = root.0.join("library");
        let target = imported.rom.with_file_name("target.nes");
        fs::rename(&imported.rom, &target).unwrap();
        symlink(&target, &imported.rom).unwrap();
        assert_eq!(scan_library(&library).unwrap().skipped, 1);
        let alias = root.0.join("alias");
        symlink(&library, &alias).unwrap();
        assert!(scan_library(&alias).is_err());
    }

    #[test]
    fn rejects_changed_identity_pal_and_symlink_saves_without_republication() {
        let root = TestRoot::new();
        let bytes = cartridge();
        let imported = publish(&root, &bytes);
        let library = root.0.join("library");
        let mut changed = bytes.clone();
        changed[16] = 1;
        fs::write(&imported.rom, &changed).unwrap();
        assert!(inspect_cartridge(&library, &imported.digest).is_err());
        assert_eq!(scan_library(&library).unwrap().skipped, 1);
        changed = bytes.clone();
        changed[9] = 1;
        fs::write(&imported.rom, &changed).unwrap();
        assert!(inspect_cartridge(&library, &imported.digest).is_err());
        fs::write(&imported.rom, &bytes).unwrap();
        #[cfg(unix)]
        {
            fs::remove_dir(&imported.saves).unwrap();
            std::os::unix::fs::symlink(&root.0, &imported.saves).unwrap();
            assert!(inspect_cartridge(&library, &imported.digest).is_err());
        }
        assert_eq!(fs::read_dir(&library).unwrap().count(), 1);
    }

    #[test]
    fn entry_cap_and_read_budget_are_explicit() {
        let root = TestRoot::new();
        let mut bytes = cartridge();
        let first = publish(&root, &bytes);
        let library = root.0.join("library");
        let mut budget = bytes.len() as u64 - 1;
        assert!(inspect(&library, &first.digest, &mut budget).is_err());
        for n in 1..65 {
            bytes[16] = n;
            publish(&root, &bytes);
        }
        let snapshot = scan_library(&library).unwrap();
        assert_eq!(snapshot.entries.len(), 64);
        assert!(snapshot.truncated);
        assert!(
            snapshot
                .entries
                .windows(2)
                .all(|pair| pair[0].digest < pair[1].digest)
        );
    }
}
