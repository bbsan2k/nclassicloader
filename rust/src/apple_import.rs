//! Bounded immutable store for raw, standard iNES cartridges.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(crate) const MAX_BYTES: u64 = 1024 * 1024;
static STAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) struct ImportedCartridge {
    pub rom: PathBuf,
    pub saves: PathBuf,
    pub digest: String,
    pub mapper: u16,
}

fn invalid(message: impl Into<String>) -> String {
    message.into()
}

fn require_real_dir(path: &Path, allow_missing: bool) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("Refusing symlink: {}", path.display()))
        }
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(format!("Expected directory: {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && allow_missing => Ok(false),
        Err(error) => Err(format!("Cannot inspect {}: {error}", path.display())),
    }
}

fn verify_destination(dir: &Path, digest: &str, bytes: &[u8]) -> Result<bool, String> {
    if !require_real_dir(dir, true)? {
        return Ok(false);
    }
    let rom = dir.join("game.nes");
    let metadata = match fs::symlink_metadata(&rom) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err("Existing import has no game.nes".into());
        }
        Err(error) => return Err(format!("Cannot inspect existing ROM: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Existing game.nes is not a regular file".into());
    }
    if metadata.len() != bytes.len() as u64 {
        return Err("Existing digest collision has different ROM bytes".into());
    }
    let file = File::open(&rom).map_err(|e| format!("Cannot read existing ROM: {e}"))?;
    let mut existing = Vec::with_capacity(bytes.len());
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut existing)
        .map_err(|e| format!("Cannot read existing ROM: {e}"))?;
    if existing != bytes || crate::hash::digest(&existing) != digest {
        return Err("Existing digest collision has different ROM bytes".into());
    }
    require_real_dir(&dir.join("saves"), false)?;
    Ok(true)
}

pub(crate) fn import_cartridge(source: &Path, library: &Path) -> Result<ImportedCartridge, String> {
    if !source.is_absolute() || !library.is_absolute() {
        return Err(invalid("Source and library paths must be absolute"));
    }
    let source_meta =
        fs::symlink_metadata(source).map_err(|e| format!("Cannot inspect source: {e}"))?;
    if source_meta.file_type().is_symlink() || !source_meta.is_file() {
        return Err(invalid("Source must be a regular, nonsymlink file"));
    }
    if source_meta.len() > MAX_BYTES {
        return Err(invalid("Cartridge exceeds the 1 MiB import limit"));
    }
    let input = File::open(source).map_err(|e| format!("Cannot open source: {e}"))?;
    let mut bytes = Vec::new();
    input
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read source: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("Cartridge exceeds the 1 MiB import limit"));
    }
    let mapper = crate::nes::validate(&bytes)?;
    if bytes[9] & 1 != 0 {
        return Err("The Apple raw-NES alpha currently supports NTSC timing only".into());
    }
    let digest = crate::hash::digest(&bytes);

    if !require_real_dir(library, true)? {
        fs::create_dir_all(library).map_err(|e| format!("Cannot create library: {e}"))?;
        require_real_dir(library, false)?;
    }
    let destination = library.join(&digest);
    if verify_destination(&destination, &digest, &bytes)? {
        return Ok(ImportedCartridge {
            rom: destination.join("game.nes"),
            saves: destination.join("saves"),
            digest,
            mapper,
        });
    }

    let stage_path = loop {
        let id = STAGE_COUNTER
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| invalid("Import staging counter exhausted"))?;
        let path = library.join(format!(".import-{}-{id}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Cannot create import stage: {error}")),
        }
    };
    let stage = crate::import_storage::OwnedStage::new(stage_path.clone());
    let rom_path = stage.path().join("game.nes");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&rom_path)
        .map_err(|e| format!("Cannot create staged ROM: {e}"))?;
    output
        .write_all(&bytes)
        .and_then(|_| output.sync_all())
        .map_err(|e| format!("Cannot write staged ROM: {e}"))?;
    drop(output);
    fs::create_dir(stage.path().join("saves"))
        .map_err(|e| format!("Cannot create saves directory: {e}"))?;
    File::open(stage.path()).and_then(|f| f.sync_all()).ok();

    match fs::rename(stage.path(), &destination) {
        Ok(()) => {}
        Err(error) => {
            if verify_destination(&destination, &digest, &bytes)? {
                return Ok(ImportedCartridge {
                    rom: destination.join("game.nes"),
                    saves: destination.join("saves"),
                    digest,
                    mapper,
                });
            }
            return Err(format!("Cannot publish cartridge: {error}"));
        }
    }
    File::open(library).and_then(|f| f.sync_all()).ok();
    Ok(ImportedCartridge {
        rom: destination.join("game.nes"),
        saves: destination.join("saves"),
        digest,
        mapper,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_ID: AtomicU64 = AtomicU64::new(0);
    struct Root(PathBuf);
    impl Root {
        fn new() -> Self {
            let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
            let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.build/apple-import-tests");
            fs::create_dir_all(&parent).unwrap();
            let path = parent.join(format!("apple-import-{}-{id}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Root {
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
    fn source(root: &Root, bytes: &[u8]) -> PathBuf {
        let path = root.0.join("source.nes");
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn publishes_and_reuses_immutable_cartridge_without_touching_source() {
        let root = Root::new();
        let bytes = cartridge();
        let source = source(&root, &bytes);
        let before = fs::read(&source).unwrap();
        let library = root.0.join("library");
        let first = import_cartridge(&source, &library).unwrap();
        assert_eq!(first.mapper, 0);
        assert_eq!(fs::read(&first.rom).unwrap(), bytes);
        assert!(first.saves.is_dir());
        fs::write(first.saves.join("preserve-save"), b"owned save bytes").unwrap();
        let second = import_cartridge(&source, &library).unwrap();
        assert_eq!(first.rom, second.rom);
        assert_eq!(
            fs::read(second.saves.join("preserve-save")).unwrap(),
            b"owned save bytes"
        );
        assert_eq!(fs::read(source).unwrap(), before);
    }

    #[test]
    fn rejects_invalid_oversized_and_nonabsolute_inputs() {
        let root = Root::new();
        let valid = cartridge();
        assert!(crate::nes::validate(&valid[..valid.len() - 1]).is_err());
        let mut unsupported = valid.clone();
        unsupported[6] = 0xf0;
        assert!(crate::nes::validate(&unsupported).is_err());
        let unsupported_path = source(&root, &unsupported);
        assert!(import_cartridge(&unsupported_path, &root.0.join("unsupported-lib")).is_err());
        let mut pal = valid.clone();
        pal[9] = 1;
        let pal_path = source(&root, &pal);
        assert!(import_cartridge(&pal_path, &root.0.join("pal-lib")).is_err());
        let short = source(&root, &valid[..valid.len() - 1]);
        assert!(import_cartridge(&short, &root.0.join("lib")).is_err());
        let large = source(&root, &vec![0; MAX_BYTES as usize + 1]);
        assert!(import_cartridge(&large, &root.0.join("lib")).is_err());
        assert!(import_cartridge(Path::new("relative.nes"), &root.0.join("lib")).is_err());
        assert!(import_cartridge(&short, Path::new("relative-lib")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_source_library_destination_and_saves() {
        use std::os::unix::fs::symlink;
        let root = Root::new();
        let bytes = cartridge();
        let src = source(&root, &bytes);
        let link = root.0.join("source-link");
        symlink(&src, &link).unwrap();
        assert!(import_cartridge(&link, &root.0.join("lib")).is_err());
        let library_target = root.0.join("library-target");
        fs::create_dir(&library_target).unwrap();
        let library_link = root.0.join("library-link");
        symlink(&library_target, &library_link).unwrap();
        assert!(import_cartridge(&src, &library_link).is_err());
        let digest = crate::hash::digest(&bytes);
        let library = root.0.join("lib");
        fs::create_dir(&library).unwrap();
        let dest = library.join(digest);
        symlink(&library_target, &dest).unwrap();
        assert!(import_cartridge(&src, &library).is_err());
        fs::remove_file(&dest).unwrap();
        fs::create_dir(&dest).unwrap();
        symlink(&src, dest.join("game.nes")).unwrap();
        assert!(import_cartridge(&src, &library).is_err());
        fs::remove_file(dest.join("game.nes")).unwrap();
        fs::write(dest.join("game.nes"), &bytes).unwrap();
        symlink(&library_target, dest.join("saves")).unwrap();
        assert!(import_cartridge(&src, &library).is_err());
    }

    #[test]
    fn refuses_digest_collision_without_overwriting_existing_rom() {
        let root = Root::new();
        let bytes = cartridge();
        let src = source(&root, &bytes);
        let library = root.0.join("lib");
        fs::create_dir(&library).unwrap();
        let dest = library.join(crate::hash::digest(&bytes));
        fs::create_dir(&dest).unwrap();
        let collision = b"keep existing bytes";
        fs::write(dest.join("game.nes"), collision).unwrap();
        fs::create_dir(dest.join("saves")).unwrap();
        assert!(import_cartridge(&src, &library).is_err());
        assert_eq!(fs::read(dest.join("game.nes")).unwrap(), collision);
    }

    #[test]
    fn invalid_content_does_not_publish_or_create_a_library() {
        let root = Root::new();
        let src = source(&root, b"not a cartridge");
        let library = root.0.join("lib");
        assert!(import_cartridge(&src, &library).is_err());
        assert!(!library.exists());
    }

    #[test]
    fn existing_empty_destination_is_not_replaced() {
        let root = Root::new();
        let bytes = cartridge();
        let src = source(&root, &bytes);
        let library = root.0.join("lib");
        let destination = library.join(crate::hash::digest(&bytes));
        fs::create_dir_all(&destination).unwrap();
        assert!(import_cartridge(&src, &library).is_err());
        assert!(destination.is_dir());
        assert_eq!(fs::read_dir(destination).unwrap().count(), 0);
    }
}
