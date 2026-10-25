use std::collections::HashSet;

pub(super) use crate::frontend::menu_panel::{PanelKey, PanelScene, PanelStep};

pub(super) struct PanelBindings {
    pub generation: usize,
    allowed: HashSet<String>,
}

impl PanelBindings {
    pub fn new(generation: usize, actions: impl IntoIterator<Item = PanelKey>) -> Self {
        Self {
            generation,
            allowed: actions.into_iter().map(|key| key.0).collect(),
        }
    }

    pub fn resolve(&self, key: &PanelKey) -> Option<String> {
        self.allowed.contains(&key.0).then(|| key.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(value: &str) -> PanelKey {
        PanelKey(value.into())
    }

    #[test]
    fn captured_keys_resolve_only_against_current_panel_entries_not_catalog_indices() {
        let initial = PanelBindings::new(
            17,
            [key("profile:alpha"), key("profile:beta"), key("close")],
        );
        let reordered = PanelBindings::new(
            17,
            [key("close"), key("profile:beta"), key("profile:alpha")],
        );
        for context in [&initial, &reordered] {
            assert_eq!(context.generation, 17);
            assert_eq!(
                context.resolve(&key("profile:alpha")).as_deref(),
                Some("profile:alpha")
            );
            assert_eq!(context.resolve(&key("profile:missing")), None);
            assert_eq!(context.resolve(&key("Profile:alpha")), None);
            assert_eq!(context.resolve(&key("language:usa_en")), None);
        }
        let switched = PanelBindings::new(17, [key("language:usa_en"), key("close")]);
        assert_eq!(switched.resolve(&key("profile:alpha")), None);
        assert_eq!(
            switched.resolve(&key("language:usa_en")).as_deref(),
            Some("language:usa_en")
        );
    }

    #[test]
    fn disabled_import_footer_is_not_an_allowed_hit_or_keyboard_action() {
        let enabled = PanelBindings::new(18, [key("dismiss-import"), key("cancel-import")]);
        assert_eq!(
            enabled.resolve(&key("cancel-import")).as_deref(),
            Some("cancel-import")
        );
        let finishing = PanelBindings::new(18, [key("dismiss-import")]);
        assert_eq!(
            finishing.resolve(&key("dismiss-import")).as_deref(),
            Some("dismiss-import")
        );
        for disallowed in ["cancel-import", "close", "rom", "save", "import-image"] {
            assert_eq!(finishing.resolve(&key(disallowed)), None);
        }
        assert_eq!(
            PanelBindings::new(18, []).resolve(&key("dismiss-import")),
            None
        );
    }
}
