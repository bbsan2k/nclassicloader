use crate::frontend::launcher_frame;
use crate::frontend::requests::{Capability, Outcome, RequestId, RequestTracker};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Image,
    PreparedFolder,
}

#[derive(Default)]
pub(super) struct Requests {
    tracker: RequestTracker,
    pending_kind: Option<Kind>,
}
impl Requests {
    pub(super) fn begin(
        &mut self,
        kind: Kind,
        capability: Capability,
    ) -> Result<RequestId, crate::frontend::requests::BeginError> {
        let id = self.tracker.begin(capability)?;
        self.pending_kind = Some(kind);
        Ok(id)
    }
    pub(super) fn cancel(&mut self, id: RequestId) -> bool {
        let ok = self.tracker.cancel(id);
        if ok {
            self.pending_kind = None;
        }
        ok
    }
    pub(super) fn invalidate(&mut self) {
        self.tracker.invalidate();
        self.pending_kind = None;
    }
    pub(super) fn close(&mut self) {
        self.tracker.close();
        self.pending_kind = None;
    }
}

pub(super) enum Action {
    Done,
    Image(PathBuf),
    Assets(PathBuf),
    Error(String),
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
pub(super) fn finish(
    requests: &mut Requests,
    id: RequestId,
    kind: Kind,
    outcome: Outcome<PathBuf, String>,
    _busy: bool,
    _closing: bool,
) -> Option<Action> {
    if requests.pending_kind != Some(kind) {
        return None;
    }
    let outcome = requests.tracker.complete(id, outcome)?;
    requests.pending_kind = None;
    Some(match outcome {
        Outcome::Selected(_) if _closing => Action::Done,
        Outcome::Selected(path) => match kind {
            Kind::Image => Action::Image(path),
            Kind::PreparedFolder => Action::Assets(path),
        },
        Outcome::Cancelled => Action::Done,
        Outcome::Denied(e) | Outcome::Failed(e) => Action::Error(e),
    })
}
pub(super) fn accept_queued_source(busy: bool, closing: bool, picker_pending: bool) -> bool {
    launcher_frame::source_allowed(busy, closing, picker_pending)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeHost {
        requests: Requests,
        picker_pending: bool,
        status: String,
        images: Vec<PathBuf>,
        folders: Vec<PathBuf>,
        reseeds: usize,
    }
    impl FakeHost {
        fn new() -> Self {
            Self {
                requests: Requests::default(),
                picker_pending: false,
                status: "idle".into(),
                images: vec![],
                folders: vec![],
                reseeds: 0,
            }
        }
        fn begin(&mut self, kind: Kind, capability: Capability) -> RequestId {
            let id = self.requests.begin(kind, capability).unwrap();
            self.picker_pending = true;
            id
        }
        fn deliver(
            &mut self,
            id: RequestId,
            kind: Kind,
            outcome: Outcome<PathBuf, String>,
            busy: bool,
            closing: bool,
        ) {
            let Some(action) = finish(&mut self.requests, id, kind, outcome, busy, closing) else {
                return;
            };
            self.picker_pending = false;
            self.reseeds += 1;
            match action {
                Action::Image(path) => self.images.push(path),
                Action::Assets(path) => self.folders.push(path),
                Action::Error(message) => self.status = message,
                Action::Done => {}
            }
        }
    }
    #[test]
    fn native_result_mapping_is_distinct_and_selects_first() {
        assert!(matches!(map_dialog(Ok(vec![])), Outcome::Cancelled));
        assert!(matches!(
            map_dialog(Err(sdl3::dialog::DialogError::Canceled)),
            Outcome::Cancelled
        ));
        assert!(matches!(
            map_dialog(Err(sdl3::dialog::DialogError::SdlError(sdl3::get_error()))),
            Outcome::Failed(_)
        ));
        assert_eq!(
            map_dialog(Ok(vec!["a".into(), "b".into()])),
            Outcome::Selected("a".into())
        );
    }
    #[test]
    fn fake_host_rejects_stale_wrong_kind_and_duplicate_without_mutation() {
        for (old_kind, new_kind) in [
            (Kind::Image, Kind::PreparedFolder),
            (Kind::PreparedFolder, Kind::Image),
        ] {
            let mut h = FakeHost::new();
            let old = h.begin(old_kind, Capability::Available);
            assert!(h.requests.cancel(old));
            let current = h.begin(new_kind, Capability::Available);
            assert_ne!(old, current);
            assert!(!h.requests.cancel(old));
            for outcome in [
                Outcome::Selected("stale".into()),
                Outcome::Failed("stale error".into()),
                Outcome::Cancelled,
            ] {
                h.deliver(old, old_kind, outcome, false, false);
            }
            h.deliver(
                current,
                if new_kind == Kind::Image {
                    Kind::PreparedFolder
                } else {
                    Kind::Image
                },
                Outcome::Selected("wrong".into()),
                false,
                false,
            );
            assert!(h.picker_pending);
            assert_eq!(h.status, "idle");
            assert_eq!(h.reseeds, 0);
            assert!(h.images.is_empty());
            assert!(h.folders.is_empty());
            assert_eq!(h.requests.pending_kind, Some(new_kind));
            h.deliver(
                current,
                new_kind,
                Outcome::Selected("accepted".into()),
                false,
                false,
            );
            assert!(!h.picker_pending);
            assert_eq!(h.reseeds, 1);
            assert_eq!(h.images.len(), usize::from(new_kind == Kind::Image));
            assert_eq!(
                h.folders.len(),
                usize::from(new_kind == Kind::PreparedFolder)
            );
            h.deliver(
                current,
                new_kind,
                Outcome::Selected("duplicate".into()),
                false,
                false,
            );
            assert_eq!(h.reseeds, 1);
            assert_eq!(h.images.len() + h.folders.len(), 1);
        }
    }
    #[test]
    fn capabilities_closing_busy_and_source_guard() {
        let mut h = FakeHost::new();
        assert!(
            h.requests
                .begin(Kind::PreparedFolder, Capability::Unsupported)
                .is_err()
        );
        let image = h.begin(Kind::Image, Capability::Available);
        assert_eq!(h.requests.pending_kind, Some(Kind::Image));
        // Focus/controller transitions only reseed inputs; neither changes request eligibility.
        h.reseeds += 1;
        h.reseeds -= 1;
        assert_eq!(h.requests.pending_kind, Some(Kind::Image));
        for kind in [Kind::Image, Kind::PreparedFolder] {
            for busy in [false, true] {
                for closing in [false, true] {
                    let mut r = Requests::default();
                    let id = r.begin(kind, Capability::Available).unwrap();
                    let selected = finish(
                        &mut r,
                        id,
                        kind,
                        Outcome::Selected("chosen".into()),
                        busy,
                        closing,
                    )
                    .unwrap();
                    if closing {
                        assert!(matches!(selected, Action::Done));
                    } else {
                        assert!(matches!(
                            (kind, selected),
                            (Kind::Image, Action::Image(_))
                                | (Kind::PreparedFolder, Action::Assets(_))
                        ));
                    }
                }
            }
        }
        for outcome in [
            Outcome::Cancelled,
            Outcome::Denied("denied".into()),
            Outcome::Failed("failed".into()),
        ] {
            let mut r = Requests::default();
            let id = r.begin(Kind::Image, Capability::Available).unwrap();
            let result = finish(&mut r, id, Kind::Image, outcome, false, false).unwrap();
            assert!(matches!(result, Action::Done | Action::Error(_)));
        }
        for busy in [false, true] {
            for closing in [false, true] {
                for pending in [false, true] {
                    assert_eq!(
                        accept_queued_source(busy, closing, pending),
                        !busy && !closing && !pending
                    );
                }
            }
        }
        assert!(h.requests.cancel(image));
        h.picker_pending = false;
        let id = h.begin(Kind::PreparedFolder, Capability::Available);
        h.requests.close();
        assert!(
            h.requests
                .begin(Kind::Image, Capability::Available)
                .is_err()
        );
        assert!(
            finish(
                &mut h.requests,
                id,
                Kind::PreparedFolder,
                Outcome::Selected("late".into()),
                false,
                false
            )
            .is_none()
        );
    }
}
