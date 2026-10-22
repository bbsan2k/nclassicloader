use crate::frontend::requests::{BeginError, Capability, Outcome, RequestId, RequestTracker};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Context {
    pub profile: PathBuf,
    pub game: String,
    pub start: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Rom,
    Firmware,
    Image,
}

#[derive(Default)]
pub(super) struct Requests {
    tracker: RequestTracker,
    context: Option<Context>,
    kind: Option<Kind>,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum Action {
    Selected(Kind, Context, PathBuf),
    Error(String),
    Done,
}

impl Requests {
    pub fn begin(
        &mut self,
        kind: Kind,
        context: Context,
        capability: Capability,
    ) -> Result<RequestId, BeginError> {
        let id = self.tracker.begin(capability)?;
        self.context = Some(context);
        self.kind = Some(kind);
        Ok(id)
    }
    pub fn invalidate(&mut self) {
        self.tracker.invalidate();
        self.context = None;
        self.kind = None;
    }
    pub fn close(&mut self) {
        self.tracker.close();
        self.context = None;
        self.kind = None;
    }
    pub fn finish(
        &mut self,
        id: RequestId,
        kind: Kind,
        result: Outcome<PathBuf, String>,
        current: &Context,
    ) -> Option<Action> {
        if self.kind != Some(kind) {
            return None;
        }
        let outcome = self.tracker.complete(id, result)?;
        let context = self.context.take()?;
        self.kind = None;
        if context.profile != current.profile || context.game != current.game {
            return None;
        }
        Some(match outcome {
            Outcome::Selected(path) => Action::Selected(kind, context, path),
            Outcome::Failed(error) | Outcome::Denied(error) => Action::Error(error),
            Outcome::Cancelled => Action::Done,
        })
    }
}

pub(super) fn map_dialog(
    result: Result<Vec<PathBuf>, sdl3::dialog::DialogError>,
) -> Outcome<PathBuf, String> {
    match result {
        Ok(paths) => paths
            .into_iter()
            .next()
            .map(Outcome::Selected)
            .unwrap_or(Outcome::Cancelled),
        Err(sdl3::dialog::DialogError::Canceled) => Outcome::Cancelled,
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::requests::BeginError;

    fn context(profile: &str, game: &str, start: bool) -> Context {
        Context {
            profile: profile.into(),
            game: game.into(),
            start,
        }
    }

    #[derive(Default)]
    struct FakeHost {
        requests: Requests,
        current: Context,
        overrides: Vec<PathBuf>,
        starts: usize,
        status: String,
        firmware: Option<PathBuf>,
        imports: Vec<PathBuf>,
    }
    impl Default for Context {
        fn default() -> Self {
            context("profile", "game", false)
        }
    }
    impl FakeHost {
        fn deliver(&mut self, id: RequestId, kind: Kind, result: Outcome<PathBuf, String>) {
            if let Some(action) = self.requests.finish(id, kind, result, &self.current) {
                match action {
                    Action::Selected(kind, ctx, path) => match kind {
                        Kind::Rom => {
                            self.overrides.push(path);
                            if ctx.start {
                                self.starts += 1;
                            }
                        }
                        Kind::Firmware => self.firmware = Some(path),
                        Kind::Image => self.imports.push(path),
                    },
                    Action::Error(error) => self.status = error,
                    Action::Done => {}
                }
            }
        }
    }

    #[test]
    fn current_context_start_flag_and_context_mismatch_are_enforced() {
        for (captured, current) in [
            (context("a", "same", true), context("b", "same", false)),
            (context("a", "old", true), context("a", "new", false)),
        ] {
            let mut req = Requests::default();
            let id = req
                .begin(Kind::Rom, captured, Capability::Available)
                .unwrap();
            assert_eq!(
                req.finish(id, Kind::Rom, Outcome::Selected("rom.nes".into()), &current),
                None
            );
        }
        let mut req = Requests::default();
        let current = context("a", "game", false);
        let id = req
            .begin(Kind::Rom, context("a", "game", true), Capability::Available)
            .unwrap();
        assert_eq!(
            req.finish(id, Kind::Rom, Outcome::Selected("rom.nes".into()), &current),
            Some(Action::Selected(
                Kind::Rom,
                context("a", "game", true),
                "rom.nes".into()
            ))
        );
    }

    #[test]
    fn stale_duplicate_and_invalidated_results_have_no_host_side_effects() {
        let mut host = FakeHost {
            current: context("p", "g", false),
            ..FakeHost::default()
        };
        let old = host
            .requests
            .begin(Kind::Rom, context("p", "g", true), Capability::Available)
            .unwrap();
        assert_eq!(
            host.requests
                .begin(Kind::Rom, context("p", "g", false), Capability::Available),
            Err(BeginError::AlreadyPending)
        );
        host.requests.invalidate();
        let new = host
            .requests
            .begin(Kind::Rom, context("p", "g", false), Capability::Available)
            .unwrap();
        for result in [
            Outcome::Selected("stale".into()),
            Outcome::Failed("stale error".into()),
            Outcome::Cancelled,
        ] {
            host.deliver(old, Kind::Rom, result);
        }
        host.deliver(new, Kind::Rom, Outcome::Selected("fresh".into()));
        host.deliver(new, Kind::Rom, Outcome::Failed("duplicate".into()));
        assert_eq!(host.overrides, [PathBuf::from("fresh")]);
        assert_eq!(host.starts, 0);
        assert_eq!(host.status, "");

        let id = host
            .requests
            .begin(Kind::Rom, context("p", "g", true), Capability::Available)
            .unwrap();
        host.requests.invalidate();
        let replacement = host
            .requests
            .begin(Kind::Rom, context("p", "g", false), Capability::Available)
            .unwrap();
        host.deliver(id, Kind::Rom, Outcome::Selected("back-to-same".into()));
        host.deliver(
            replacement,
            Kind::Rom,
            Outcome::Selected("not-started".into()),
        );
        assert_eq!(host.overrides.len(), 2);
        assert_eq!(host.starts, 0);
    }

    #[test]
    fn close_and_native_callback_mapping() {
        for kind in [Kind::Rom, Kind::Firmware, Kind::Image] {
            let mut req = Requests::default();
            let id = req
                .begin(kind, context("p", "g", false), Capability::Available)
                .unwrap();
            req.close();
            assert_eq!(
                req.begin(kind, context("p", "g", false), Capability::Available),
                Err(BeginError::Closed)
            );
            assert_eq!(
                req.finish(
                    id,
                    kind,
                    Outcome::Selected("late".into()),
                    &context("p", "g", false)
                ),
                None
            );
        }
        assert_eq!(map_dialog(Ok(vec![])), Outcome::Cancelled);
        assert_eq!(
            map_dialog(Err(sdl3::dialog::DialogError::Canceled)),
            Outcome::Cancelled
        );
        assert_eq!(
            map_dialog(Ok(vec!["first".into(), "second".into()])),
            Outcome::Selected("first".into())
        );
        assert!(matches!(
            map_dialog(Err(sdl3::dialog::DialogError::SdlError(sdl3::get_error()))),
            Outcome::Failed(_)
        ));
    }

    #[test]
    fn shared_counter_rejects_cross_kind_stale_and_wrong_kind_without_consuming() {
        for first in [Kind::Rom, Kind::Firmware, Kind::Image] {
            for second in [Kind::Rom, Kind::Firmware, Kind::Image] {
                if first == second {
                    continue;
                }
                let mut host = FakeHost {
                    current: context("p", "g", false),
                    ..FakeHost::default()
                };
                let old = host
                    .requests
                    .begin(first, context("p", "g", false), Capability::Available)
                    .unwrap();
                host.requests.invalidate();
                let current = host
                    .requests
                    .begin(
                        second,
                        context("p", "g", second == Kind::Rom),
                        Capability::Available,
                    )
                    .unwrap();
                assert_ne!(old, current);
                for result in [
                    Outcome::Selected("stale".into()),
                    Outcome::Failed("stale error".into()),
                    Outcome::Cancelled,
                ] {
                    host.deliver(old, first, result);
                }
                host.deliver(current, first, Outcome::Selected("wrong".into()));
                assert!(host.overrides.is_empty());
                assert_eq!(host.starts, 0);
                assert_eq!(host.status, "");
                assert_eq!(host.firmware, None);
                assert!(host.imports.is_empty());
                host.deliver(current, second, Outcome::Selected("right".into()));
                assert_eq!(host.status, "");
                if second == Kind::Rom {
                    assert_eq!(host.overrides, [PathBuf::from("right")]);
                    assert_eq!(host.starts, 1);
                    assert_eq!(host.firmware, None);
                } else if second == Kind::Firmware {
                    assert!(host.overrides.is_empty());
                    assert_eq!(host.starts, 0);
                    assert_eq!(host.firmware, Some(PathBuf::from("right")));
                    assert!(host.imports.is_empty());
                } else {
                    assert!(host.overrides.is_empty());
                    assert_eq!(host.starts, 0);
                    assert_eq!(host.firmware, None);
                    assert_eq!(host.imports, [PathBuf::from("right")]);
                }
            }
        }
    }

    #[test]
    fn firmware_results_require_current_profile_and_game() {
        for current in [
            context("other-profile", "g", false),
            context("p", "other-game", false),
        ] {
            let mut req = Requests::default();
            let id = req
                .begin(
                    Kind::Firmware,
                    context("p", "g", false),
                    Capability::Available,
                )
                .unwrap();
            assert_eq!(
                req.finish(
                    id,
                    Kind::Firmware,
                    Outcome::Selected("folder".into()),
                    &current
                ),
                None
            );
        }
        let mut req = Requests::default();
        assert_eq!(
            req.begin(
                Kind::Firmware,
                context("p", "g", false),
                Capability::Unsupported
            ),
            Err(BeginError::Unsupported)
        );
        let id = req
            .begin(Kind::Rom, context("p", "g", true), Capability::Available)
            .unwrap();
        assert_eq!(
            req.finish(
                id,
                Kind::Rom,
                Outcome::Selected("rom.nes".into()),
                &context("p", "g", false)
            ),
            Some(Action::Selected(
                Kind::Rom,
                context("p", "g", true),
                "rom.nes".into()
            ))
        );
    }

    #[test]
    fn fake_host_firmware_is_updated_only_by_eligible_selection() {
        for (result, expected_status) in [
            (Outcome::Cancelled, ""),
            (Outcome::Failed("failure".into()), "failure"),
            (Outcome::Denied("denied".into()), "denied"),
        ] {
            let mut host = FakeHost {
                current: context("p", "g", false),
                ..FakeHost::default()
            };
            let id = host
                .requests
                .begin(
                    Kind::Firmware,
                    context("p", "g", false),
                    Capability::Available,
                )
                .unwrap();
            assert_eq!(
                host.requests
                    .begin(Kind::Rom, context("p", "g", true), Capability::Available),
                Err(BeginError::AlreadyPending)
            );
            host.deliver(id, Kind::Firmware, result.clone());
            let before = (
                host.firmware.clone(),
                host.overrides.clone(),
                host.starts,
                host.status.clone(),
            );
            assert_eq!(host.status, expected_status);
            assert_eq!(host.firmware, None);
            assert!(host.overrides.is_empty());
            assert_eq!(host.starts, 0);
            host.deliver(id, Kind::Firmware, Outcome::Selected("duplicate".into()));
            assert_eq!(
                (host.firmware, host.overrides, host.starts, host.status),
                before
            );
        }

        for (profile, game) in [("other", "g"), ("p", "other")] {
            let mut host = FakeHost {
                current: context(profile, game, false),
                status: "prior".into(),
                ..FakeHost::default()
            };
            let id = host
                .requests
                .begin(
                    Kind::Firmware,
                    context("p", "g", false),
                    Capability::Available,
                )
                .unwrap();
            host.deliver(id, Kind::Firmware, Outcome::Failed("stale error".into()));
            assert_eq!(host.status, "prior");
            assert_eq!(host.firmware, None);
            assert!(host.overrides.is_empty());
            assert_eq!(host.starts, 0);
        }

        let mut host = FakeHost {
            current: context("p", "g", false),
            ..FakeHost::default()
        };
        let rom = host
            .requests
            .begin(Kind::Rom, context("p", "g", true), Capability::Available)
            .unwrap();
        host.deliver(rom, Kind::Rom, Outcome::Selected("start.nes".into()));
        assert_eq!(host.overrides, [PathBuf::from("start.nes")]);
        assert_eq!(host.starts, 1);
    }

    #[test]
    fn image_selection_is_distinct_context_bound_and_never_starts_rom() {
        let mut req = Requests::default();
        let captured = context("p", "g", false);
        let id = req
            .begin(Kind::Image, captured.clone(), Capability::Available)
            .unwrap();
        assert_eq!(
            req.finish(
                id,
                Kind::Image,
                Outcome::Selected("archive.hsqs".into()),
                &captured
            ),
            Some(Action::Selected(
                Kind::Image,
                context("p", "g", false),
                "archive.hsqs".into()
            ))
        );

        for current in [
            context("other-profile", "g", false),
            context("p", "other", false),
        ] {
            let id = req
                .begin(Kind::Image, context("p", "g", false), Capability::Available)
                .unwrap();
            assert_eq!(
                req.finish(
                    id,
                    Kind::Image,
                    Outcome::Selected("stale.hsqs".into()),
                    &current
                ),
                None
            );
        }
    }

    #[test]
    fn image_wrong_kind_and_image_to_rom_firmware_stale_results_do_not_consume_current_request() {
        for next_kind in [Kind::Rom, Kind::Firmware] {
            let mut req = Requests::default();
            let old = req
                .begin(Kind::Image, context("p", "g", false), Capability::Available)
                .unwrap();
            req.invalidate();
            let current = req
                .begin(
                    next_kind,
                    context("p", "g", next_kind == Kind::Rom),
                    Capability::Available,
                )
                .unwrap();
            assert_ne!(old, current);
            for result in [
                Outcome::Selected("old".into()),
                Outcome::Failed("old error".into()),
                Outcome::Cancelled,
            ] {
                assert_eq!(
                    req.finish(old, Kind::Image, result, &context("p", "g", false)),
                    None
                );
            }
            let wrong_kind = if next_kind == Kind::Rom {
                Kind::Firmware
            } else {
                Kind::Rom
            };
            assert_eq!(
                req.finish(
                    current,
                    wrong_kind,
                    Outcome::Failed("wrong kind".into()),
                    &context("p", "g", false)
                ),
                None
            );
            let outcome = req.finish(
                current,
                next_kind,
                Outcome::Selected("current".into()),
                &context("p", "g", false),
            );
            assert!(matches!(outcome, Some(Action::Selected(kind, _, _)) if kind == next_kind));
        }
    }

    #[test]
    fn image_context_away_and_back_stays_invalidated_and_pending_requests_are_singleflight() {
        let mut req = Requests::default();
        let id = req
            .begin(Kind::Image, context("p", "g", false), Capability::Available)
            .unwrap();
        for kind in [Kind::Rom, Kind::Firmware] {
            assert_eq!(
                req.begin(kind, context("p", "g", true), Capability::Available),
                Err(BeginError::AlreadyPending)
            );
        }
        assert_eq!(
            req.finish(
                id,
                Kind::Image,
                Outcome::Selected("accepted.hsqs".into()),
                &context("p", "g", false)
            ),
            Some(Action::Selected(
                Kind::Image,
                context("p", "g", false),
                "accepted.hsqs".into()
            ))
        );
        let stale = req
            .begin(Kind::Image, context("p", "g", false), Capability::Available)
            .unwrap();
        req.invalidate();
        req.begin(
            Kind::Rom,
            context("p", "other", true),
            Capability::Available,
        )
        .unwrap();
        req.invalidate();
        req.begin(Kind::Image, context("p", "g", false), Capability::Available)
            .unwrap();
        assert_eq!(
            req.finish(
                stale,
                Kind::Image,
                Outcome::Selected("stale".into()),
                &context("p", "g", false)
            ),
            None
        );
    }

    #[test]
    fn all_three_request_kinds_use_unique_ids_from_the_shared_counter() {
        let mut req = Requests::default();
        let mut ids = Vec::new();
        for kind in [Kind::Rom, Kind::Firmware, Kind::Image] {
            let id = req
                .begin(kind, context("p", "g", false), Capability::Available)
                .unwrap();
            ids.push(id);
            assert_eq!(
                req.finish(id, kind, Outcome::Cancelled, &context("p", "g", false)),
                Some(Action::Done)
            );
        }
        assert_ne!(ids[0], ids[1]);
        assert_ne!(ids[0], ids[2]);
        assert_ne!(ids[1], ids[2]);
    }

    #[test]
    fn image_errors_and_cancellation_are_scoped_and_typed() {
        let mut req = Requests::default();
        for (result, expected) in [
            (Outcome::Cancelled, Action::Done),
            (
                Outcome::Failed("image error".into()),
                Action::Error("image error".into()),
            ),
            (
                Outcome::Denied("image denied".into()),
                Action::Error("image denied".into()),
            ),
        ] {
            let id = req
                .begin(Kind::Image, context("p", "g", false), Capability::Available)
                .unwrap();
            assert_eq!(
                req.finish(id, Kind::Image, result, &context("p", "g", false)),
                Some(expected)
            );
        }
    }
}
