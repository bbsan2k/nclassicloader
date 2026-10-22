//! Filesystem-backed launcher startup discovery and preference restoration.
use super::*;

pub(super) struct Startup {
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub filter: LibraryFilter,
}

type ReleaseKey = (String, String, String);

pub(super) fn load(home: &Path) -> Startup {
    load_with_places(home, &discovery_places())
}

fn discovery_places() -> Vec<PathBuf> {
    let mut places = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        places.push(cwd.clone());
        places.push(cwd.join("SNES-Mini-Kernels"));
        places.push(cwd.join("NES-Mini-Kernels"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        places.push(parent.to_owned());
        places.push(parent.join("../../.."));
        places.push(parent.join("../../../SNES-Mini-Kernels"));
        places.push(parent.join("../../../NES-Mini-Kernels"));
        // Repository app bundles live in dist/*.app/Contents/MacOS;
        // their image folders are one directory above dist, not inside it.
        places.push(parent.join("../../../../SNES-Mini-Kernels"));
        places.push(parent.join("../../../../NES-Mini-Kernels"));
    }
    places
}

fn load_with_places(home: &Path, places: &[PathBuf]) -> Startup {
    let mut entries: Vec<_> = crate::profile::list_profiles(&home.join("profiles"))
        .into_iter()
        .map(Entry::Cached)
        .collect();
    append_unimported_images(&mut entries, crate::profile::discover_images(places));
    // Stable sorting keeps verified caches and discovered versions ordered
    // within each system family.
    entries.sort_by_key(view::system_group);
    let last = read_json(&home.join("last-profile.json")).ok();
    let mut selected = entries
        .iter()
        .position(|entry| match entry {
            Entry::Cached(profile) => {
                profile.directory.file_name().and_then(|n| n.to_str())
                    == last
                        .as_ref()
                        .and_then(|v| v.get("directory"))
                        .and_then(Value::as_str)
            }
            Entry::Image(_) => false,
        })
        .unwrap_or(0);
    let settings = read_json(&home.join("launcher.json")).ok();
    let mut filter = crate::frontend::preferences::decode_filter(
        settings
            .as_ref()
            .and_then(|value| value.get("systemFilter"))
            .and_then(Value::as_str),
    );
    if filter != LibraryFilter::All {
        let next = filter;
        filter = LibraryFilter::All;
        change_filter(&entries, &mut filter, next, &mut selected, &mut None);
    }
    Startup {
        entries,
        selected,
        filter,
    }
}

pub(super) fn append_unimported_images(
    entries: &mut Vec<Entry>,
    images: impl IntoIterator<Item = PathBuf>,
) {
    let mut ready_sources = HashSet::new();
    let mut ready_releases: HashSet<ReleaseKey> = HashSet::new();
    for entry in entries.iter() {
        if let Entry::Cached(profile) = entry {
            if let Some(source) = profile.manifest.get("source").and_then(Value::as_str)
                && let Ok(source) = fs::canonicalize(source)
            {
                ready_sources.insert(source);
            }
            if let Some(release) = view::release_key(entry) {
                ready_releases.insert(release);
            }
        }
    }

    for path in images {
        let same_source = fs::canonicalize(&path)
            .ok()
            .is_some_and(|path| ready_sources.contains(&path));
        let image = Entry::Image(path);
        let same_release =
            view::release_key(&image).is_some_and(|release| ready_releases.contains(&release));
        if !same_source && !same_release {
            entries.push(image);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let serial = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "clover-launcher-startup-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn home(&self) -> PathBuf {
            self.0.join("home")
        }

        fn add_profile(&self, slug: &str, system: &str, ui: &str) -> String {
            use sha2::{Digest, Sha256};
            let contents = [
                ("assets/catalog.json", b"{}".as_slice()),
                ("assets/skin/layout.json", b"{}".as_slice()),
            ];
            let hashes: serde_json::Map<String, Value> = contents
                .iter()
                .map(|(name, bytes)| {
                    (
                        (*name).to_owned(),
                        Value::String(format!("{:x}", Sha256::digest(bytes))),
                    )
                })
                .collect();
            let source_hash = format!("{:x}", Sha256::digest(slug.as_bytes()));
            let directory_name = format!("{source_hash}-v{}", crate::profile::IMPORTER_VERSION);
            let directory = self.home().join("profiles").join(&directory_name);
            let source = self.0.join("sources").join(format!("{slug}.hsqs"));
            fs::create_dir_all(source.parent().unwrap()).unwrap();
            fs::write(&source, b"synthetic image").unwrap();
            for (name, bytes) in contents {
                let path = directory.join(name);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, bytes).unwrap();
            }
            fs::write(
                directory.join("profile.json"),
                serde_json::to_vec(&json!({
                    "importerVersion": crate::profile::IMPORTER_VERSION,
                    "sourceSHA256": source_hash,
                    "files": hashes,
                    "system": system,
                    "ui": ui,
                    "region": "USA",
                    "version": slug,
                    "source": source,
                }))
                .unwrap(),
            )
            .unwrap();
            directory_name
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn entry_names(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Cached(profile) => profile
                    .directory
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                Entry::Image(path) => path.file_name().unwrap().to_string_lossy().into_owned(),
            })
            .collect()
    }

    #[test]
    fn startup_restores_cached_identity_and_filter_after_stable_grouping() {
        let fixture = Fixture::new();
        let cache_b = fixture.add_profile("v1.0.2", "nes", "nes");
        let cache_a = fixture.add_profile("v1.0.1", "nes", "nes");
        let snes = fixture.add_profile("snes-1.0.1", "snes", "shvc");
        let places = [fixture.0.join("empty-source-folder")];
        fs::create_dir_all(&places[0]).unwrap();
        fs::create_dir_all(fixture.home()).unwrap();
        fs::write(
            fixture.home().join("last-profile.json"),
            json!({"directory": cache_b}).to_string(),
        )
        .unwrap();
        fs::write(
            fixture.home().join("launcher.json"),
            json!({"systemFilter": "NES"}).to_string(),
        )
        .unwrap();

        let startup = load_with_places(&fixture.home(), &places);
        assert_eq!(startup.filter, LibraryFilter::Nes);
        let names = entry_names(&startup.entries);
        assert_eq!(names, [cache_a.clone(), cache_b.clone(), snes]);
        assert_eq!(startup.selected, 1);
        assert_eq!(names[startup.selected], cache_b);
    }

    #[test]
    fn discovery_deduplicates_cached_source_and_release_rows() {
        let fixture = Fixture::new();
        let cached = fixture.add_profile("1.0.2", "nes", "nes");
        let source = fixture.0.join("sources/1.0.2.hsqs");
        let release = fixture
            .0
            .join("sources/dp-nes-release-v1.0.2-0-g99e37e1.hsqs");
        fs::write(&release, b"another copy of release").unwrap();
        let startup = load_with_places(&fixture.home(), &[fixture.0.join("sources")]);
        assert_eq!(entry_names(&startup.entries), [cached]);
        assert!(fs::canonicalize(source).is_ok());
    }

    #[test]
    fn missing_or_invalid_preferences_default_without_failing_startup() {
        let fixture = Fixture::new();
        let places = [fixture.0.join("empty")];
        fs::create_dir_all(&places[0]).unwrap();
        let missing = load_with_places(&fixture.home(), &places);
        assert!(missing.entries.is_empty());
        assert_eq!(missing.selected, 0);
        assert_eq!(missing.filter, LibraryFilter::All);

        fs::create_dir_all(fixture.home()).unwrap();
        fs::write(fixture.home().join("last-profile.json"), b"not json").unwrap();
        fs::write(fixture.home().join("launcher.json"), b"not json").unwrap();
        let invalid = load_with_places(&fixture.home(), &places);
        assert!(invalid.entries.is_empty());
        assert_eq!(invalid.selected, 0);
        assert_eq!(invalid.filter, LibraryFilter::All);
    }
}
