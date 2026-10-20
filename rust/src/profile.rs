//! Hash-addressed image profiles, conversion, cache validation, and data migration.

use crate::{
    hash::{digest, file_digest},
    image_reader::{self, Files},
    sfrom::convert_sfrom,
    skin::prepare_skin,
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Write},
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

struct StagingDirectory(PathBuf);

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        // This path was exclusively created by this importer invocation.
        let _ = fs::remove_dir_all(&self.0);
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
    let bytes = fs::read(path).map_err(|e| format!("Cannot read profile manifest: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid profile manifest JSON: {e}"))
}

fn write_bytes(path: &Path, bytes: &[u8]) -> ImportResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create profile directory: {e}"))?;
    }
    let mut file = File::create(path).map_err(|e| format!("Cannot write profile file: {e}"))?;
    file.write_all(bytes)
        .map_err(|e| format!("Cannot write profile file: {e}"))
}

fn write_json(path: &Path, value: &Value) -> ImportResult<()> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|e| format!("Cannot encode profile JSON: {e}"))?;
    bytes.push(b'\n');
    write_bytes(path, &bytes)
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

fn create_stage(cache: &Path, sha: &str) -> ImportResult<StagingDirectory> {
    for _ in 0..20 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let serial = STAGE_SERIAL.fetch_add(1, Ordering::Relaxed);
        let path = cache.join(format!(".import-{}-{now}-{serial}", &sha[..12]));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(StagingDirectory(path)),
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

fn preserve_old_data(
    cache: &Path,
    sha: &str,
    stage: &Path,
    destination: &Path,
    manifest: &mut Value,
    report: &mut dyn FnMut(&str),
) -> ImportResult<()> {
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
        report("Preserving settings and saves from the previous importer...");
        let mut entries = Vec::new();
        let mut stack = vec![data.clone()];
        let mut total = 0u64;
        while let Some(directory) = stack.pop() {
            for entry in fs::read_dir(&directory)
                .map_err(|e| format!("Cannot read previous profile data: {e}"))?
            {
                let entry = entry.map_err(|e| format!("Cannot read previous profile data: {e}"))?;
                let metadata = fs::symlink_metadata(entry.path())
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
            let relative = source
                .strip_prefix(&data)
                .map_err(|_| "Previous profile path escaped its data directory")?;
            let target = stage.join("data").join(relative);
            if is_directory {
                fs::create_dir_all(&target)
                    .map_err(|e| format!("Cannot migrate previous profile data: {e}"))?;
            } else {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Cannot migrate previous profile data: {e}"))?;
                }
                fs::copy(&source, &target)
                    .map_err(|e| format!("Cannot migrate previous profile data: {e}"))?;
            }
        }
        let preferences = stage.join("data/preferences.json");
        if regular_not_symlink(&preferences) {
            let mut values = read_json(&preferences, 32 * 1024 * 1024)?;
            let old_firmware = path_string(&old.join("assets/firmware"));
            if values.get("firmware").and_then(Value::as_str) == Some(&old_firmware) {
                values["firmware"] = json!(path_string(&destination.join("assets/firmware")));
                write_json(&preferences, &values)?;
            }
        }
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
    mut progress: Option<&mut Progress<'_>>,
) -> ImportResult<Profile> {
    let report = |progress: &mut Option<&mut Progress<'_>>, message: &str| {
        if let Some(callback) = progress.as_deref_mut() {
            callback(message);
        }
    };
    let filesystem = resolve_filesystem(source)?;
    let size = fs::metadata(&filesystem)
        .map_err(|e| format!("Cannot inspect filesystem image: {e}"))?
        .len();
    if size > 1024 * 1024 * 1024 {
        return Err("Image exceeds 1 GiB import limit".into());
    }
    report(&mut progress, "Hashing filesystem image...");
    let sha = file_digest(&filesystem).map_err(|e| format!("Cannot hash filesystem image: {e}"))?;
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
        if !valid_profile(&existing) {
            return Err(format!(
                "Cached profile failed integrity checks. Preserve/rename that profile folder, then import again: {}",
                destination.display()
            ));
        }
        report(&mut progress, "Using verified cache");
        return Ok(existing);
    }
    report(
        &mut progress,
        "Reading image (no extracted code is executed)...",
    );
    let files = image_reader::read_image(&filesystem)?;
    if file_digest(&filesystem).map_err(|e| format!("Cannot rehash filesystem image: {e}"))? != sha
    {
        return Err("Source image changed during import".into());
    }
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
    let stage_path = stage.0.clone();
    fs::create_dir_all(stage_path.join("data"))
        .map_err(|e| format!("Cannot initialize profile data directory: {e}"))?;
    report(&mut progress, "Decoding Clover artwork and fonts...");
    prepare_skin(&files, ui, &stage_path.join("assets/skin"))?;

    if !nes {
        report(
            &mut progress,
            "Locating verified SMP boot firmware in the supplied image...",
        );
        let ipl = locate_ipl(&files)
            .ok_or("Image does not contain the verified SMP IPL firmware required by ares")?;
        write_bytes(&stage_path.join("assets/firmware/ipl.rom"), &ipl)?;
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
            write_bytes(&stage_path.join("assets/games").join(code).join(name), art)?;
        }
        report(&mut progress, &format!("Preparing {title}..."));
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
                    write_bytes(&stage_path.join("assets").join(&relative), &rom)?;
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
                write_bytes(&stage_path.join("assets").join(&relative), &converted.rom)?;
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
    }
    if games.len() != if nes { 30 } else { 21 } {
        return Err(format!(
            "Unsupported stock catalog: expected {} entries",
            if nes { 30 } else { 21 }
        ));
    }
    let catalog = json!({
        "schema": 1, "region": region, "ui": ui, "system": system,
        "locale": locale,
        "version": version, "games": games,
    });
    write_json(&stage_path.join("assets/catalog.json"), &catalog)?;
    let selected_source = absolute(source)?;
    let mut manifest = json!({
        "schema": 1, "importerVersion": IMPORTER_VERSION, "sourceSHA256": sha,
        "source": path_string(&filesystem), "selectedSource": path_string(&selected_source),
        "selectedSourceSHA256": file_digest(source).map_err(|e| format!("Cannot hash selected image: {e}"))?,
        "region": region, "ui": ui, "locale": locale, "system": system,
        "version": version, "games": games.len(), "playableGames": playable,
        "externalFirmwareGames": external_firmware_games, "conversions": conversions,
        "smpIPL": if nes { Value::Null } else { json!({"source": "usr/bin/canoe-shvc", "sha256": "c95f88b299030d5afa55b1031e2b5ef2dff650c4b4e6bb6f8b1359436521278f"}) },
        "files": {},
    });
    preserve_old_data(
        cache,
        &sha,
        &stage_path,
        &destination,
        &mut manifest,
        &mut |message| report(&mut progress, message),
    )?;
    manifest["files"] = Value::Object(hash_asset_tree(&stage_path.join("assets"))?);
    write_json(&stage_path.join("profile.json"), &manifest)?;
    match fs::rename(&stage_path, &destination) {
        Ok(()) => {}
        Err(error) if fs::symlink_metadata(&destination).is_ok() => {
            let existing = Profile {
                directory: destination.clone(),
                manifest: read_json(&destination.join("profile.json"), 32 * 1024 * 1024)?,
            };
            if valid_profile(&existing) {
                report(&mut progress, "Using verified cache");
                return Ok(existing);
            }
            return Err(format!(
                "Cannot publish profile because its destination already exists: {error}"
            ));
        }
        Err(error) => return Err(format!("Cannot publish imported profile: {error}")),
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
    use std::time::UNIX_EPOCH;

    fn temporary(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("clover-profile-{name}-{now}"))
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
}
