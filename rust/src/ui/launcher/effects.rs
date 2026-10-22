//! Native launcher effects. These operations are deliberately outside navigation reduction.
use super::*;
use crate::frontend::launcher_frame::{self, PickerDecision, PickerOutcome};

pub(super) fn finish(home: &Path, profile: crate::profile::Profile) -> Result<Selection, String> {
    let name = profile
        .directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid profile path")?;
    atomic_json(&home.join("last-profile.json"), &json!({"directory": name}))?;
    Ok(Selection {
        assets: profile.assets(),
        data: profile.data(),
    })
}

pub(super) enum Activation {
    Select(Selection),
    Import(PathBuf),
}

pub(super) fn activate_entry(home: &Path, entry: &Entry) -> Result<Activation, String> {
    match entry {
        Entry::Cached(profile) => {
            let old = profile
                .manifest
                .get("importerVersion")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                < u64::from(crate::profile::IMPORTER_VERSION);
            let source = profile
                .manifest
                .get("source")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            if old && let Some(source) = source.filter(|path| path.is_file()) {
                Ok(Activation::Import(source))
            } else {
                finish(home, profile.clone()).map(Activation::Select)
            }
        }
        Entry::Image(path) => Ok(Activation::Import(path.clone())),
    }
}

pub(super) fn persist_filter(home: &Path, state: &mut session::SessionState) -> Result<(), String> {
    if state.saved_filter != state.navigation.filter {
        atomic_json(
            &home.join("launcher.json"),
            &json!({"systemFilter":crate::frontend::preferences::encode_filter(state.navigation.filter)}),
        )?;
        state.saved_filter = state.navigation.filter;
    }
    Ok(())
}

pub(super) fn open_image_picker(
    window: &sdl3::video::Window,
    pad: Option<&mut controller::Controller>,
    state: &mut session::SessionState,
    services: &mut session::NativeServices,
) -> Result<(), String> {
    let request = services
        .picker
        .begin(image_picker::Kind::Image, Capability::Available)
        .map_err(|e| format!("Image picker unavailable: {e:?}"))?;
    state
        .activity
        .reduce(crate::frontend::launcher_activity::Event::PickerOpened);
    if let Some(pad) = pad {
        pad.suspend();
    }
    let sender = services.sender.clone();
    dialog::show_open_file_dialog(
        &[DialogFileFilter {
            name: "NES / SNES Classic filesystem",
            pattern: "hsqs;gz;img",
        }],
        None::<&Path>,
        false,
        Some(window),
        Box::new(move |result, _| {
            let outcome = image_picker::map_dialog(result);
            let _ = sender.send(Message::Picked(request, image_picker::Kind::Image, outcome));
        }),
    )
    .map_err(|e| {
        services.picker.cancel(request);
        launcher_frame::complete_picker(&mut state.activity, &PickerOutcome::Failed(e.to_string()));
        e.to_string()
    })
}

pub(super) fn open_folder_picker(
    window: &sdl3::video::Window,
    pad: Option<&mut controller::Controller>,
    state: &mut session::SessionState,
    services: &mut session::NativeServices,
) -> Result<(), String> {
    let request = services
        .picker
        .begin(image_picker::Kind::PreparedFolder, Capability::Available)
        .map_err(|e| format!("Folder picker unavailable: {e:?}"))?;
    state
        .activity
        .reduce(crate::frontend::launcher_activity::Event::PickerOpened);
    if let Some(pad) = pad {
        pad.suspend();
    }
    let sender = services.sender.clone();
    dialog::show_open_folder_dialog(
        None::<&Path>,
        false,
        Some(window),
        Box::new(move |result, _| {
            let outcome = image_picker::map_dialog(result);
            let _ = sender.send(Message::Picked(
                request,
                image_picker::Kind::PreparedFolder,
                outcome,
            ));
        }),
    );
    Ok(())
}

pub(super) fn start_image_worker(
    path: PathBuf,
    cache: &Path,
    pad: Option<&mut controller::Controller>,
    activity: &mut crate::frontend::launcher_activity::Activity,
    services: &mut session::NativeServices,
) {
    let sender = services.sender.clone();
    let cache = cache.to_owned();
    match services.worker.start(move |id, control| {
        crate::profile::import_image_cancellable(
            &path,
            &cache,
            Some(&mut |progress| {
                let _ = sender.send(Message::Status(id, progress.to_owned()));
            }),
            &control,
        )
    }) {
        Ok(_) => {
            activity.reduce(crate::frontend::launcher_activity::Event::ImportStarted);
            if let Some(pad) = pad {
                pad.suspend();
            }
        }
        Err(message) => {
            launcher_frame::host_error(activity, message);
        }
    }
}

pub(super) fn continue_picker_action(
    home: &Path,
    cache: &Path,
    action: image_picker::Action,
    pad: Option<&mut controller::Controller>,
    activity: &mut crate::frontend::launcher_activity::Activity,
    services: &mut session::NativeServices,
) -> Result<PickerCompletion, String> {
    let outcome = match &action {
        image_picker::Action::Image(_) => PickerOutcome::SelectedImage,
        image_picker::Action::Assets(_) => PickerOutcome::SelectedFolder,
        image_picker::Action::Error(message) => PickerOutcome::Failed(message.clone()),
        image_picker::Action::Done => PickerOutcome::Cancelled,
    };
    let decision = launcher_frame::complete_picker(activity, &outcome);
    if decision == PickerDecision::Rejected {
        return Ok(PickerCompletion {
            accepted: false,
            selection: None,
        });
    }

    let selection = match (decision, action) {
        (PickerDecision::ImportImage, image_picker::Action::Image(path)) => {
            start_image_worker(path, cache, pad, activity, services);
            None
        }
        (PickerDecision::ResolveFolder, image_picker::Action::Assets(path)) => {
            match resolve_prepared_folder(home, &path) {
                Ok(selection) => Some(selection),
                Err(message) => {
                    launcher_frame::host_error(
                        activity,
                        format!("Choose a folder containing catalog.json and skin/: {message}"),
                    );
                    None
                }
            }
        }
        (PickerDecision::Done, _) => None,
        _ => None,
    };
    Ok(PickerCompletion {
        accepted: true,
        selection,
    })
}

pub(super) struct PickerCompletion {
    pub accepted: bool,
    pub selection: Option<Selection>,
}

pub(super) fn resolve_prepared_folder(home: &Path, path: &Path) -> Result<Selection, String> {
    let assets = if path.join("assets/catalog.json").is_file() {
        path.join("assets")
    } else {
        path.to_owned()
    };
    read_games(&assets)?;
    let absolute = fs::canonicalize(&assets).map_err(|e| e.to_string())?;
    use sha2::{Digest, Sha256};
    let key = format!(
        "{:x}",
        Sha256::digest(absolute.to_string_lossy().as_bytes())
    );
    Ok(Selection {
        assets: absolute,
        data: home.join("external-data").join(key),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "clover-launcher-effects-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn profile(directory: PathBuf, manifest: Value) -> crate::profile::Profile {
        crate::profile::Profile {
            directory,
            manifest,
        }
    }

    #[test]
    fn cached_finish_persists_only_the_selected_profile() {
        let fixture = Fixture::new();
        let home = fixture.0.join("home");
        fs::create_dir_all(&home).unwrap();
        let selected = Entry::Cached(profile(
            PathBuf::from("cache/profile-selected"),
            json!({"importerVersion": crate::profile::IMPORTER_VERSION}),
        ));
        let selection = match activate_entry(&home, &selected).unwrap() {
            Activation::Select(selection) => selection,
            Activation::Import(_) => panic!("current cached entry should open directly"),
        };
        assert_eq!(
            selection.assets,
            PathBuf::from("cache/profile-selected/assets")
        );
        assert_eq!(selection.data, PathBuf::from("cache/profile-selected/data"));
        let persisted: Value =
            serde_json::from_slice(&fs::read(home.join("last-profile.json")).unwrap()).unwrap();
        assert_eq!(persisted, json!({"directory":"profile-selected"}));
    }

    #[test]
    fn old_cached_profile_queues_existing_source_instead_of_opening_cache() {
        let fixture = Fixture::new();
        let home = fixture.0.join("home");
        fs::create_dir_all(&home).unwrap();
        let source = fixture.0.join("source.hsqs");
        fs::write(&source, b"source").unwrap();
        let entry = Entry::Cached(profile(
            PathBuf::from("cache/profile"),
            json!({"importerVersion": 0, "source": source}),
        ));
        assert!(matches!(
            activate_entry(&home, &entry).unwrap(),
            Activation::Import(path) if path == source
        ));
        assert!(!home.join("last-profile.json").exists());
    }

    #[test]
    fn direct_picker_source_handoff_obeys_busy_closing_and_pending_guards() {
        let fixture = Fixture::new();
        let mut services = session::NativeServices::default();
        for (busy, closing, starts) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let mut activity = crate::frontend::launcher_activity::Activity::new("before");
            activity.busy = busy;
            activity.closing = closing;
            activity.picker_pending = true;
            let completion = continue_picker_action(
                &fixture.0,
                &fixture.0.join("profiles"),
                image_picker::Action::Image(fixture.0.join("ignored.hsqs")),
                None,
                &mut activity,
                &mut services,
            )
            .unwrap();
            assert!(completion.accepted);
            assert!(completion.selection.is_none());
            assert_eq!(activity.busy, busy || starts);
            assert!(!activity.picker_pending);
            assert_eq!(activity.closing, closing);
            assert_eq!(
                activity.status,
                if starts {
                    "Starting read-only import..."
                } else {
                    "before"
                }
            );
            assert_eq!(services.worker.is_busy(), starts);
            if starts {
                services.shutdown();
            }
        }
    }

    #[test]
    fn accepted_picker_image_starts_before_old_bounded_status_backlog() {
        use std::{
            thread,
            time::{Duration, Instant},
        };

        let fixture = Fixture::new();
        let home = fixture.0.join("home");
        let cache = home.join("profiles");
        fs::create_dir_all(&home).unwrap();
        let mut services = session::NativeServices::default();
        let status_job = services
            .worker
            .start(|_, _| {
                Ok(crate::profile::Profile {
                    directory: PathBuf::from("retired"),
                    manifest: json!({}),
                })
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if services.worker.poll(false).is_some() {
                break;
            }
            assert!(Instant::now() < deadline, "fixture worker did not retire");
            thread::yield_now();
        }
        for index in 0..super::super::COMPLETION_BUDGET {
            services
                .sender
                .send(Message::Status(status_job, format!("old status {index}")))
                .unwrap();
        }

        let request = services
            .picker
            .begin(image_picker::Kind::Image, Capability::Available)
            .unwrap();
        let mut activity = crate::frontend::launcher_activity::Activity::new("picker");
        assert!(activity.reduce(crate::frontend::launcher_activity::Event::PickerOpened));
        let before = activity.clone();
        assert!(
            image_picker::finish(
                &mut services.picker,
                request,
                image_picker::Kind::PreparedFolder,
                Outcome::Selected(PathBuf::from("wrong-kind")),
                false,
                false,
            )
            .is_none()
        );
        assert_eq!(activity, before);
        assert!(!services.worker.is_busy());

        services.picker.invalidate();
        let current_request = services
            .picker
            .begin(image_picker::Kind::Image, Capability::Available)
            .unwrap();
        assert!(
            image_picker::finish(
                &mut services.picker,
                request,
                image_picker::Kind::Image,
                Outcome::Selected(fixture.0.join("stale-image.hsqs")),
                false,
                false,
            )
            .is_none()
        );
        assert_eq!(activity, before);
        assert!(!services.worker.is_busy());

        let action = image_picker::finish(
            &mut services.picker,
            current_request,
            image_picker::Kind::Image,
            Outcome::Selected(fixture.0.join("missing-image.hsqs")),
            false,
            false,
        )
        .expect("current correlated picker result is accepted");
        let completion =
            continue_picker_action(&home, &cache, action, None, &mut activity, &mut services)
                .unwrap();
        assert!(completion.accepted && completion.selection.is_none());
        assert!(activity.busy);
        assert!(services.worker.is_busy());
        assert_eq!(
            services.receiver.try_iter().count(),
            super::super::COMPLETION_BUDGET,
            "accepted source does not get queued behind the old backlog"
        );
        assert_eq!(
            eligible_action(
                Some(0),
                activity.busy,
                activity.closing,
                activity.picker_pending
            ),
            None,
            "a later launcher action cannot overtake the accepted source"
        );

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(action) = services.worker.poll(false) {
                assert!(matches!(action, import_worker::Action::Error(_)));
                activity.reduce(crate::frontend::launcher_activity::Event::ImportFinished(
                    crate::frontend::launcher_activity::ImportOutcome::Failed(
                        "fixture import failed".into(),
                    ),
                ));
                break;
            }
            assert!(Instant::now() < deadline, "fixture import did not finish");
            thread::yield_now();
        }
        assert!(!activity.busy);
        assert!(!services.worker.is_busy());
    }

    #[test]
    fn prepared_folder_validation_canonicalizes_and_isolates_external_data() {
        let fixture = Fixture::new();
        let home = fixture.0.join("home");
        fs::create_dir_all(&home).unwrap();
        let assets = fixture.0.join("prepared/assets");
        fs::create_dir_all(&assets).unwrap();
        fs::write(
            assets.join("catalog.json"),
            br#"{"schema":1,"games":[{"id":"fixture"}]}"#,
        )
        .unwrap();
        let first = resolve_prepared_folder(&home, &fixture.0.join("prepared")).unwrap();
        let alias = resolve_prepared_folder(&home, &assets).unwrap();
        assert_eq!(first.assets, fs::canonicalize(&assets).unwrap());
        assert_eq!(first.data, alias.data);
        let mut activity = crate::frontend::launcher_activity::Activity::new("folder");
        activity.reduce(crate::frontend::launcher_activity::Event::PickerOpened);
        let mut services = session::NativeServices::default();
        let completion = continue_picker_action(
            &home,
            &home.join("profiles"),
            image_picker::Action::Assets(fixture.0.join("prepared")),
            None,
            &mut activity,
            &mut services,
        )
        .unwrap();
        assert!(completion.accepted);
        let continued = completion.selection.unwrap();
        assert_eq!(continued.assets, first.assets);
        assert_eq!(continued.data, first.data);
        assert!(!activity.busy && !services.worker.is_busy());

        let other = fixture.0.join("other");
        fs::create_dir_all(&other).unwrap();
        fs::write(
            other.join("catalog.json"),
            br#"{"schema":1,"games":[{"id":"fixture"}]}"#,
        )
        .unwrap();
        let separate = resolve_prepared_folder(&home, &other).unwrap();
        assert_ne!(first.data, separate.data);
    }

    #[test]
    fn invalid_prepared_folder_preserves_validation_error() {
        let fixture = Fixture::new();
        let error = match resolve_prepared_folder(&fixture.0, &fixture.0.join("missing")) {
            Ok(_) => panic!("missing folder unexpectedly validated"),
            Err(error) => error,
        };
        assert!(error.starts_with("Cannot read "));
    }
}
