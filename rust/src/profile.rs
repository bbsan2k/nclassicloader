//! Hash-addressed image profiles, conversion, cache validation, and data migration.

use crate::{
    hash::{digest, file_digest},
    image_reader::Files,
    import_job::{self, ImportControl, ImportError},
    import_storage::{
        self, CopyBackend, MigrationScanner, NativeImportBackend, NativeMigrationScanner,
        NativeStageBackend, OwnedStage, Publication, PublishBackend, SourceBackend, StageBackend,
        WriteBackend, WriteError,
    },
    sfrom::convert_sfrom,
    skin::prepare_skin_with,
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const IMPORTER_VERSION: u32 = 7;
const DATA_MIGRATION_LIMIT: u64 = 1024 * 1024 * 1024;
static STAGE_SERIAL: AtomicU64 = AtomicU64::new(0);

type ImportResult<T> = Result<T, String>;
pub type Progress<'a> = dyn FnMut(&str) + 'a;

#[derive(Clone, Debug)]
pub struct Profile {
    pub directory: PathBuf,
    pub manifest: Value,
}

impl Profile {
    pub fn assets(&self) -> PathBuf {
        self.directory.join("assets")
    }
    pub fn data(&self) -> PathBuf {
        self.directory.join("data")
    }

    pub fn label(&self) -> String {
        let region = self
            .manifest
            .get("region")
            .and_then(Value::as_str)
            .unwrap_or("Unknown");
        let version = self
            .manifest
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("Unknown");
        let games = self
            .manifest
            .get("playableGames")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let firmware = self
            .manifest
            .get("externalFirmwareGames")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let system = if self.manifest.get("system").and_then(Value::as_str) == Some("nes") {
            "NES"
        } else {
            "SNES"
        };
        let mut label = format!("{system} / {region} / {version} ({games} prepared");
        if firmware != 0 {
            label.push_str(&format!("; {firmware} needs firmware"));
        }
        label.push(')');
        label
    }
}

fn fail(message: impl Into<String>) -> String {
    message.into()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn absolute(path: &Path) -> ImportResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|e| fail(format!("Cannot resolve selected image path: {e}")))
    }
}

fn regular_not_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

fn directory_not_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_dir())
}

/// Resolve an Android boot image to its local matching filesystem companion.
pub fn resolve_filesystem(source: &Path) -> ImportResult<PathBuf> {
    if !regular_not_symlink(source) {
        return Err("Choose a regular local firmware image or filesystem archive".into());
    }
    let mut file = File::open(source).map_err(|e| format!("Cannot read selected image: {e}"))?;
    let mut magic = [0; 8];
    let count = file
        .read(&mut magic)
        .map_err(|e| format!("Cannot read selected image: {e}"))?;
    if count != 8 || &magic != b"ANDROID!" {
        return absolute(source);
    }
    let name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if name.starts_with("kernel-") && name.ends_with(".img") {
        let stem = &name[7..name.len() - 4];
        let parent = source.parent().unwrap_or_else(|| Path::new("."));
        for directory in [
            parent.to_path_buf(),
            parent.join("SNES-Mini-Kernels"),
            parent.join("NES-Mini-Kernels"),
        ] {
            for suffix in [".hsqs", "_trimmed.hsqs", ".tar.gz"] {
                let sibling = directory.join(format!("{stem}{suffix}"));
                if regular_not_symlink(&sibling) {
                    return absolute(&sibling);
                }
            }
        }
    }
    Err("This Android boot image contains no game filesystem. Put its matching .hsqs or .tar.gz beside it, or choose the filesystem directly.".into())
}

fn read_json(path: &Path, limit: u64) -> ImportResult<Value> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("Cannot read profile manifest: {e}"))?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err("Invalid profile manifest".into());
    }
    let mut file = File::open(path).map_err(|e| format!("Cannot read profile manifest: {e}"))?;
    read_json_from(&mut file, limit)
}

fn read_json_from<R: Read>(reader: &mut R, limit: u64) -> ImportResult<Value> {
    let bytes =
        read_bounded(reader, limit).map_err(|e| format!("Cannot read profile manifest: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err("Invalid profile manifest".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid profile manifest JSON: {e}"))
}

fn read_bounded<R: Read>(reader: &mut R, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn write_bytes_with<B: WriteBackend>(
    backend: &mut B,
    path: &Path,
    bytes: &[u8],
) -> ImportResult<()> {
    import_storage::write_bytes_with(backend, path, bytes).map_err(|error| match error {
        WriteError::Directory(error) => format!("Cannot create profile directory: {error}"),
        WriteError::File(error) => format!("Cannot write profile file: {error}"),
    })
}

fn write_json_with<B: WriteBackend>(
    backend: &mut B,
    path: &Path,
    value: &Value,
) -> ImportResult<()> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|e| format!("Cannot encode profile JSON: {e}"))?;
    bytes.push(b'\n');
    write_bytes_with(backend, path, &bytes)
}

#[cfg(test)]
fn write_json(path: &Path, value: &Value) -> ImportResult<()> {
    write_json_with(&mut NativeImportBackend, path, value)
}

fn id_safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() < 100
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
}

fn trim(value: &str) -> &str {
    value.trim_matches([' ', '\t', '\r', '\n'])
}

fn desktop(bytes: &[u8]) -> ImportResult<BTreeMap<String, BTreeMap<String, String>>> {
    let text =
        std::str::from_utf8(bytes).map_err(|e| format!("Invalid desktop metadata UTF-8: {e}"))?;
    let mut result: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = trim(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_owned();
            continue;
        }
        let Some(pos) = line.find('=') else { continue };
        let key = trim(&line[..pos]).to_owned();
        let value = trim(&line[pos + 1..]).to_owned();
        let map = result.entry(section.clone()).or_default();
        if map.insert(key, value).is_some() {
            return Err("Duplicate desktop metadata key".into());
        }
    }
    Ok(result)
}

fn field<'a>(section: &'a BTreeMap<String, String>, name: &str) -> &'a str {
    section.get(name).map(String::as_str).unwrap_or("")
}

fn safe_profile_file(directory: &Path, relative: &str) -> bool {
    let path = Path::new(relative);
    if path.is_absolute() || relative.contains('\\') || !relative.starts_with("assets/") {
        return false;
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return false;
    }
    let mut current = directory.to_path_buf();
    for part in path.components() {
        current.push(part.as_os_str());
        if fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return false;
        }
    }
    true
}

pub fn valid_profile(profile: &Profile) -> bool {
    let result = (|| -> ImportResult<bool> {
        let m = &profile.manifest;
        let version = m
            .get("importerVersion")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if ![4, 5, 6, u64::from(IMPORTER_VERSION)].contains(&version)
            || !directory_not_symlink(&profile.directory)
        {
            return Ok(false);
        }
        let files = match m.get("files").and_then(Value::as_object) {
            Some(value) if !value.is_empty() => value,
            _ => return Ok(false),
        };
        let sha = match m.get("sourceSHA256").and_then(Value::as_str) {
            Some(v) if v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()) => v,
            _ => return Ok(false),
        };
        let expected_name = format!("{sha}-v{version}");
        if profile.directory.file_name().and_then(|v| v.to_str()) != Some(&expected_name) {
            return Ok(false);
        }
        for (name, hash) in files {
            if !safe_profile_file(&profile.directory, name) {
                return Ok(false);
            }
            let path = profile.directory.join(name);
            if !regular_not_symlink(&path) || file_digest(&path).ok().as_deref() != hash.as_str() {
                return Ok(false);
            }
        }
        Ok(files.contains_key("assets/catalog.json")
            && files.contains_key("assets/skin/layout.json"))
    })();
    result.unwrap_or(false)
}

fn create_stage(cache: &Path, sha: &str) -> ImportResult<OwnedStage> {
    create_stage_with(cache, sha, &mut NativeStageBackend)
}

fn create_stage_with<B: StageBackend>(
    cache: &Path,
    sha: &str,
    backend: &mut B,
) -> ImportResult<OwnedStage> {
    for _ in 0..20 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let serial = STAGE_SERIAL.fetch_add(1, Ordering::Relaxed);
        let path = cache.join(format!(".import-{}-{now}-{serial}", &sha[..12]));
        match backend.create_dir(&path) {
            // Ownership is recorded only after this invocation exclusively creates the path.
            Ok(()) => return Ok(OwnedStage::new(path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Cannot create staging profile: {error}")),
        }
    }
    Err("Cannot allocate unique staging profile directory".into())
}

fn locate_ipl(files: &Files) -> Option<[u8; 64]> {
    const EXPECTED: &str = "c95f88b299030d5afa55b1031e2b5ef2dff650c4b4e6bb6f8b1359436521278f";
    let carrier = files.get("usr/bin/canoe-shvc")?;
    if carrier.len() < 64 {
        return None;
    }
    for offset in 0..=carrier.len() - 64 {
        if carrier[offset..offset + 4] != [0xcd, 0xef, 0xbd, 0xe8] {
            continue;
        }
        let bytes: [u8; 64] = carrier[offset..offset + 64].try_into().ok()?;
        if digest(&bytes) == EXPECTED {
            return Some(bytes);
        }
    }
    None
}

struct MigrationHooks<'a> {
    report: &'a mut dyn FnMut(&str),
    checkpoint: &'a mut dyn FnMut() -> Result<(), ImportError>,
}

#[cfg(test)]
fn preserve_old_data<B: WriteBackend + CopyBackend>(
    cache: &Path,
    sha: &str,
    stage: &Path,
    destination: &Path,
    manifest: &mut Value,
    hooks: &mut MigrationHooks<'_>,
    backend: &mut B,
) -> Result<(), ImportError> {
    preserve_old_data_with_scanner(
        cache,
        sha,
        stage,
        destination,
        manifest,
        hooks,
        backend,
        &mut NativeMigrationScanner,
    )
}

#[allow(clippy::too_many_arguments)]
fn preserve_old_data_with_scanner<B, S>(
    cache: &Path,
    sha: &str,
    stage: &Path,
    destination: &Path,
    manifest: &mut Value,
    hooks: &mut MigrationHooks<'_>,
    backend: &mut B,
    scanner: &mut S,
) -> Result<(), ImportError>
where
    B: WriteBackend + CopyBackend,
    S: MigrationScanner,
{
    for previous in (1..IMPORTER_VERSION).rev() {
        let old = cache.join(format!("{sha}-v{previous}"));
        let data = old.join("data");
        if !directory_not_symlink(&old) || !directory_not_symlink(&data) {
            continue;
        }
        let old_manifest = match read_json(&old.join("profile.json"), 32 * 1024 * 1024) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if old_manifest.get("sourceSHA256").and_then(Value::as_str) != Some(sha)
            || old_manifest.get("importerVersion").and_then(Value::as_u64)
                != Some(u64::from(previous))
        {
            continue;
        }
        (hooks.report)("Preserving settings and saves from the previous importer...");
        (hooks.checkpoint)()?;
        let mut entries = Vec::new();
        let mut stack = vec![data.clone()];
        let mut total = 0u64;
        while let Some(directory) = stack.pop() {
            for entry in scanner
                .read_dir(&directory)
                .map_err(|e| format!("Cannot read previous profile data: {e}"))?
            {
                let entry = entry.map_err(|e| format!("Cannot read previous profile data: {e}"))?;
                let metadata = scanner
                    .symlink_metadata(&entry.path())
                    .map_err(|e| format!("Cannot inspect previous profile data: {e}"))?;
                if metadata.file_type().is_symlink() {
                    return Err("Previous profile data exceeds migration limits or contains a symlink; the old profile is unchanged".into());
                }
                if metadata.is_dir() {
                    entries.push((entry.path(), true, 0));
                    stack.push(entry.path());
                } else if metadata.is_file() {
                    total = total.checked_add(metadata.len()).ok_or(
                        "Previous saves exceed the 1 GiB migration limit; old profile is unchanged",
                    )?;
                    if total > DATA_MIGRATION_LIMIT {
                        return Err("Previous saves exceed the 1 GiB migration limit; old profile is unchanged".into());
                    }
                    entries.push((entry.path(), false, metadata.len()));
                }
                if entries.len() > 20_000 {
                    return Err("Previous profile data exceeds migration limits or contains a symlink; the old profile is unchanged".into());
                }
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (source, is_directory, _) in entries {
            (hooks.checkpoint)()?;
            let relative = source
                .strip_prefix(&data)
                .map_err(|_| "Previous profile path escaped its data directory")?;
            let target = stage.join("data").join(relative);
            if is_directory {
                backend
                    .create_dir_all(&target)
                    .map_err(|e| format!("Cannot migrate previous profile data: {e}"))?;
                (hooks.checkpoint)()?;
            } else {
                if let Some(parent) = target.parent() {
                    backend
                        .create_dir_all(parent)
                        .map_err(|e| format!("Cannot migrate previous profile data: {e}"))?;
                }
                backend
                    .copy(&source, &target)
                    .map_err(|e| format!("Cannot migrate previous profile data: {e}"))?;
                (hooks.checkpoint)()?;
            }
        }
        let preferences = stage.join("data/preferences.json");
        if regular_not_symlink(&preferences) {
            let mut values = read_json(&preferences, 32 * 1024 * 1024)?;
            let old_firmware = path_string(&old.join("assets/firmware"));
            if values.get("firmware").and_then(Value::as_str) == Some(&old_firmware) {
                values["firmware"] = json!(path_string(&destination.join("assets/firmware")));
                write_json_with(backend, &preferences, &values)?;
            }
        }
        (hooks.checkpoint)()?;
        manifest["migratedFrom"] = json!(path_string(&old));
        break;
    }
    Ok(())
}

fn hash_asset_tree(root: &Path) -> ImportResult<Map<String, Value>> {
    let mut files = Map::new();
    let mut stack = vec![root.to_path_buf()];
    let mut count = 0;
    while let Some(directory) = stack.pop() {
        for entry in
            fs::read_dir(&directory).map_err(|e| format!("Cannot read generated assets: {e}"))?
        {
            let entry = entry.map_err(|e| format!("Cannot read generated assets: {e}"))?;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|e| format!("Cannot inspect generated assets: {e}"))?;
            if metadata.file_type().is_symlink() {
                return Err("Generated profile assets unexpectedly contain a symbolic link".into());
            }
            if metadata.is_dir() {
                stack.push(entry.path());
            } else if metadata.is_file() {
                count += 1;
                if count > 100_000 {
                    return Err("Generated asset count exceeds limit".into());
                }
                let relative = entry
                    .path()
                    .strip_prefix(root.parent().expect("assets has parent"))
                    .map_err(|_| "Generated asset escaped stage directory")?
                    .to_string_lossy()
                    .replace('\\', "/");
                files.insert(
                    relative,
                    json!(
                        file_digest(&entry.path())
                            .map_err(|e| format!("Cannot hash generated asset: {e}"))?
                    ),
                );
            }
        }
    }
    Ok(files)
}

/// Import a supplied image into a verified profile cache. The source image and
/// extracted executable payloads are never modified or launched.
pub fn import_image(
    source: &Path,
    cache: &Path,
    progress: Option<&mut Progress<'_>>,
) -> ImportResult<Profile> {
    import_image_cancellable(source, cache, progress, &ImportControl::new())
        .map_err(|error| error.to_string())
}

/// Imports an image using a caller-owned cancellation handle.
///
/// Cancellation is observed between import phases; an in-progress filesystem
/// read, hash, skin preparation, or game conversion runs to its phase boundary.
pub fn import_image_cancellable(
    source: &Path,
    cache: &Path,
    progress: Option<&mut Progress<'_>>,
    control: &ImportControl,
) -> Result<Profile, ImportError> {
    import_image_controlled_with_backend(source, cache, progress, &mut NativeImportBackend, control)
}

#[cfg(test)]
pub(crate) fn import_image_with_backend<B>(
    source: &Path,
    cache: &Path,
    progress: Option<&mut Progress<'_>>,
    backend: &mut B,
) -> ImportResult<Profile>
where
    B: WriteBackend + PublishBackend + CopyBackend + SourceBackend,
{
    import_image_controlled_with_backend(source, cache, progress, backend, &ImportControl::new())
        .map_err(|error| error.to_string())
}

fn import_image_controlled_with_backend<B>(
    source: &Path,
    cache: &Path,
    progress: Option<&mut Progress<'_>>,
    backend: &mut B,
    control: &ImportControl,
) -> Result<Profile, ImportError>
where
    B: WriteBackend + PublishBackend + CopyBackend + SourceBackend,
{
    import_image_controlled_with_scanner(
        source,
        cache,
        progress,
        backend,
        control,
        &mut NativeMigrationScanner,
    )
}

fn import_image_controlled_with_scanner<B, S>(
    source: &Path,
    cache: &Path,
    progress: Option<&mut Progress<'_>>,
    backend: &mut B,
    control: &ImportControl,
    scanner: &mut S,
) -> Result<Profile, ImportError>
where
    B: WriteBackend + PublishBackend + CopyBackend + SourceBackend,
    S: MigrationScanner,
{
    control.start().map_err(import_job::start_error)?;
    let result = import_image_inner(source, cache, progress, backend, control, scanner);
    match result {
        Err(ImportError::Cancelled) => Err(ImportError::Cancelled),
        Err(error) => {
            control.finish_failure();
            Err(error)
        }
        Ok(profile) => {
            if control.finish_success() {
                Ok(profile)
            } else {
                Err(ImportError::Failed(
                    "Invalid import lifecycle completion state".into(),
                ))
            }
        }
    }
}

fn import_image_inner<B>(
    source: &Path,
    cache: &Path,
    mut progress: Option<&mut Progress<'_>>,
    backend: &mut B,
    control: &ImportControl,
    scanner: &mut impl MigrationScanner,
) -> Result<Profile, ImportError>
where
    B: WriteBackend + PublishBackend + CopyBackend + SourceBackend,
{
    let report = |progress: &mut Option<&mut Progress<'_>>, message: &str| {
        if let Some(callback) = progress.as_deref_mut() {
            callback(message);
        }
    };
    let mut checkpoint = || control.checkpoint().map_err(import_job::checkpoint_error);
    checkpoint()?;
    let filesystem = resolve_filesystem(source)?;
    let size = fs::metadata(&filesystem)
        .map_err(|e| format!("Cannot inspect filesystem image: {e}"))?
        .len();
    if size > 1024 * 1024 * 1024 {
        return Err("Image exceeds 1 GiB import limit".into());
    }
    report(&mut progress, "Hashing filesystem image...");
    checkpoint()?;
    let sha = backend
        .source_digest(&filesystem)
        .map_err(|e| format!("Cannot hash filesystem image: {e}"))?;
    checkpoint()?;
    if fs::symlink_metadata(cache).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("Profile cache must not be a symbolic link".into());
    }
    fs::create_dir_all(cache).map_err(|e| format!("Cannot create profile cache: {e}"))?;
    if fs::symlink_metadata(cache).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("Profile cache must not be a symbolic link".into());
    }
    let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
    if fs::symlink_metadata(&destination).is_ok() {
        let existing = Profile {
            directory: destination.clone(),
            manifest: read_json(&destination.join("profile.json"), 32 * 1024 * 1024)?,
        };
        report(&mut progress, "Checking cached profile...");
        checkpoint()?;
        if !valid_profile(&existing) {
            return Err(format!(
                "Cached profile failed integrity checks. Preserve/rename that profile folder, then import again: {}",
                destination.display()
            ).into());
        }
        report(&mut progress, "Using verified cache");
        checkpoint()?;
        control
            .begin_commit()
            .map_err(import_job::checkpoint_error)?;
        return Ok(existing);
    }
    report(
        &mut progress,
        "Reading image (no extracted code is executed)...",
    );
    checkpoint()?;
    let files = backend.read_image(&filesystem)?;
    checkpoint()?;
    if backend
        .source_digest(&filesystem)
        .map_err(|e| format!("Cannot rehash filesystem image: {e}"))?
        != sha
    {
        return Err("Source image changed during import".into());
    }
    checkpoint()?;
    let mut ui = None;
    for candidate in ["snes-eur", "snes-usa", "shvc"] {
        if files.contains_key(&format!("usr/share/ui/{candidate}/dependencies.json"))
            && ui.replace(candidate).is_some()
        {
            return Err("Ambiguous image: multiple stock UI roots".into());
        }
    }
    if files.contains_key("usr/share/clover-ui/dependencies.json") {
        if ui.is_some() {
            return Err("Ambiguous image: multiple stock UI roots".into());
        }
        ui = Some(
            if files.contains_key("usr/share/clover-ui/resources/fonts/hvc/m.fnt") {
                "hvc"
            } else {
                "nes"
            },
        );
    }
    let ui = ui.ok_or("Unsupported image: no recognized NES/SNES Classic UI (a save-data backup is not a game filesystem)")?;
    let nes = matches!(ui, "nes" | "hvc");
    let locale = if matches!(ui, "shvc" | "hvc") {
        "jpn_ja"
    } else {
        "usa_en"
    };
    let system = if nes { "nes" } else { "snes" };
    let region = match ui {
        "shvc" | "hvc" => "Japan",
        "snes-usa" | "nes" => "USA",
        _ => "Europe",
    };
    let mut version = "unknown".to_owned();
    if let Some(bytes) = files.get("etc/clover/VERSION") {
        let value = std::str::from_utf8(bytes)
            .map_err(|e| format!("Invalid image version metadata: {e}"))?;
        version = trim(value).to_owned();
    }
    if version.is_empty() || version.len() > 200 {
        return Err("Invalid image version metadata".into());
    }

    let stage = create_stage(cache, &sha)?;
    let stage_path = stage.path().to_path_buf();
    fs::create_dir_all(stage_path.join("data"))
        .map_err(|e| format!("Cannot initialize profile data directory: {e}"))?;
    report(&mut progress, "Decoding Clover artwork and fonts...");
    checkpoint()?;
    prepare_skin_with(&files, ui, &stage_path.join("assets/skin"), backend)?;
    checkpoint()?;

    if !nes {
        report(
            &mut progress,
            "Locating verified SMP boot firmware in the supplied image...",
        );
        checkpoint()?;
        let ipl = locate_ipl(&files)
            .ok_or("Image does not contain the verified SMP IPL firmware required by ares")?;
        write_bytes_with(backend, &stage_path.join("assets/firmware/ipl.rom"), &ipl)?;
    }

    let mut games = Vec::new();
    let mut conversions = Vec::new();
    let mut playable = 0u32;
    let mut external_firmware_games = 0u32;
    let mut codes = BTreeSet::new();
    let desktop_members: Vec<_> = files
        .iter()
        .filter(|(path, _)| path.starts_with("usr/share/games/") && path.ends_with(".desktop"))
        .collect();
    for (path, bytes) in desktop_members {
        let sections = desktop(bytes)?;
        let entry = sections.get("Desktop Entry").cloned().unwrap_or_default();
        let meta = sections.get("X-CLOVER Game").cloned().unwrap_or_default();
        let code = field(&meta, "Code");
        if nes
            && code == "PRODUCTION-TESTS"
            && path == "usr/share/games/nes/kachikachi/PRODUCTION-TESTS/PRODUCTION-TESTS.desktop"
        {
            continue;
        }
        let game_root = if nes {
            format!("usr/share/games/nes/kachikachi/{code}/")
        } else {
            format!("usr/share/games/{code}/")
        };
        if !id_safe(code)
            || path != &format!("{game_root}{code}.desktop")
            || !codes.insert(code.to_owned())
        {
            return Err("Invalid or duplicate catalog code".into());
        }
        let title = field(&entry, "Name");
        if title.is_empty() {
            return Err("Missing catalog title".into());
        }
        let players = if field(&meta, "Players").is_empty() {
            1
        } else {
            field(&meta, "Players")
                .parse::<u32>()
                .map_err(|_| "Invalid player count")?
        };
        if !(1..=8).contains(&players) {
            return Err("Invalid player count".into());
        }
        let mut game = json!({
            "id": code,
            "system": system,
            "title": title,
            "sortTitle": field(&meta, "SortRawTitle"),
            "publisher": field(&meta, "SortRawPublisher"),
            "releaseDate": field(&meta, "ReleaseDate"),
            "players": players,
            "simultaneous": field(&meta, "Simultaneous") == "1",
            "copyright": field(&meta, "Copyright"),
        });
        for suffix in [".png", "_small.png"] {
            let name = format!("{code}{suffix}");
            let art = files
                .get(&format!("{game_root}{name}"))
                .ok_or_else(|| format!("Catalog artwork is missing: {code}"))?;
            write_bytes_with(
                backend,
                &stage_path.join("assets/games").join(code).join(name),
                art,
            )?;
        }
        report(&mut progress, &format!("Preparing {title}..."));
        checkpoint()?;
        if nes {
            let mut conversion = json!({"id": code, "title": title});
            let result = if let Some(rom) = files.get(&format!("{game_root}{code}.nes")) {
                crate::nes::prepare(rom)
            } else if files.contains_key(&format!("{game_root}{code}.qd")) {
                Err("Famicom Disk System .qd images require disk conversion and a user-supplied BIOS; not implemented yet".to_owned())
            } else {
                Err("Missing NES cartridge image".to_owned())
            };
            match result {
                Ok((rom, mapper, status)) => {
                    let relative = format!("roms/{code}.nes");
                    write_bytes_with(backend, &stage_path.join("assets").join(&relative), &rom)?;
                    game["rom"] = json!(relative);
                    game["importStatus"] = json!(status);
                    game["mapper"] = json!(mapper);
                    conversion["status"] = json!(status);
                    conversion["sourceSHA256"] = json!(digest(
                        files
                            .get(&format!("{game_root}{code}.nes"))
                            .expect("validated member")
                    ));
                    conversion["romSHA256"] = json!(digest(&rom));
                    playable += 1;
                }
                Err(error) => {
                    game["importError"] = json!(error);
                    conversion["error"] = json!(error);
                }
            }
            games.push(game);
            conversions.push(conversion);
            checkpoint()?;
            continue;
        }
        let source_rom = files.get(&format!("{game_root}{code}.sfrom"));
        let mut conversion = json!({"id": code, "title": title});
        let converted = match source_rom {
            None => Err("Missing SFROM member".to_owned()),
            Some(input) => {
                conversion["sourceSHA256"] = json!(digest(input));
                convert_sfrom(input).map_err(|e| e.to_string())
            }
        };
        match converted {
            Ok(converted) => {
                if converted.rom.is_empty() {
                    return Err("Converter produced no ROM".into());
                }
                let relative = format!("roms/{code}.sfc");
                write_bytes_with(
                    backend,
                    &stage_path.join("assets").join(&relative),
                    &converted.rom,
                )?;
                game["rom"] = json!(relative);
                game["importStatus"] = json!(converted.status);
                if code == "CLV-P-SAAFE" || code == "CLV-P-VAAFJ" {
                    let required = json!(["dsp1b.upd7725.program.rom", "dsp1b.upd7725.data.rom"]);
                    game["firmwareRequired"] = required.clone();
                    conversion["firmwareRequired"] = required;
                    external_firmware_games += 1;
                }
                conversion["status"] = json!(converted.status);
                conversion["metadata"] = converted.metadata;
                conversion["romSHA256"] = json!(digest(&converted.rom));
                playable += 1;
            }
            Err(error) => {
                game["importError"] = json!(error);
                conversion["error"] = json!(error);
            }
        }
        games.push(game);
        conversions.push(conversion);
        checkpoint()?;
    }
    if games.len() != if nes { 30 } else { 21 } {
        return Err(format!(
            "Unsupported stock catalog: expected {} entries",
            if nes { 30 } else { 21 }
        )
        .into());
    }
    let catalog = json!({
        "schema": 1, "region": region, "ui": ui, "system": system,
        "locale": locale,
        "version": version, "games": games,
    });
    write_json_with(backend, &stage_path.join("assets/catalog.json"), &catalog)?;
    let selected_source = absolute(source)?;
    let mut manifest = json!({
        "schema": 1, "importerVersion": IMPORTER_VERSION, "sourceSHA256": sha,
        "source": path_string(&filesystem), "selectedSource": path_string(&selected_source),
        "selectedSourceSHA256": backend.source_digest(source).map_err(|e| format!("Cannot hash selected image: {e}"))?,
        "region": region, "ui": ui, "locale": locale, "system": system,
        "version": version, "games": games.len(), "playableGames": playable,
        "externalFirmwareGames": external_firmware_games, "conversions": conversions,
        "smpIPL": if nes { Value::Null } else { json!({"source": "usr/bin/canoe-shvc", "sha256": "c95f88b299030d5afa55b1031e2b5ef2dff650c4b4e6bb6f8b1359436521278f"}) },
        "files": {},
    });
    checkpoint()?;
    {
        let mut migration_report = |message: &str| report(&mut progress, message);
        let mut migration_hooks = MigrationHooks {
            report: &mut migration_report,
            checkpoint: &mut checkpoint,
        };
        preserve_old_data_with_scanner(
            cache,
            &sha,
            &stage_path,
            &destination,
            &mut manifest,
            &mut migration_hooks,
            backend,
            scanner,
        )?;
    }
    checkpoint()?;
    manifest["files"] = Value::Object(hash_asset_tree(&stage_path.join("assets"))?);
    write_json_with(backend, &stage_path.join("profile.json"), &manifest)?;
    checkpoint()?;
    control
        .begin_commit()
        .map_err(import_job::checkpoint_error)?;
    match import_storage::publish_with(backend, &stage_path, &destination, || {
        let existing = Profile {
            directory: destination.clone(),
            manifest: read_json(&destination.join("profile.json"), 32 * 1024 * 1024)?,
        };
        Ok(valid_profile(&existing).then_some(existing))
    })? {
        Publication::Published => {}
        Publication::Existing(existing) => {
            report(&mut progress, "Using verified cache");
            return Ok(existing);
        }
    }
    report(&mut progress, "Profile ready");
    Ok(Profile {
        directory: destination,
        manifest,
    })
}

pub fn list_profiles(cache: &Path) -> Vec<Profile> {
    let Ok(entries) = fs::read_dir(cache) else {
        return Vec::new();
    };
    let mut profiles = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default();
        if name.starts_with('.') || !directory_not_symlink(&path) {
            continue;
        }
        let Ok(manifest) = read_json(&path.join("profile.json"), 32 * 1024 * 1024) else {
            continue;
        };
        let profile = Profile {
            directory: path,
            manifest,
        };
        if valid_profile(&profile) {
            profiles.push(profile);
        }
    }
    profiles.sort_by_key(Profile::label);
    let current: BTreeSet<_> = profiles
        .iter()
        .filter(|profile| {
            profile
                .manifest
                .get("importerVersion")
                .and_then(Value::as_u64)
                == Some(u64::from(IMPORTER_VERSION))
        })
        .filter_map(|profile| {
            profile
                .manifest
                .get("sourceSHA256")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    profiles.retain(|profile| {
        profile
            .manifest
            .get("importerVersion")
            .and_then(Value::as_u64)
            == Some(u64::from(IMPORTER_VERSION))
            || !profile
                .manifest
                .get("sourceSHA256")
                .and_then(Value::as_str)
                .is_some_and(|sha| current.contains(sha))
    });
    profiles
}

pub fn discover_images(directories: &[PathBuf]) -> Vec<PathBuf> {
    let mut unique = BTreeSet::new();
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for (index, entry) in entries.flatten().enumerate() {
            if index >= 2000 {
                break;
            }
            let path = entry.path();
            if !regular_not_symlink(&path) {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default();
            if !(name.ends_with(".img") || name.ends_with(".hsqs") || name.ends_with(".tar.gz")) {
                continue;
            }
            if let Ok(filesystem) = resolve_filesystem(&path) {
                unique.insert(
                    fs::canonicalize(&filesystem)
                        .unwrap_or_else(|_| absolute(&filesystem).unwrap_or(filesystem)),
                );
            }
        }
    }
    let mut result: Vec<_> = unique.iter().cloned().collect();
    result.retain(|path| {
        let text = path.to_string_lossy();
        let original = if let Some(stem) = text.strip_suffix("_trimmed.hsqs") {
            format!("{stem}.hsqs")
        } else if let Some(stem) = text.strip_suffix(".tar.gz") {
            format!("{stem}.hsqs")
        } else {
            text.to_string()
        };
        text == original || !unique.contains(&PathBuf::from(original))
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_reader;
    use crate::import_storage::{NativeWriteBackend, PublishBackend, publish_with};
    use std::{
        io::{self, Write},
        process::Command,
        time::UNIX_EPOCH,
    };

    struct RenameFault {
        after_rename: bool,
    }

    struct MigrationCopyFailure;
    struct MigrationDirectoryFailure;

    #[derive(Clone, Copy)]
    enum MigrationScanFault {
        ReadDir,
        Entry,
        Metadata,
    }

    struct FaultMigrationScanner {
        fault: MigrationScanFault,
        fired: bool,
    }

    impl MigrationScanner for FaultMigrationScanner {
        type Entries = Box<dyn Iterator<Item = io::Result<fs::DirEntry>>>;

        fn read_dir(&mut self, path: &Path) -> io::Result<Self::Entries> {
            if !self.fired && matches!(self.fault, MigrationScanFault::ReadDir) {
                self.fired = true;
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            if !self.fired && matches!(self.fault, MigrationScanFault::Entry) {
                self.fired = true;
                return Ok(Box::new(std::iter::once(Err(io::Error::from(
                    io::ErrorKind::PermissionDenied,
                )))));
            }
            Ok(Box::new(fs::read_dir(path)?))
        }

        fn symlink_metadata(&mut self, path: &Path) -> io::Result<fs::Metadata> {
            if !self.fired && matches!(self.fault, MigrationScanFault::Metadata) {
                self.fired = true;
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            fs::symlink_metadata(path)
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum WholeImportFault {
        PackedSprite,
        SkinDirectory,
        Catalog,
        Manifest,
        MigrationDirectory,
        MigrationCopy,
        RenameBefore,
        RenameAfter,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum RestartExitPhase {
        BeforePublication,
        AfterPublication,
    }

    struct RestartExitBackend {
        phase: RestartExitPhase,
    }

    struct FaultBackend {
        cache: PathBuf,
        fault: WholeImportFault,
        fired: bool,
    }

    #[derive(Clone, Copy)]
    enum CancellationBackendPoint {
        MigrationCopy,
        ManifestCreate,
        Rename,
    }

    struct CancellationBackend {
        control: crate::import_job::ImportControl,
        point: CancellationBackendPoint,
        fired: bool,
    }

    impl SourceBackend for FaultBackend {
        fn source_digest(&mut self, path: &Path) -> io::Result<String> {
            NativeImportBackend.source_digest(path)
        }

        fn read_image(&mut self, path: &Path) -> Result<image_reader::Files, String> {
            NativeImportBackend.read_image(path)
        }
    }

    impl SourceBackend for CancellationBackend {
        fn source_digest(&mut self, path: &Path) -> io::Result<String> {
            NativeImportBackend.source_digest(path)
        }

        fn read_image(&mut self, path: &Path) -> Result<image_reader::Files, String> {
            NativeImportBackend.read_image(path)
        }
    }

    impl SourceBackend for RestartExitBackend {
        fn source_digest(&mut self, path: &Path) -> io::Result<String> {
            NativeImportBackend.source_digest(path)
        }

        fn read_image(&mut self, path: &Path) -> Result<image_reader::Files, String> {
            NativeImportBackend.read_image(path)
        }
    }

    struct SourceProbe {
        hashes: usize,
        reads: usize,
        fail_hash: bool,
    }

    impl SourceBackend for SourceProbe {
        fn source_digest(&mut self, path: &Path) -> io::Result<String> {
            self.hashes += 1;
            if self.fail_hash {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                NativeImportBackend.source_digest(path)
            }
        }

        fn read_image(&mut self, path: &Path) -> Result<image_reader::Files, String> {
            self.reads += 1;
            NativeImportBackend.read_image(path)
        }
    }

    impl WriteBackend for SourceProbe {
        type File = File;

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            NativeImportBackend.create_dir_all(path)
        }

        fn create_file(&mut self, path: &Path) -> io::Result<Self::File> {
            NativeImportBackend.create_file(path)
        }
    }

    impl CopyBackend for SourceProbe {
        fn copy(&mut self, source: &Path, destination: &Path) -> io::Result<u64> {
            NativeImportBackend.copy(source, destination)
        }
    }

    impl PublishBackend for SourceProbe {
        fn rename(&mut self, stage: &Path, destination: &Path) -> io::Result<()> {
            NativeImportBackend.rename(stage, destination)
        }

        fn destination_exists(&mut self, destination: &Path) -> bool {
            NativeImportBackend.destination_exists(destination)
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum SourceFault {
        InitialHash,
        Parser,
        FilesystemRehash,
        FilesystemRehashMismatch,
        SelectedSourceHash,
        ParserAfterCancel,
    }

    struct SourceFaultBackend {
        fault: SourceFault,
        control: crate::import_job::ImportControl,
        hash_calls: usize,
        read_calls: usize,
        fired: bool,
        cancel_result: Option<crate::frontend::import_lifecycle::CancelResult>,
    }

    impl SourceBackend for SourceFaultBackend {
        fn source_digest(&mut self, path: &Path) -> io::Result<String> {
            self.hash_calls += 1;
            let fail_on_call = match self.fault {
                SourceFault::InitialHash => Some(1),
                SourceFault::FilesystemRehash => Some(2),
                SourceFault::SelectedSourceHash => Some(3),
                _ => None,
            };
            if fail_on_call == Some(self.hash_calls) {
                self.fired = true;
                return Err(io::Error::other("injected source digest failure"));
            }
            if matches!(self.fault, SourceFault::FilesystemRehashMismatch) && self.hash_calls == 2 {
                self.fired = true;
                return Ok("0".repeat(64));
            }
            NativeImportBackend.source_digest(path)
        }

        fn read_image(&mut self, path: &Path) -> Result<image_reader::Files, String> {
            self.read_calls += 1;
            if matches!(
                self.fault,
                SourceFault::Parser | SourceFault::ParserAfterCancel
            ) {
                self.fired = true;
                if matches!(self.fault, SourceFault::ParserAfterCancel) {
                    self.cancel_result = Some(self.control.cancel());
                }
                return Err("Image reader: injected parser error".into());
            }
            NativeImportBackend.read_image(path)
        }
    }

    impl WriteBackend for SourceFaultBackend {
        type File = File;

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            NativeImportBackend.create_dir_all(path)
        }

        fn create_file(&mut self, path: &Path) -> io::Result<Self::File> {
            NativeImportBackend.create_file(path)
        }
    }

    impl CopyBackend for SourceFaultBackend {
        fn copy(&mut self, source: &Path, destination: &Path) -> io::Result<u64> {
            NativeImportBackend.copy(source, destination)
        }
    }

    impl PublishBackend for SourceFaultBackend {
        fn rename(&mut self, stage: &Path, destination: &Path) -> io::Result<()> {
            NativeImportBackend.rename(stage, destination)
        }

        fn destination_exists(&mut self, destination: &Path) -> bool {
            NativeImportBackend.destination_exists(destination)
        }
    }

    #[test]
    fn initial_source_hash_failure_precedes_cache_creation() {
        let root = temporary("source-hash-failure");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("tiny.img");
        fs::write(&source, b"small synthetic source").unwrap();
        let cache = root.join("cache");
        let mut backend = SourceProbe {
            hashes: 0,
            reads: 0,
            fail_hash: true,
        };

        let error = import_image_with_backend(&source, &cache, None, &mut backend).unwrap_err();
        assert!(error.starts_with("Cannot hash filesystem image:"));
        assert_eq!(backend.hashes, 1);
        assert_eq!(backend.reads, 0);
        assert!(!cache.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parser_error_is_unchanged_after_initial_hash_and_cache_creation() {
        let root = temporary("source-parser-error");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("tiny.img");
        fs::write(&source, b"small synthetic source").unwrap();
        let cache = root.join("cache");
        let expected = image_reader::read_image(&source).unwrap_err();
        let mut backend = SourceProbe {
            hashes: 0,
            reads: 0,
            fail_hash: false,
        };

        let error = import_image_with_backend(&source, &cache, None, &mut backend).unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(backend.hashes, 1);
        assert_eq!(backend.reads, 1);
        assert!(cache.is_dir());
        let _ = fs::remove_dir_all(root);
    }

    struct FaultFile {
        file: File,
        fail_after: Option<usize>,
        written: usize,
    }

    impl Write for FaultFile {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let Some(limit) = self.fail_after else {
                return self.file.write(bytes);
            };
            if self.written >= limit {
                return Err(io::Error::from(io::ErrorKind::StorageFull));
            }
            let count = bytes.len().min(limit - self.written);
            let written = self.file.write(&bytes[..count])?;
            self.written += written;
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }

    impl FaultBackend {
        fn stage_path(&self, path: &Path) -> bool {
            path.strip_prefix(&self.cache)
                .ok()
                .and_then(|relative| relative.components().next())
                .and_then(|component| component.as_os_str().to_str())
                .is_some_and(|name| name.starts_with(".import-"))
        }

        fn partial_write_target(&self, path: &Path) -> bool {
            if !self.stage_path(path) {
                return false;
            }
            match self.fault {
                WholeImportFault::PackedSprite => path.ends_with("assets/skin/packed.png"),
                WholeImportFault::Catalog => path.ends_with("assets/catalog.json"),
                WholeImportFault::Manifest => path.ends_with("profile.json"),
                _ => false,
            }
        }

        fn directory_failure_target(&self, path: &Path) -> bool {
            if !self.stage_path(path) {
                return false;
            }
            match self.fault {
                WholeImportFault::SkinDirectory => path.ends_with("assets/skin"),
                // This subdirectory is created while migrating the prior profile's saves,
                // not by import_image's direct stage/data initialization.
                WholeImportFault::MigrationDirectory => path.ends_with("data/saves"),
                _ => false,
            }
        }
    }

    impl WriteBackend for FaultBackend {
        type File = FaultFile;

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            if self.directory_failure_target(path) {
                self.fired = true;
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            NativeWriteBackend.create_dir_all(path)
        }

        fn create_file(&mut self, path: &Path) -> io::Result<Self::File> {
            let fail_after = if self.partial_write_target(path) {
                self.fired = true;
                Some(17)
            } else {
                None
            };
            Ok(FaultFile {
                file: NativeWriteBackend.create_file(path)?,
                fail_after,
                written: 0,
            })
        }
    }

    impl CopyBackend for FaultBackend {
        fn copy(&mut self, source: &Path, destination: &Path) -> io::Result<u64> {
            let count = fs::copy(source, destination)?;
            if matches!(self.fault, WholeImportFault::MigrationCopy) {
                self.fired = true;
                return Err(io::Error::other("injected migration copy failure"));
            }
            Ok(count)
        }
    }

    impl PublishBackend for FaultBackend {
        fn rename(&mut self, stage: &Path, destination: &Path) -> io::Result<()> {
            match self.fault {
                WholeImportFault::RenameBefore => {
                    self.fired = true;
                    Err(io::Error::other("injected pre-rename failure"))
                }
                WholeImportFault::RenameAfter => {
                    fs::rename(stage, destination)?;
                    self.fired = true;
                    Err(io::Error::other("injected post-rename failure"))
                }
                _ => fs::rename(stage, destination),
            }
        }

        fn destination_exists(&mut self, destination: &Path) -> bool {
            fs::symlink_metadata(destination).is_ok()
        }
    }

    impl WriteBackend for CancellationBackend {
        type File = File;

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            NativeWriteBackend.create_dir_all(path)
        }

        fn create_file(&mut self, path: &Path) -> io::Result<Self::File> {
            if matches!(self.point, CancellationBackendPoint::ManifestCreate)
                && path.ends_with("profile.json")
            {
                assert!(!self.fired, "manifest cancellation was requested twice");
                self.fired = true;
                assert_eq!(
                    self.control.cancel(),
                    crate::frontend::import_lifecycle::CancelResult::Requested
                );
            }
            NativeWriteBackend.create_file(path)
        }
    }

    impl CopyBackend for CancellationBackend {
        fn copy(&mut self, source: &Path, destination: &Path) -> io::Result<u64> {
            let copied = fs::copy(source, destination)?;
            if matches!(self.point, CancellationBackendPoint::MigrationCopy) {
                assert!(!self.fired, "migration cancellation was requested twice");
                self.fired = true;
                assert_eq!(
                    self.control.cancel(),
                    crate::frontend::import_lifecycle::CancelResult::Requested
                );
            }
            Ok(copied)
        }
    }

    impl PublishBackend for CancellationBackend {
        fn rename(&mut self, stage: &Path, destination: &Path) -> io::Result<()> {
            if matches!(self.point, CancellationBackendPoint::Rename) {
                assert!(!self.fired, "rename cancellation was requested twice");
                self.fired = true;
                assert_eq!(
                    self.control.cancel(),
                    crate::frontend::import_lifecycle::CancelResult::TooLate
                );
            }
            fs::rename(stage, destination)
        }

        fn destination_exists(&mut self, destination: &Path) -> bool {
            fs::symlink_metadata(destination).is_ok()
        }
    }

    impl WriteBackend for RestartExitBackend {
        type File = File;

        fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
            NativeWriteBackend.create_dir_all(path)
        }

        fn create_file(&mut self, path: &Path) -> io::Result<Self::File> {
            NativeWriteBackend.create_file(path)
        }
    }

    impl CopyBackend for RestartExitBackend {
        fn copy(&mut self, source: &Path, destination: &Path) -> io::Result<u64> {
            fs::copy(source, destination)
        }
    }

    impl PublishBackend for RestartExitBackend {
        fn rename(&mut self, stage: &Path, destination: &Path) -> io::Result<()> {
            if self.phase == RestartExitPhase::BeforePublication {
                std::process::exit(77);
            }
            fs::rename(stage, destination)?;
            std::process::exit(77);
        }

        fn destination_exists(&mut self, destination: &Path) -> bool {
            fs::symlink_metadata(destination).is_ok()
        }
    }

    impl WriteBackend for MigrationCopyFailure {
        type File = File;

        fn create_dir_all(&mut self, path: &Path) -> std::io::Result<()> {
            NativeWriteBackend.create_dir_all(path)
        }

        fn create_file(&mut self, path: &Path) -> std::io::Result<Self::File> {
            NativeWriteBackend.create_file(path)
        }
    }

    impl CopyBackend for MigrationCopyFailure {
        fn copy(&mut self, _source: &Path, _destination: &Path) -> std::io::Result<u64> {
            Err(std::io::Error::other("synthetic migration copy failure"))
        }
    }

    impl WriteBackend for MigrationDirectoryFailure {
        type File = File;

        fn create_dir_all(&mut self, path: &Path) -> std::io::Result<()> {
            if path.ends_with("data/a") {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                NativeWriteBackend.create_dir_all(path)
            }
        }

        fn create_file(&mut self, path: &Path) -> std::io::Result<Self::File> {
            NativeWriteBackend.create_file(path)
        }
    }

    impl CopyBackend for MigrationDirectoryFailure {
        fn copy(&mut self, source: &Path, destination: &Path) -> std::io::Result<u64> {
            fs::copy(source, destination)
        }
    }

    impl PublishBackend for RenameFault {
        fn rename(&mut self, stage: &Path, destination: &Path) -> std::io::Result<()> {
            let error = || {
                std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "synthetic rename failure",
                )
            };
            if self.after_rename {
                fs::rename(stage, destination)?;
            }
            Err(error())
        }

        fn destination_exists(&mut self, destination: &Path) -> bool {
            fs::symlink_metadata(destination).is_ok()
        }
    }

    fn synthetic_profile(directory: &Path, sha: &str) -> Profile {
        let assets = directory.join("assets");
        fs::create_dir_all(assets.join("skin")).unwrap();
        let catalog = assets.join("catalog.json");
        let layout = assets.join("skin/layout.json");
        fs::write(&catalog, b"synthetic catalog").unwrap();
        fs::write(&layout, b"synthetic layout").unwrap();
        let files = json!({
            "assets/catalog.json": file_digest(&catalog).unwrap(),
            "assets/skin/layout.json": file_digest(&layout).unwrap(),
        });
        Profile {
            directory: directory.to_path_buf(),
            manifest: json!({
                "importerVersion": IMPORTER_VERSION,
                "sourceSHA256": sha,
                "files": files,
            }),
        }
    }

    fn fixture_profile(cache: &Path, sha: &str) -> Profile {
        let profile = synthetic_profile(&cache.join(format!("{sha}-v{IMPORTER_VERSION}")), sha);
        write_json(&profile.directory.join("profile.json"), &profile.manifest).unwrap();
        assert!(valid_profile(&profile));
        profile
    }

    fn seed_previous_profile(cache: &Path, sha: &str, marker: &str) -> Profile {
        let previous_version = IMPORTER_VERSION - 1;
        assert!([4, 5, 6].contains(&previous_version));
        let directory = cache.join(format!("{sha}-v{previous_version}"));
        let mut profile = synthetic_profile(&directory, sha);
        profile.manifest["importerVersion"] = json!(previous_version);
        let firmware = path_string(&directory.join("assets/firmware"));
        fs::create_dir_all(directory.join("data/saves")).unwrap();
        write_json(
            &directory.join("data/preferences.json"),
            &json!({
                "firmware": firmware,
                "theme": "classic",
                "preserve": marker,
            }),
        )
        .unwrap();
        fs::write(
            directory.join("data/saves/sentinel"),
            format!("migration sentinel: {marker}"),
        )
        .unwrap();
        write_json(&directory.join("profile.json"), &profile.manifest).unwrap();
        assert!(valid_profile(&profile));
        profile
    }

    fn temporary(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("clover-profile-{name}-{now}"))
    }

    struct EndlessReader {
        bytes_read: usize,
    }

    impl Read for EndlessReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            buffer.fill(b'!');
            self.bytes_read += buffer.len();
            Ok(buffer.len())
        }
    }

    struct InterruptOnceReader {
        interrupted: bool,
        bytes: io::Cursor<Vec<u8>>,
    }

    impl Read for InterruptOnceReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.bytes.read(buffer)
        }
    }

    struct ErrorAfterPartialReader {
        first_read: bool,
    }

    impl Read for ErrorAfterPartialReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.first_read {
                self.first_read = true;
                buffer[0] = b'{';
                return Ok(1);
            }
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "synthetic read failure",
            ))
        }
    }

    #[test]
    fn bounded_json_reader_accepts_exact_limit_and_rejects_limit_plus_one() {
        let exact = b"  {}\n";
        assert_eq!(
            serde_json::from_slice::<Value>(&read_bounded(&mut &exact[..], 5).unwrap()).unwrap(),
            json!({})
        );

        let mut oversized = EndlessReader { bytes_read: 0 };
        let bytes = read_bounded(&mut oversized, 7).unwrap();
        assert_eq!(bytes.len(), 8);
        assert_eq!(oversized.bytes_read, 8);
        assert!(bytes.len() as u64 > 7);

        let mut oversized = EndlessReader { bytes_read: 0 };
        assert_eq!(
            read_json_from(&mut oversized, 7).unwrap_err(),
            "Invalid profile manifest"
        );
        assert_eq!(oversized.bytes_read, 8);
    }

    #[test]
    fn bounded_json_reader_retries_interrupted_reads_and_reports_real_errors() {
        let mut partial = InterruptOnceReader {
            interrupted: false,
            bytes: io::Cursor::new(b"{}".to_vec()),
        };
        assert_eq!(read_bounded(&mut partial, 2).unwrap(), b"{}");

        let error =
            read_bounded(&mut ErrorAfterPartialReader { first_read: false }, 20).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "synthetic read failure");

        let error =
            read_json_from(&mut ErrorAfterPartialReader { first_read: false }, 20).unwrap_err();
        assert!(error.starts_with("Cannot read profile manifest: synthetic read failure"));
    }

    #[test]
    fn bounded_json_reader_handles_max_limit_without_overflow_or_large_allocation() {
        let bytes = read_bounded(&mut &b"{}"[..], u64::MAX).unwrap();
        assert_eq!(bytes, b"{}");
        assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), json!({}));
    }

    #[test]
    fn read_json_preserves_native_validation_and_parse_errors() {
        let root = temporary("read-json");
        fs::create_dir_all(&root).unwrap();

        let exact = root.join("exact.json");
        fs::write(&exact, b"  {}\n").unwrap();
        assert_eq!(read_json(&exact, 5).unwrap(), json!({}));

        let oversized = root.join("oversized.json");
        fs::write(&oversized, b" {}\n").unwrap();
        assert_eq!(
            read_json(&oversized, 3).unwrap_err(),
            "Invalid profile manifest"
        );

        let invalid_utf8 = root.join("invalid-utf8.json");
        fs::write(&invalid_utf8, [b'{', 0xff, b'}']).unwrap();
        assert!(
            read_json(&invalid_utf8, 3)
                .unwrap_err()
                .starts_with("Invalid profile manifest JSON:")
        );

        let invalid_json = root.join("invalid-json.json");
        fs::write(&invalid_json, b"{").unwrap();
        assert!(
            read_json(&invalid_json, 1)
                .unwrap_err()
                .starts_with("Invalid profile manifest JSON:")
        );

        let missing = root.join("missing.json");
        assert!(
            read_json(&missing, 10)
                .unwrap_err()
                .starts_with("Cannot read profile manifest:")
        );
        let directory = root.join("directory.json");
        fs::create_dir(&directory).unwrap();
        assert_eq!(
            read_json(&directory, 10).unwrap_err(),
            "Invalid profile manifest"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = root.join("link.json");
            symlink(&exact, &link).unwrap();
            assert_eq!(
                read_json(&link, 10).unwrap_err(),
                "Invalid profile manifest"
            );
        }

        fs::remove_dir_all(root).unwrap();
    }

    struct StageCreationBackend {
        collisions: usize,
        calls: usize,
        paths: Vec<PathBuf>,
        fail_early: bool,
    }

    impl StageBackend for StageCreationBackend {
        fn create_dir(&mut self, path: &Path) -> io::Result<()> {
            self.calls += 1;
            self.paths.push(path.to_path_buf());
            if self.fail_early {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            if self.calls <= self.collisions {
                fs::create_dir(path)?;
                fs::write(path.join("sentinel"), b"existing candidate")?;
                return Err(io::Error::from(io::ErrorKind::AlreadyExists));
            }
            fs::create_dir(path)
        }
    }

    fn assert_stage_name(path: &Path, cache: &Path, sha: &str) {
        let name = path.file_name().unwrap().to_str().unwrap();
        let prefix = format!(".import-{}-", &sha[..12]);
        let suffix = name.strip_prefix(&prefix).unwrap();
        let (timestamp, serial) = suffix.split_once('-').unwrap();
        assert!(!timestamp.is_empty() && timestamp.bytes().all(|byte| byte.is_ascii_digit()));
        assert!(!serial.is_empty() && serial.bytes().all(|byte| byte.is_ascii_digit()));
        assert_eq!(path.parent(), Some(cache));
    }

    #[test]
    fn stage_creation_collision_does_not_acquire_or_cleanup_existing_path() {
        let root = temporary("stage-collision");
        let cache = root.join("cache");
        let published = root.join("published-profile");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir(&published).unwrap();
        fs::write(published.join("profile.json"), b"published profile").unwrap();
        fs::write(published.join("userdata"), b"published userdata").unwrap();
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let mut backend = StageCreationBackend {
            collisions: 1,
            calls: 0,
            paths: Vec::new(),
            fail_early: false,
        };

        let stage = create_stage_with(&cache, sha, &mut backend).unwrap();
        assert_eq!(backend.calls, 2);
        assert_eq!(backend.paths.len(), 2);
        assert_stage_name(&backend.paths[0], &cache, sha);
        assert_stage_name(&backend.paths[1], &cache, sha);
        assert_eq!(stage.path(), backend.paths[1]);
        assert_eq!(
            fs::read(backend.paths[0].join("sentinel")).unwrap(),
            b"existing candidate"
        );
        assert!(backend.paths[1].is_dir());
        drop(stage);

        assert!(backend.paths[0].is_dir());
        assert_eq!(
            fs::read(backend.paths[0].join("sentinel")).unwrap(),
            b"existing candidate"
        );
        assert!(!backend.paths[1].exists());
        assert_eq!(
            fs::read(published.join("profile.json")).unwrap(),
            b"published profile"
        );
        assert_eq!(
            fs::read(published.join("userdata")).unwrap(),
            b"published userdata"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stage_creation_exhaustion_preserves_all_collision_candidates() {
        let root = temporary("stage-exhaustion");
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let sha = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let mut backend = StageCreationBackend {
            collisions: 20,
            calls: 0,
            paths: Vec::new(),
            fail_early: false,
        };

        let error = create_stage_with(&cache, sha, &mut backend).err().unwrap();
        assert_eq!(error, "Cannot allocate unique staging profile directory");
        assert_eq!(backend.calls, 20);
        assert_eq!(backend.paths.len(), 20);
        for path in &backend.paths {
            assert_stage_name(path, &cache, sha);
            assert_eq!(
                fs::read(path.join("sentinel")).unwrap(),
                b"existing candidate"
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stage_creation_permission_error_is_not_retried_or_cleaned() {
        let root = temporary("stage-permission-error");
        let cache = root.join("cache");
        let published = root.join("published-profile");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir(&published).unwrap();
        fs::write(published.join("profile.json"), b"published profile").unwrap();
        fs::write(published.join("userdata"), b"published userdata").unwrap();
        let sha = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
        let mut backend = StageCreationBackend {
            collisions: 0,
            calls: 0,
            paths: Vec::new(),
            fail_early: true,
        };

        let error = create_stage_with(&cache, sha, &mut backend).err().unwrap();
        assert!(
            error.starts_with("Cannot create staging profile:"),
            "{error}"
        );
        assert_eq!(backend.calls, 1);
        assert_eq!(backend.paths.len(), 1);
        assert!(!backend.paths[0].exists());
        assert_eq!(
            fs::read(published.join("profile.json")).unwrap(),
            b"published profile"
        );
        assert_eq!(
            fs::read(published.join("userdata")).unwrap(),
            b"published userdata"
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn profile_tree_digest(directory: &Path) -> String {
        fn visit(directory: &Path, current: &Path, entries: &mut Vec<(String, String)>) {
            for item in fs::read_dir(current).unwrap() {
                let item = item.unwrap();
                let path = item.path();
                if item.file_type().unwrap().is_dir() {
                    visit(directory, &path, entries);
                } else {
                    let relative = path.strip_prefix(directory).unwrap();
                    entries.push((
                        relative.to_string_lossy().replace('\\', "/"),
                        file_digest(&path).unwrap(),
                    ));
                }
            }
        }
        let mut entries = Vec::new();
        visit(directory, directory, &mut entries);
        entries.sort();
        digest(&serde_json::to_vec(&entries).unwrap())
    }

    fn assert_no_import_stages(cache: &Path) {
        assert!(
            fs::read_dir(cache).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".import-")),
            "native stage cleanup left an import staging directory"
        );
    }

    struct ImportInvariantSnapshot<'a> {
        control: &'a crate::import_job::ImportControl,
        cache: &'a Path,
        destination: &'a Path,
        old_profile: &'a Profile,
        old_digest: &'a str,
        source: &'a Path,
        source_hash: &'a str,
    }

    fn assert_cancelled_import_invariants(
        result: Result<Profile, crate::import_job::ImportError>,
        snapshot: &ImportInvariantSnapshot<'_>,
    ) {
        assert!(matches!(
            result,
            Err(crate::import_job::ImportError::Cancelled)
        ));
        assert_eq!(
            snapshot.control.state(),
            crate::frontend::import_lifecycle::State::Cancelled
        );
        assert!(!snapshot.destination.exists());
        assert_no_import_stages(snapshot.cache);
        assert!(valid_profile(snapshot.old_profile));
        assert_eq!(
            profile_tree_digest(&snapshot.old_profile.directory),
            snapshot.old_digest
        );
        assert_eq!(file_digest(snapshot.source).unwrap(), snapshot.source_hash);
    }

    fn import_stages(cache: &Path) -> Vec<PathBuf> {
        fs::read_dir(cache)
            .unwrap()
            .map(Result::unwrap)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with(".import-")
            })
            .collect()
    }

    fn run_restart_child(
        source: &Path,
        parent_root: &Path,
        cache: &Path,
        phase: &str,
    ) -> std::process::ExitStatus {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "profile::tests::whole_import_restart_child",
                "--ignored",
            ])
            .env("CLOVER_IMPORT_RESTART_SOURCE", source)
            .env("CLOVER_IMPORT_RESTART_ROOT", parent_root)
            .env("CLOVER_IMPORT_RESTART_CACHE", cache)
            .env("CLOVER_IMPORT_RESTART_PHASE", phase)
            .status()
            .expect("could not start restart child test")
    }

    fn assert_migrated_restart_data(profile: &Profile, expected_root: &Path, marker: &str) {
        assert_eq!(
            fs::read(profile.directory.join("data/saves/sentinel")).unwrap(),
            format!("migration sentinel: {marker}").as_bytes()
        );
        let settings: Value = read_json(
            &profile.directory.join("data/preferences.json"),
            32 * 1024 * 1024,
        )
        .unwrap();
        assert_eq!(settings["theme"], "classic");
        assert_eq!(settings["preserve"], marker);
        assert_eq!(
            settings["firmware"],
            path_string(&expected_root.join("assets/firmware"))
        );
    }

    #[test]
    #[ignore = "parent for whole_import_restart_child; requires CLOVER_IMPORT_FAULT_IMAGE local image"]
    fn whole_import_process_restart_matrix() {
        let source = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_FAULT_IMAGE")
                .expect("CLOVER_IMPORT_FAULT_IMAGE must name a local image"),
        );
        assert!(
            regular_not_symlink(&source),
            "fault image must be a regular file"
        );
        let source_hash = file_digest(&source).unwrap();
        let sha = file_digest(&resolve_filesystem(&source).unwrap()).unwrap();
        let root = temporary("whole-import-process-restart-matrix");
        fs::create_dir_all(&root).unwrap();

        let before_cache = root.join("before/cache");
        fs::create_dir_all(&before_cache).unwrap();
        let before_old = seed_previous_profile(&before_cache, &sha, "restart-before");
        let before_old_digest = profile_tree_digest(&before_old.directory);
        let before_destination = before_cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
        let status = run_restart_child(&source, &root, &before_cache, "before");
        assert_eq!(
            status.code(),
            Some(77),
            "pre-publication child did not exit at hook"
        );
        assert!(!before_destination.exists());
        let profiles = list_profiles(&before_cache);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].directory, before_old.directory);
        assert!(valid_profile(&profiles[0]));
        let before_orphans = import_stages(&before_cache);
        assert_eq!(
            before_orphans.len(),
            1,
            "process exit should bypass stage Drop"
        );
        let orphan_manifest =
            read_json(&before_orphans[0].join("profile.json"), 32 * 1024 * 1024).unwrap();
        assert_eq!(
            orphan_manifest["importerVersion"].as_u64(),
            Some(u64::from(IMPORTER_VERSION))
        );
        let orphan_files = orphan_manifest["files"].as_object().unwrap();
        assert!(!orphan_files.is_empty());
        for (relative, expected_hash) in orphan_files {
            assert_eq!(
                file_digest(&before_orphans[0].join(relative)).unwrap(),
                expected_hash.as_str().unwrap()
            );
        }
        let orphan_profile = Profile {
            directory: before_orphans[0].clone(),
            manifest: orphan_manifest,
        };
        assert_migrated_restart_data(&orphan_profile, &before_destination, "restart-before");
        assert!(valid_profile(&before_old));
        assert_eq!(
            profile_tree_digest(&before_old.directory),
            before_old_digest
        );
        assert_eq!(file_digest(&source).unwrap(), source_hash);

        let restarted = import_image(&source, &before_cache, None).unwrap();
        assert!(valid_profile(&restarted));
        assert_eq!(restarted.directory, before_destination);
        assert_migrated_restart_data(&restarted, &before_destination, "restart-before");
        let profiles = list_profiles(&before_cache);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].directory, before_destination);
        assert!(valid_profile(&profiles[0]));
        assert_eq!(import_stages(&before_cache), before_orphans);
        assert!(valid_profile(&before_old));
        assert_eq!(
            profile_tree_digest(&before_old.directory),
            before_old_digest
        );
        assert_eq!(file_digest(&source).unwrap(), source_hash);

        let after_cache = root.join("after/cache");
        fs::create_dir_all(&after_cache).unwrap();
        let after_old = seed_previous_profile(&after_cache, &sha, "restart-after");
        let after_old_digest = profile_tree_digest(&after_old.directory);
        let after_destination = after_cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
        let status = run_restart_child(&source, &root, &after_cache, "after");
        assert_eq!(
            status.code(),
            Some(77),
            "post-publication child did not exit at hook"
        );

        let published = Profile {
            directory: after_destination.clone(),
            manifest: read_json(&after_destination.join("profile.json"), 32 * 1024 * 1024).unwrap(),
        };
        assert!(valid_profile(&published));
        assert_migrated_restart_data(&published, &after_destination, "restart-after");
        let profiles = list_profiles(&after_cache);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].directory, after_destination);
        assert!(valid_profile(&profiles[0]));
        let manifest_path = after_destination.join("profile.json");
        let manifest_hash = file_digest(&manifest_path).unwrap();
        let manifest_modified = fs::metadata(&manifest_path).unwrap().modified().unwrap();

        let (restarted, progress) = {
            let mut progress = Vec::new();
            let profile = import_image(
                &source,
                &after_cache,
                Some(&mut |message| {
                    progress.push(message.to_owned());
                }),
            )
            .unwrap();
            (profile, progress)
        };
        assert!(valid_profile(&restarted));
        assert_eq!(restarted.directory, after_destination);
        assert!(
            progress
                .iter()
                .any(|message| message == "Using verified cache")
        );
        assert_eq!(file_digest(&manifest_path).unwrap(), manifest_hash);
        assert_eq!(
            fs::metadata(&manifest_path).unwrap().modified().unwrap(),
            manifest_modified
        );
        assert_migrated_restart_data(&restarted, &after_destination, "restart-after");
        let profiles = list_profiles(&after_cache);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].directory, after_destination);
        assert!(valid_profile(&profiles[0]));
        assert!(valid_profile(&after_old));
        assert_eq!(profile_tree_digest(&after_old.directory), after_old_digest);
        assert_eq!(file_digest(&source).unwrap(), source_hash);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "child for whole_import_process_restart_matrix; launched with parent-owned paths"]
    fn whole_import_restart_child() {
        let source = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_RESTART_SOURCE")
                .expect("child requires CLOVER_IMPORT_RESTART_SOURCE"),
        );
        let parent_root = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_RESTART_ROOT")
                .expect("child requires CLOVER_IMPORT_RESTART_ROOT"),
        );
        let cache = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_RESTART_CACHE")
                .expect("child requires CLOVER_IMPORT_RESTART_CACHE"),
        );
        let phase = match std::env::var("CLOVER_IMPORT_RESTART_PHASE").as_deref() {
            Ok("before") => RestartExitPhase::BeforePublication,
            Ok("after") => RestartExitPhase::AfterPublication,
            _ => panic!("child requires a valid CLOVER_IMPORT_RESTART_PHASE"),
        };
        assert!(
            regular_not_symlink(&source),
            "child source must be a regular file"
        );
        assert!(parent_root.is_dir(), "child parent root must exist");
        assert!(
            cache.starts_with(&parent_root),
            "child cache escaped parent root"
        );

        let mut backend = RestartExitBackend { phase };
        let result = import_image_with_backend(&source, &cache, None, &mut backend);
        panic!("restart child returned normally instead of exiting at publish: {result:?}");
    }

    #[test]
    #[ignore = "requires CLOVER_IMPORT_FAULT_IMAGE local image"]
    fn whole_import_source_fault_matrix() {
        use crate::frontend::{import_lifecycle::CancelResult, import_lifecycle::State};

        let source = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_FAULT_IMAGE")
                .expect("CLOVER_IMPORT_FAULT_IMAGE must name a local image"),
        );
        assert!(
            regular_not_symlink(&source),
            "fault image must be a regular file"
        );
        let source_hash = file_digest(&source).unwrap();
        let filesystem = resolve_filesystem(&source).unwrap();
        let sha = file_digest(&filesystem).unwrap();
        let root = temporary("whole-import-source-fault-matrix");

        let cases = [
            (
                "initial-hash",
                SourceFault::InitialHash,
                "Cannot hash filesystem image: injected source digest failure",
                1,
                0,
                false,
            ),
            (
                "parser",
                SourceFault::Parser,
                "Image reader: injected parser error",
                1,
                1,
                false,
            ),
            (
                "filesystem-rehash",
                SourceFault::FilesystemRehash,
                "Cannot rehash filesystem image: injected source digest failure",
                2,
                1,
                false,
            ),
            (
                "filesystem-rehash-mismatch",
                SourceFault::FilesystemRehashMismatch,
                "Source image changed during import",
                2,
                1,
                false,
            ),
            (
                "selected-source-hash",
                SourceFault::SelectedSourceHash,
                "Cannot hash selected image: injected source digest failure",
                3,
                1,
                false,
            ),
            (
                "parser-after-cancel-request",
                SourceFault::ParserAfterCancel,
                "Image reader: injected parser error",
                1,
                1,
                true,
            ),
        ];

        for (name, fault, expected_error, expected_hash_calls, expected_reads, cancellation) in
            cases
        {
            let cache = root.join(name);
            fs::create_dir_all(&cache).unwrap();
            let old_profile = seed_previous_profile(&cache, &sha, name);
            let old_digest = profile_tree_digest(&old_profile.directory);
            let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
            let control = crate::import_job::ImportControl::new();
            let mut backend = SourceFaultBackend {
                fault,
                control: control.clone(),
                hash_calls: 0,
                read_calls: 0,
                fired: false,
                cancel_result: None,
            };

            let result =
                import_image_controlled_with_backend(&source, &cache, None, &mut backend, &control);

            assert!(backend.fired, "source fault did not fire: {name}");
            assert_eq!(backend.hash_calls, expected_hash_calls, "{name}");
            assert_eq!(backend.read_calls, expected_reads, "{name}");
            assert!(
                matches!(&result, Err(crate::import_job::ImportError::Failed(message)) if message == expected_error),
                "{name}: unexpected result {result:?}"
            );
            assert_eq!(control.state(), State::Failed, "{name}");
            assert_eq!(
                backend.cancel_result,
                cancellation.then_some(CancelResult::Requested),
                "{name}"
            );
            assert!(!destination.exists(), "{name} published a profile");
            assert_no_import_stages(&cache);
            assert!(
                valid_profile(&old_profile),
                "{name} invalidated prior profile"
            );
            assert_eq!(
                profile_tree_digest(&old_profile.directory),
                old_digest,
                "{name} changed prior profile settings or saves"
            );
            assert_eq!(
                file_digest(&source).unwrap(),
                source_hash,
                "{name} changed source"
            );
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires CLOVER_IMPORT_FAULT_IMAGE local image"]
    fn whole_import_migration_read_fault_matrix() {
        use crate::frontend::import_lifecycle::State;

        let source = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_FAULT_IMAGE")
                .expect("CLOVER_IMPORT_FAULT_IMAGE must name a local image"),
        );
        assert!(regular_not_symlink(&source));
        let source_hash = file_digest(&source).unwrap();
        let sha = file_digest(&resolve_filesystem(&source).unwrap()).unwrap();
        let root = temporary("whole-import-migration-read-fault-matrix");

        for (name, fault, expected) in [
            (
                "directory-open",
                MigrationScanFault::ReadDir,
                "Cannot read previous profile data:",
            ),
            (
                "iterator-entry",
                MigrationScanFault::Entry,
                "Cannot read previous profile data:",
            ),
            (
                "entry-metadata",
                MigrationScanFault::Metadata,
                "Cannot inspect previous profile data:",
            ),
        ] {
            let cache = root.join(name);
            fs::create_dir_all(&cache).unwrap();
            let old = seed_previous_profile(&cache, &sha, name);
            let old_digest = profile_tree_digest(&old.directory);
            let protected_files = [
                "profile.json",
                "assets/catalog.json",
                "assets/skin/layout.json",
                "data/preferences.json",
                "data/saves/sentinel",
            ]
            .map(|relative| {
                let path = old.directory.join(relative);
                (path.clone(), file_digest(&path).unwrap())
            });
            let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
            let control = ImportControl::new();
            let mut scanner = FaultMigrationScanner {
                fault,
                fired: false,
            };

            let result = import_image_controlled_with_scanner(
                &source,
                &cache,
                None,
                &mut NativeImportBackend,
                &control,
                &mut scanner,
            );

            assert!(scanner.fired, "migration scan fault did not fire: {name}");
            assert!(
                matches!(&result, Err(ImportError::Failed(message)) if message.starts_with(expected)),
                "{name}: unexpected import result {result:?}"
            );
            assert_eq!(control.state(), State::Failed, "{name}");
            assert!(!destination.exists(), "{name} published a profile");
            assert_no_import_stages(&cache);
            assert!(valid_profile(&old), "{name} invalidated old profile");
            assert_eq!(profile_tree_digest(&old.directory), old_digest, "{name}");
            for (path, expected_hash) in protected_files {
                assert_eq!(
                    file_digest(&path).unwrap(),
                    expected_hash,
                    "{name}: {path:?}"
                );
            }
            assert_eq!(file_digest(&source).unwrap(), source_hash, "{name}");
        }

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires CLOVER_IMPORT_FAULT_IMAGE local image"]
    fn whole_import_filesystem_fault_matrix() {
        let source = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_FAULT_IMAGE")
                .expect("CLOVER_IMPORT_FAULT_IMAGE must name a local image"),
        );
        assert!(
            regular_not_symlink(&source),
            "fault image must be a regular file"
        );
        let source_hash = file_digest(&source).unwrap();
        let root = temporary("whole-import-fault-matrix");
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let sha = file_digest(&resolve_filesystem(&source).unwrap()).unwrap();
        let old_profile = seed_previous_profile(&cache, &sha, "fault-matrix");
        let old = old_profile.directory.clone();
        let old_digest = profile_tree_digest(&old);
        let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));

        for (fault, context) in [
            (
                WholeImportFault::SkinDirectory,
                "Cannot create skin output directory:",
            ),
            (WholeImportFault::PackedSprite, "Cannot write decoded skin:"),
            (WholeImportFault::Catalog, "Cannot write profile file:"),
            (WholeImportFault::Manifest, "Cannot write profile file:"),
            (
                WholeImportFault::MigrationDirectory,
                "Cannot migrate previous profile data:",
            ),
            (
                WholeImportFault::MigrationCopy,
                "Cannot migrate previous profile data:",
            ),
            (
                WholeImportFault::RenameBefore,
                "Cannot publish imported profile:",
            ),
        ] {
            assert!(!destination.exists());
            let mut backend = FaultBackend {
                cache: cache.clone(),
                fault,
                fired: false,
            };
            let error = import_image_with_backend(&source, &cache, None, &mut backend).unwrap_err();
            assert!(backend.fired, "injected fault did not fire: {fault:?}");
            assert!(error.starts_with(context), "{fault:?}: {error}");
            assert!(!destination.exists(), "{fault:?} published a profile");
            assert_no_import_stages(&cache);
            assert!(
                valid_profile(&old_profile),
                "{fault:?} invalidated old profile"
            );
            assert_eq!(
                profile_tree_digest(&old),
                old_digest,
                "{fault:?} changed old profile"
            );
            assert_eq!(
                file_digest(&source).unwrap(),
                source_hash,
                "{fault:?} changed source"
            );
        }

        let mut backend = FaultBackend {
            cache: cache.clone(),
            fault: WholeImportFault::RenameAfter,
            fired: false,
        };
        let imported = import_image_with_backend(&source, &cache, None, &mut backend).unwrap();
        assert!(backend.fired, "post-rename fault did not fire");
        assert!(valid_profile(&imported));
        assert_eq!(imported.directory, destination);
        let profiles = list_profiles(&cache);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].directory, destination);
        assert_eq!(
            fs::read(destination.join("data/saves/sentinel")).unwrap(),
            b"migration sentinel: fault-matrix"
        );
        let migrated_settings: Value =
            read_json(&destination.join("data/preferences.json"), 32 * 1024 * 1024).unwrap();
        assert_eq!(migrated_settings["theme"], "classic");
        assert_eq!(
            migrated_settings["firmware"],
            path_string(&destination.join("assets/firmware"))
        );
        assert_no_import_stages(&cache);
        assert!(valid_profile(&old_profile));
        assert_eq!(profile_tree_digest(&old), old_digest);
        assert_eq!(file_digest(&source).unwrap(), source_hash);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires CLOVER_IMPORT_FAULT_IMAGE local image"]
    fn whole_import_cancellation_matrix() {
        use crate::{
            frontend::import_lifecycle::{CancelResult, State},
            import_job::ImportControl,
        };

        #[derive(Clone, Copy)]
        enum ProgressPoint {
            Hashing,
            Reading,
            Decoding,
            FirstPreparing,
            Preserving,
        }

        fn matches_progress(message: &str, point: ProgressPoint) -> bool {
            match point {
                ProgressPoint::Hashing => message == "Hashing filesystem image...",
                ProgressPoint::Reading => {
                    message == "Reading image (no extracted code is executed)..."
                }
                ProgressPoint::Decoding => message == "Decoding Clover artwork and fonts...",
                ProgressPoint::FirstPreparing => message.starts_with("Preparing "),
                ProgressPoint::Preserving => {
                    message == "Preserving settings and saves from the previous importer..."
                }
            }
        }

        let source = PathBuf::from(
            std::env::var_os("CLOVER_IMPORT_FAULT_IMAGE")
                .expect("CLOVER_IMPORT_FAULT_IMAGE must name a local image"),
        );
        assert!(regular_not_symlink(&source));
        let source_hash = file_digest(&source).unwrap();
        let sha = file_digest(&resolve_filesystem(&source).unwrap()).unwrap();
        let root = temporary("whole-import-cancellation-matrix");
        fs::create_dir_all(&root).unwrap();

        // Checkpoints acknowledge cancellation after reports, not during the
        // filesystem hash/read, skin preparation, conversion, or migration copy.
        for (name, point) in [
            ("hashing", ProgressPoint::Hashing),
            ("reading", ProgressPoint::Reading),
            ("decoding", ProgressPoint::Decoding),
            ("preparing", ProgressPoint::FirstPreparing),
            ("preserving", ProgressPoint::Preserving),
        ] {
            let cache = root.join(name);
            fs::create_dir_all(&cache).unwrap();
            let old_profile = seed_previous_profile(&cache, &sha, name);
            let old_digest = profile_tree_digest(&old_profile.directory);
            let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
            let control = ImportControl::new();
            let callback_control = control.clone();
            let mut fired = false;
            let mut progress = |message: &str| {
                if !fired && matches_progress(message, point) {
                    fired = true;
                    assert_eq!(callback_control.cancel(), CancelResult::Requested);
                }
            };
            let result = import_image_cancellable(&source, &cache, Some(&mut progress), &control);
            assert!(fired, "progress cancellation target not reached: {name}");
            assert_cancelled_import_invariants(
                result,
                &ImportInvariantSnapshot {
                    control: &control,
                    cache: &cache,
                    destination: &destination,
                    old_profile: &old_profile,
                    old_digest: &old_digest,
                    source: &source,
                    source_hash: &source_hash,
                },
            );
        }

        for (name, point) in [
            ("migration-copy", CancellationBackendPoint::MigrationCopy),
            ("manifest-create", CancellationBackendPoint::ManifestCreate),
        ] {
            let cache = root.join(name);
            fs::create_dir_all(&cache).unwrap();
            let old_profile = seed_previous_profile(&cache, &sha, name);
            let old_digest = profile_tree_digest(&old_profile.directory);
            let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
            let control = ImportControl::new();
            let mut backend = CancellationBackend {
                control: control.clone(),
                point,
                fired: false,
            };
            let result =
                import_image_controlled_with_backend(&source, &cache, None, &mut backend, &control);
            assert!(
                backend.fired,
                "backend cancellation target not reached: {name}"
            );
            assert_cancelled_import_invariants(
                result,
                &ImportInvariantSnapshot {
                    control: &control,
                    cache: &cache,
                    destination: &destination,
                    old_profile: &old_profile,
                    old_digest: &old_digest,
                    source: &source,
                    source_hash: &source_hash,
                },
            );
        }

        let cache = root.join("rename-boundary");
        fs::create_dir_all(&cache).unwrap();
        let old_profile = seed_previous_profile(&cache, &sha, "rename-boundary");
        let old_digest = profile_tree_digest(&old_profile.directory);
        let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
        let control = ImportControl::new();
        let mut backend = CancellationBackend {
            control: control.clone(),
            point: CancellationBackendPoint::Rename,
            fired: false,
        };
        let result =
            import_image_controlled_with_backend(&source, &cache, None, &mut backend, &control)
                .unwrap();
        assert!(
            backend.fired,
            "final rename cancellation target not reached"
        );
        assert!(valid_profile(&result));
        assert_eq!(result.directory, destination);
        assert_eq!(control.state(), State::Completed);
        assert_no_import_stages(&cache);
        assert!(valid_profile(&old_profile));
        assert_eq!(profile_tree_digest(&old_profile.directory), old_digest);
        assert_eq!(file_digest(&source).unwrap(), source_hash);
        assert_migrated_restart_data(&result, &destination, "rename-boundary");

        fs::remove_dir_all(root).unwrap();
    }

    fn migration_fixture(cache: &Path, sha: &str) -> Profile {
        let previous_version = IMPORTER_VERSION - 1;
        let directory = cache.join(format!("{sha}-v{previous_version}"));
        fs::create_dir_all(directory.join("assets/skin")).unwrap();
        fs::create_dir_all(directory.join("data/a")).unwrap();
        let catalog = directory.join("assets/catalog.json");
        let layout = directory.join("assets/skin/layout.json");
        fs::write(&catalog, b"migration fixture catalog").unwrap();
        fs::write(&layout, b"migration fixture layout").unwrap();
        fs::write(directory.join("data/a/save.bin"), b"old save bytes").unwrap();
        fs::write(
            directory.join("data/preferences.json"),
            br#"{"firmware":"old/path","theme":"classic"}"#,
        )
        .unwrap();
        let profile = Profile {
            directory: directory.clone(),
            manifest: json!({
                "importerVersion": previous_version,
                "sourceSHA256": sha,
                "files": {
                    "assets/catalog.json": file_digest(&catalog).unwrap(),
                    "assets/skin/layout.json": file_digest(&layout).unwrap(),
                },
            }),
        };
        write_json(&directory.join("profile.json"), &profile.manifest).unwrap();
        assert!(valid_profile(&profile));
        profile
    }

    #[test]
    fn migration_scanner_failures_preserve_old_profile_and_drop_owned_stage() {
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        for (name, fault, expected) in [
            (
                "read-dir",
                MigrationScanFault::ReadDir,
                "Cannot read previous profile data:",
            ),
            (
                "entry",
                MigrationScanFault::Entry,
                "Cannot read previous profile data:",
            ),
            (
                "metadata",
                MigrationScanFault::Metadata,
                "Cannot inspect previous profile data:",
            ),
        ] {
            let cache = temporary(&format!("migration-scan-{name}"));
            fs::create_dir_all(&cache).unwrap();
            let old = migration_fixture(&cache, sha);
            let old_digest = profile_tree_digest(&old.directory);
            let old_save = old.directory.join("data/a/save.bin");
            let old_settings = old.directory.join("data/preferences.json");
            let save_bytes = fs::read(&old_save).unwrap();
            let settings_bytes = fs::read(&old_settings).unwrap();

            let stage_path = cache.join(".migration-scan-stage");
            fs::create_dir_all(stage_path.join("data/partial")).unwrap();
            fs::write(stage_path.join("data/partial/marker"), b"partial stage").unwrap();
            let stage = OwnedStage::new(stage_path.clone());
            let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
            let mut manifest = json!({"importerVersion": IMPORTER_VERSION});
            let mut scanner = FaultMigrationScanner {
                fault,
                fired: false,
            };
            let (result, report_calls, checkpoint_calls) = {
                let mut report_calls = 0;
                let mut checkpoint_calls = 0;
                let mut report = |_message: &str| report_calls += 1;
                let mut checkpoint = || {
                    checkpoint_calls += 1;
                    Ok(())
                };
                let mut hooks = MigrationHooks {
                    report: &mut report,
                    checkpoint: &mut checkpoint,
                };
                let result = preserve_old_data_with_scanner(
                    &cache,
                    sha,
                    stage.path(),
                    &destination,
                    &mut manifest,
                    &mut hooks,
                    &mut NativeImportBackend,
                    &mut scanner,
                );
                (result, report_calls, checkpoint_calls)
            };

            let error = result.unwrap_err().to_string();
            assert!(error.starts_with(expected), "{name}: {error}");
            assert!(scanner.fired, "{name} fault did not fire");
            assert_eq!(report_calls, 1, "{name}");
            assert!(checkpoint_calls >= 1, "{name}");
            assert!(valid_profile(&old), "{name} changed old profile");
            assert_eq!(profile_tree_digest(&old.directory), old_digest, "{name}");
            assert_eq!(fs::read(old_save).unwrap(), save_bytes, "{name}");
            assert_eq!(fs::read(old_settings).unwrap(), settings_bytes, "{name}");
            assert!(!destination.exists(), "{name} published destination");
            assert_eq!(
                fs::read(stage_path.join("data/partial/marker")).unwrap(),
                b"partial stage"
            );
            drop(stage);
            assert!(!stage_path.exists(), "{name} left owned stage behind");
            fs::remove_dir_all(cache).unwrap();
        }
    }

    #[test]
    fn migration_checkpoint_cancellation_still_drops_owned_stage() {
        let cache = temporary("migration-scan-cancellation");
        fs::create_dir_all(&cache).unwrap();
        let sha = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let old = migration_fixture(&cache, sha);
        let old_digest = profile_tree_digest(&old.directory);
        let save_bytes = fs::read(old.directory.join("data/a/save.bin")).unwrap();
        let settings_bytes = fs::read(old.directory.join("data/preferences.json")).unwrap();
        let stage_path = cache.join(".migration-cancel-stage");
        fs::create_dir_all(stage_path.join("data/partial")).unwrap();
        fs::write(stage_path.join("data/partial/marker"), b"partial stage").unwrap();
        let stage = OwnedStage::new(stage_path.clone());
        let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
        let mut manifest = json!({"importerVersion": IMPORTER_VERSION});
        let mut checkpoint_calls = 0;
        let result = {
            let mut report = |_message: &str| {};
            let mut checkpoint = || {
                checkpoint_calls += 1;
                Err(ImportError::Cancelled)
            };
            let mut hooks = MigrationHooks {
                report: &mut report,
                checkpoint: &mut checkpoint,
            };
            preserve_old_data_with_scanner(
                &cache,
                sha,
                stage.path(),
                &destination,
                &mut manifest,
                &mut hooks,
                &mut NativeImportBackend,
                &mut NativeMigrationScanner,
            )
        };

        assert!(matches!(result, Err(ImportError::Cancelled)));
        assert_eq!(checkpoint_calls, 1);
        assert!(valid_profile(&old));
        assert_eq!(profile_tree_digest(&old.directory), old_digest);
        assert_eq!(
            fs::read(old.directory.join("data/a/save.bin")).unwrap(),
            save_bytes
        );
        assert_eq!(
            fs::read(old.directory.join("data/preferences.json")).unwrap(),
            settings_bytes
        );
        assert!(!destination.exists());
        drop(stage);
        assert!(!stage_path.exists());
        fs::remove_dir_all(cache).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn native_migration_scanner_rejects_symlinks_without_following_them() {
        use std::os::unix::fs::symlink;

        let cache = temporary("migration-scan-symlink");
        fs::create_dir_all(&cache).unwrap();
        let sha = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
        let old = migration_fixture(&cache, sha);
        let outside = cache.join("outside-save.bin");
        fs::write(&outside, b"must not be followed").unwrap();
        symlink(&outside, old.directory.join("data/a/linked-save")).unwrap();
        let old_digest = profile_tree_digest(&old.directory);

        let stage_path = cache.join(".migration-symlink-stage");
        fs::create_dir_all(&stage_path).unwrap();
        let stage = OwnedStage::new(stage_path.clone());
        let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
        let mut manifest = json!({"importerVersion": IMPORTER_VERSION});
        let result = {
            let mut report = |_message: &str| {};
            let mut checkpoint = || Ok(());
            let mut hooks = MigrationHooks {
                report: &mut report,
                checkpoint: &mut checkpoint,
            };
            preserve_old_data(
                &cache,
                sha,
                stage.path(),
                &destination,
                &mut manifest,
                &mut hooks,
                &mut NativeImportBackend,
            )
        };

        assert_eq!(
            result.unwrap_err().to_string(),
            "Previous profile data exceeds migration limits or contains a symlink; the old profile is unchanged"
        );
        assert_eq!(fs::read(outside).unwrap(), b"must not be followed");
        assert_eq!(profile_tree_digest(&old.directory), old_digest);
        assert!(!destination.exists());
        drop(stage);
        assert!(!stage_path.exists());
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn migration_skips_unreadable_or_malformed_older_manifests_without_touching_data() {
        for invalid_manifest in ["malformed", "nonregular"] {
            let cache = temporary(&format!("migration-invalid-manifest-{invalid_manifest}"));
            fs::create_dir_all(&cache).unwrap();
            let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
            let old = migration_fixture(&cache, sha);
            let manifest_path = old.directory.join("profile.json");
            match invalid_manifest {
                "malformed" => fs::write(&manifest_path, b"{").unwrap(),
                "nonregular" => {
                    fs::remove_file(&manifest_path).unwrap();
                    fs::create_dir(&manifest_path).unwrap();
                }
                _ => unreachable!(),
            }
            let old_digest = profile_tree_digest(&old.directory);
            let save_path = old.directory.join("data/a/save.bin");
            let settings_path = old.directory.join("data/preferences.json");
            let save_bytes = fs::read(&save_path).unwrap();
            let settings_bytes = fs::read(&settings_path).unwrap();
            let stage_path = cache.join(".migration-invalid-manifest-stage");
            fs::create_dir(&stage_path).unwrap();
            let stage = OwnedStage::new(stage_path.clone());
            let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
            let mut manifest = json!({"importerVersion": IMPORTER_VERSION});

            let result = {
                let mut report = |_message: &str| {};
                let mut checkpoint = || Ok(());
                let mut hooks = MigrationHooks {
                    report: &mut report,
                    checkpoint: &mut checkpoint,
                };
                preserve_old_data(
                    &cache,
                    sha,
                    stage.path(),
                    &destination,
                    &mut manifest,
                    &mut hooks,
                    &mut NativeImportBackend,
                )
            };

            assert!(result.is_ok(), "{invalid_manifest}: {result:?}");
            assert!(manifest.get("migratedFrom").is_none());
            assert_eq!(fs::read(save_path).unwrap(), save_bytes);
            assert_eq!(fs::read(settings_path).unwrap(), settings_bytes);
            assert_eq!(profile_tree_digest(&old.directory), old_digest);
            assert!(!destination.exists());
            assert_eq!(fs::read_dir(stage.path()).unwrap().count(), 0);
            drop(stage);
            assert!(!stage_path.exists());
            fs::remove_dir_all(cache).unwrap();
        }
    }

    #[test]
    fn invalid_copied_settings_fail_migration_without_changing_old_data() {
        let cache = temporary("migration-invalid-copied-settings");
        fs::create_dir_all(&cache).unwrap();
        let sha = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let old = migration_fixture(&cache, sha);
        let settings_path = old.directory.join("data/preferences.json");
        fs::write(&settings_path, b"{").unwrap();
        let old_digest = profile_tree_digest(&old.directory);
        let save_path = old.directory.join("data/a/save.bin");
        let save_bytes = fs::read(&save_path).unwrap();
        let settings_bytes = fs::read(&settings_path).unwrap();
        let stage_path = cache.join(".migration-invalid-settings-stage");
        fs::create_dir(&stage_path).unwrap();
        let stage = OwnedStage::new(stage_path.clone());
        let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
        let mut manifest = json!({"importerVersion": IMPORTER_VERSION});

        let result = {
            let mut report = |_message: &str| {};
            let mut checkpoint = || Ok(());
            let mut hooks = MigrationHooks {
                report: &mut report,
                checkpoint: &mut checkpoint,
            };
            preserve_old_data(
                &cache,
                sha,
                stage.path(),
                &destination,
                &mut manifest,
                &mut hooks,
                &mut NativeImportBackend,
            )
        };

        assert!(
            result
                .unwrap_err()
                .to_string()
                .starts_with("Invalid profile manifest JSON:"),
        );
        assert_eq!(fs::read(save_path).unwrap(), save_bytes);
        assert_eq!(fs::read(settings_path).unwrap(), settings_bytes);
        assert_eq!(profile_tree_digest(&old.directory), old_digest);
        assert!(!destination.exists());
        drop(stage);
        assert!(!stage_path.exists());
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn migration_copy_failure_keeps_verified_old_profile_unchanged_and_unpublished() {
        let cache = temporary("migration-copy-failure");
        let sha = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let old = cache.join(format!("{sha}-v6"));
        let assets = old.join("assets/skin");
        let data = old.join("data");
        fs::create_dir_all(assets).unwrap();
        fs::create_dir_all(data.join("a")).unwrap();
        let catalog = old.join("assets/catalog.json");
        let layout = old.join("assets/skin/layout.json");
        fs::write(&catalog, b"old catalog").unwrap();
        fs::write(&layout, b"old layout").unwrap();
        fs::write(data.join("a/first-save"), b"old save bytes").unwrap();
        let settings = data.join("preferences.json");
        let settings_bytes = br#"{"firmware":"old/path","theme":"classic"}"#;
        fs::write(&settings, settings_bytes).unwrap();
        let old_profile = Profile {
            directory: old.clone(),
            manifest: json!({
                "importerVersion": 6,
                "sourceSHA256": sha,
                "files": {
                    "assets/catalog.json": file_digest(&catalog).unwrap(),
                    "assets/skin/layout.json": file_digest(&layout).unwrap(),
                },
            }),
        };
        write_json(&old.join("profile.json"), &old_profile.manifest).unwrap();
        assert!(valid_profile(&old_profile));

        let stage = cache.join(".migration-stage");
        fs::create_dir(&stage).unwrap();
        let destination = cache.join(format!("{sha}-v{IMPORTER_VERSION}"));
        let mut manifest = json!({"importerVersion": IMPORTER_VERSION});
        let mut report = |_message: &str| {};
        let mut checkpoint = || Ok(());
        let mut hooks = MigrationHooks {
            report: &mut report,
            checkpoint: &mut checkpoint,
        };
        let error = preserve_old_data(
            &cache,
            sha,
            &stage,
            &destination,
            &mut manifest,
            &mut hooks,
            &mut MigrationCopyFailure,
        )
        .unwrap_err()
        .to_string();

        assert!(
            error.starts_with("Cannot migrate previous profile data:"),
            "{error}"
        );
        assert!(
            error.contains("synthetic migration copy failure"),
            "{error}"
        );
        assert_eq!(
            fs::read(data.join("a/first-save")).unwrap(),
            b"old save bytes"
        );
        assert_eq!(fs::read(settings).unwrap(), settings_bytes);
        assert!(stage.join("data/a").is_dir());
        assert!(!stage.join("data/a/first-save").exists());
        assert!(!destination.exists());

        let stage = cache.join(".migration-directory-stage");
        fs::create_dir(&stage).unwrap();
        let old_digest = profile_tree_digest(&old);
        let mut report = |_message: &str| {};
        let mut checkpoint = || Ok(());
        let mut hooks = MigrationHooks {
            report: &mut report,
            checkpoint: &mut checkpoint,
        };
        let error = preserve_old_data(
            &cache,
            sha,
            &stage,
            &destination,
            &mut manifest,
            &mut hooks,
            &mut MigrationDirectoryFailure,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.starts_with("Cannot migrate previous profile data:"),
            "{error}"
        );
        assert!(!stage.join("data/a").exists());
        assert!(!destination.exists());
        assert!(valid_profile(&old_profile));
        assert_eq!(profile_tree_digest(&old), old_digest);
        fs::remove_dir_all(cache).unwrap();
    }

    #[test]
    fn resolves_only_local_boot_image_companions() {
        let root = temporary("boot-resolution");
        fs::create_dir_all(&root).unwrap();
        let boot = root.join("kernel-demo.img");
        fs::write(&boot, b"ANDROID!boot payload").unwrap();
        let filesystem = root.join("demo.hsqs");
        fs::write(&filesystem, b"not parsed during path resolution").unwrap();
        assert_eq!(
            resolve_filesystem(&boot).unwrap(),
            absolute(&filesystem).unwrap()
        );
        fs::remove_file(filesystem).unwrap();
        assert!(
            resolve_filesystem(&boot)
                .unwrap_err()
                .contains("contains no game filesystem")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profile_hash_manifest_detects_asset_tampering() {
        let root = temporary("integrity");
        let sha = "a".repeat(64);
        let directory = root.join(format!("{sha}-v{IMPORTER_VERSION}"));
        fs::create_dir_all(directory.join("assets/skin")).unwrap();
        let catalog = directory.join("assets/catalog.json");
        let layout = directory.join("assets/skin/layout.json");
        fs::write(&catalog, b"{}\n").unwrap();
        fs::write(&layout, b"{}\n").unwrap();
        let mut files = Map::new();
        files.insert(
            "assets/catalog.json".into(),
            json!(file_digest(&catalog).unwrap()),
        );
        files.insert(
            "assets/skin/layout.json".into(),
            json!(file_digest(&layout).unwrap()),
        );
        let profile = Profile {
            directory: directory.clone(),
            manifest: json!({
                "importerVersion": IMPORTER_VERSION,
                "sourceSHA256": sha,
                "region": "Europe", "version": "test", "playableGames": 21,
                "files": files,
            }),
        };
        write_json(&directory.join("profile.json"), &profile.manifest).unwrap();
        assert!(valid_profile(&profile));
        assert_eq!(list_profiles(&root).len(), 1);
        fs::write(&layout, b"tampered\n").unwrap();
        assert!(!valid_profile(&profile));
        assert!(list_profiles(&root).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_success_publishes_complete_stage_and_drop_does_not_delete_destination() {
        let root = temporary("publish-success");
        let sha = "b".repeat(64);
        fs::create_dir_all(&root).unwrap();
        let stage_path = root.join(".stage");
        let destination = root.join(format!("{sha}-v{IMPORTER_VERSION}"));
        let stage_profile = synthetic_profile(&stage_path, &sha);
        write_json(&stage_path.join("profile.json"), &stage_profile.manifest).unwrap();

        {
            let _stage = OwnedStage::new(stage_path.clone());
            let result = import_storage::publish(
                &stage_path,
                &destination,
                || -> ImportResult<Option<()>> {
                    panic!("success path must not inspect destination")
                },
            )
            .unwrap();
            assert!(matches!(result, Publication::Published));
        }

        let published = Profile {
            directory: destination.clone(),
            manifest: read_json(&destination.join("profile.json"), 1024 * 1024).unwrap(),
        };
        assert!(!stage_path.exists());
        assert!(valid_profile(&published));
        assert_eq!(
            fs::read(destination.join("assets/catalog.json")).unwrap(),
            b"synthetic catalog"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pre_rename_failure_preserves_stage_until_drop_and_other_published_data() {
        let root = temporary("publish-before-error");
        fs::create_dir_all(&root).unwrap();
        let stage_path = root.join(".stage");
        let destination = root.join("new-profile");
        let published = root.join("existing-profile");
        fs::create_dir(&stage_path).unwrap();
        fs::write(stage_path.join("partial"), b"stage data").unwrap();
        fs::create_dir(&published).unwrap();
        fs::write(published.join("profile.json"), b"published profile").unwrap();
        fs::write(published.join("userdata"), b"published userdata").unwrap();
        {
            let _stage = OwnedStage::new(stage_path.clone());
            let result = publish_with(
                &mut RenameFault {
                    after_rename: false,
                },
                &stage_path,
                &destination,
                || Ok(None::<()>),
            );
            assert_eq!(
                result.unwrap_err(),
                "Cannot publish imported profile: synthetic rename failure"
            );
            assert_eq!(fs::read(stage_path.join("partial")).unwrap(), b"stage data");
            assert!(!destination.exists());
            assert_eq!(
                fs::read(published.join("profile.json")).unwrap(),
                b"published profile"
            );
            assert_eq!(
                fs::read(published.join("userdata")).unwrap(),
                b"published userdata"
            );
        }
        assert!(!stage_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn valid_concurrent_winner_is_returned_and_losing_stage_is_cleaned() {
        let root = temporary("publish-winner");
        fs::create_dir_all(&root).unwrap();
        let sha = "c".repeat(64);
        let winner = fixture_profile(&root, &sha);
        fs::write(winner.directory.join("userdata"), b"winner userdata").unwrap();
        let stage_path = root.join(".loser-stage");
        fs::create_dir(&stage_path).unwrap();
        fs::write(stage_path.join("partial"), b"loser stage").unwrap();

        {
            let _stage = OwnedStage::new(stage_path.clone());
            let result = publish_with(
                &mut RenameFault {
                    after_rename: false,
                },
                &stage_path,
                &winner.directory,
                || {
                    let profile = Profile {
                        directory: winner.directory.clone(),
                        manifest: read_json(&winner.directory.join("profile.json"), 1024 * 1024)?,
                    };
                    Ok(valid_profile(&profile).then_some(profile))
                },
            )
            .unwrap();
            assert!(matches!(result, Publication::Existing(profile) if valid_profile(&profile)));
            assert_eq!(
                fs::read(stage_path.join("partial")).unwrap(),
                b"loser stage"
            );
            assert_eq!(
                fs::read(winner.directory.join("userdata")).unwrap(),
                b"winner userdata"
            );
        }
        assert!(!stage_path.exists());
        assert_eq!(
            fs::read(winner.directory.join("userdata")).unwrap(),
            b"winner userdata"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_collision_and_callback_read_error_keep_existing_destination() {
        let root = temporary("publish-invalid");
        fs::create_dir_all(&root).unwrap();
        let stage_path = root.join(".stage");
        fs::create_dir(&stage_path).unwrap();
        let sha = "d".repeat(64);
        let invalid = fixture_profile(&root, &sha);
        fs::write(invalid.directory.join("userdata"), b"do not overwrite").unwrap();
        fs::write(invalid.directory.join("assets/catalog.json"), b"tampered").unwrap();

        let collision = publish_with(
            &mut RenameFault {
                after_rename: false,
            },
            &stage_path,
            &invalid.directory,
            || {
                let existing = Profile {
                    directory: invalid.directory.clone(),
                    manifest: read_json(&invalid.directory.join("profile.json"), 1024 * 1024)?,
                };
                Ok(valid_profile(&existing).then_some(existing))
            },
        )
        .unwrap_err();
        assert_eq!(
            collision,
            "Cannot publish profile because its destination already exists: synthetic rename failure"
        );
        assert_eq!(
            fs::read(invalid.directory.join("userdata")).unwrap(),
            b"do not overwrite"
        );
        assert_eq!(
            fs::read(invalid.directory.join("assets/catalog.json")).unwrap(),
            b"tampered"
        );
        assert!(stage_path.is_dir());

        let expected_read_error =
            read_json(&invalid.directory.join("missing.json"), 1024 * 1024).unwrap_err();
        let read_error = publish_with(
            &mut RenameFault {
                after_rename: false,
            },
            &stage_path,
            &invalid.directory,
            || read_json(&invalid.directory.join("missing.json"), 1024 * 1024).map(Some),
        )
        .unwrap_err();
        assert_eq!(read_error, expected_read_error);
        assert_eq!(
            fs::read(invalid.directory.join("userdata")).unwrap(),
            b"do not overwrite"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn error_after_rename_revalidates_fresh_destination_as_winner() {
        let root = temporary("publish-after-error");
        fs::create_dir_all(&root).unwrap();
        let sha = "e".repeat(64);
        let stage_path = root.join(".stage");
        let destination = root.join(format!("{sha}-v{IMPORTER_VERSION}"));
        let profile = synthetic_profile(&stage_path, &sha);
        write_json(&stage_path.join("profile.json"), &profile.manifest).unwrap();

        {
            let _stage = OwnedStage::new(stage_path.clone());
            let result = publish_with(
                &mut RenameFault { after_rename: true },
                &stage_path,
                &destination,
                || {
                    let fresh = Profile {
                        directory: destination.clone(),
                        manifest: read_json(&destination.join("profile.json"), 1024 * 1024)?,
                    };
                    Ok(valid_profile(&fresh).then_some(fresh))
                },
            )
            .unwrap();
            assert!(matches!(result, Publication::Existing(fresh) if valid_profile(&fresh)));
            assert!(destination.join("assets/catalog.json").is_file());
        }
        assert!(destination.join("assets/catalog.json").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
