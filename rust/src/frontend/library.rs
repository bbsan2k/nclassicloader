//! Host-independent launcher library filtering and selection policy.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SystemFamily {
    #[default]
    Unknown,
    Nes,
    Snes,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibraryFilter {
    #[default]
    All,
    Nes,
    Snes,
}

impl LibraryFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Nes => "NES",
            Self::Snes => "SNES",
        }
    }
    pub fn from_label(label: Option<&str>) -> Self {
        match label {
            Some("NES") => Self::Nes,
            Some("SNES") => Self::Snes,
            _ => Self::All,
        }
    }
    pub fn next(self, direction: i32) -> Self {
        let i = match self {
            Self::All => 0i32,
            Self::Nes => 1,
            Self::Snes => 2,
        };
        [Self::All, Self::Nes, Self::Snes][((i + direction.rem_euclid(3)).rem_euclid(3)) as usize]
    }
    pub fn matches(self, family: SystemFamily) -> bool {
        match self {
            Self::All => true,
            Self::Nes => family == SystemFamily::Nes,
            Self::Snes => family == SystemFamily::Snes,
        }
    }
}

pub fn visible_entries<T>(
    entries: &[T],
    classify: impl Fn(&T) -> SystemFamily,
    filter: LibraryFilter,
) -> Vec<&T> {
    entries
        .iter()
        .filter(|entry| filter.matches(classify(entry)))
        .collect()
}

pub fn change_filter<T, K: Eq>(
    entries: &[T],
    classify: impl Fn(&T) -> SystemFamily,
    key: impl Fn(&T) -> K,
    filter: &mut LibraryFilter,
    next: LibraryFilter,
    selected: &mut usize,
    focus: &mut Option<usize>,
) {
    let old = visible_entries(entries, &classify, *filter);
    let identity = old.get(*selected).map(|entry| key(entry));
    *filter = next;
    let new = visible_entries(entries, &classify, next);
    *selected = identity
        .and_then(|identity| new.iter().position(|entry| key(entry) == identity))
        .unwrap_or(0);
    *focus = None;
}

pub fn pad_action<T, K: Eq>(
    action: crate::frontend::launcher_controller::Action,
    entries: &[T],
    classify: impl Fn(&T) -> SystemFamily,
    key: impl Fn(&T) -> K,
    filter: &mut LibraryFilter,
    selected: &mut usize,
    focus: &mut Option<usize>,
) -> Option<i32> {
    use crate::frontend::launcher_controller::Action;
    if action == Action::Cancel {
        return Some(-1);
    }
    if matches!(action, Action::FilterPrevious | Action::FilterNext) {
        let next = filter.next(if action == Action::FilterPrevious {
            -1
        } else {
            1
        });
        change_filter(entries, classify, key, filter, next, selected, focus);
        return None;
    }
    action.navigate(
        selected,
        visible_entries(entries, classify, *filter).len(),
        focus,
    );
    action.activation(*focus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::launcher_controller::Action;

    #[derive(Debug, PartialEq, Eq)]
    struct Item(&'static str, SystemFamily);
    fn family(item: &Item) -> SystemFamily {
        item.1
    }
    fn key(item: &Item) -> &'static str {
        item.0
    }
    fn items() -> Vec<Item> {
        vec![
            Item("n1", SystemFamily::Nes),
            Item("s1", SystemFamily::Snes),
            Item("u1", SystemFamily::Unknown),
            Item("n2", SystemFamily::Nes),
        ]
    }

    #[test]
    fn classifies_mixed_and_unknown_families() {
        let entries = items();
        assert_eq!(
            visible_entries(&entries, family, LibraryFilter::All).len(),
            4
        );
        assert_eq!(
            visible_entries(&entries, family, LibraryFilter::Nes)
                .iter()
                .map(|x| x.0)
                .collect::<Vec<_>>(),
            ["n1", "n2"]
        );
        assert_eq!(
            visible_entries(&entries, family, LibraryFilter::Snes)
                .iter()
                .map(|x| x.0)
                .collect::<Vec<_>>(),
            ["s1"]
        );
    }

    #[test]
    fn labels_roundtrip_and_unknown_values_default_to_all() {
        for filter in [LibraryFilter::All, LibraryFilter::Nes, LibraryFilter::Snes] {
            assert_eq!(LibraryFilter::from_label(Some(filter.label())), filter);
        }
        assert_eq!(LibraryFilter::All.label(), "ALL");
        assert_eq!(LibraryFilter::Nes.label(), "NES");
        assert_eq!(LibraryFilter::Snes.label(), "SNES");
        for label in [None, Some("nes"), Some("snes"), Some("OTHER"), Some("")] {
            assert_eq!(LibraryFilter::from_label(label), LibraryFilter::All);
        }
    }

    #[test]
    fn cycle_wraps_in_both_directions_and_handles_i32_extremes() {
        assert_eq!(LibraryFilter::All.next(1), LibraryFilter::Nes);
        assert_eq!(LibraryFilter::All.next(-1), LibraryFilter::Snes);
        assert_eq!(LibraryFilter::Nes.next(1), LibraryFilter::Snes);
        assert_eq!(LibraryFilter::Snes.next(1), LibraryFilter::All);
        for filter in [LibraryFilter::All, LibraryFilter::Nes, LibraryFilter::Snes] {
            assert_eq!(filter.next(0), filter);
            for direction in [i32::MIN, i32::MAX, 1_000_000_000, -1_000_000_000] {
                let expected = (match filter {
                    LibraryFilter::All => 0i64,
                    LibraryFilter::Nes => 1,
                    LibraryFilter::Snes => 2,
                } + i64::from(direction))
                .rem_euclid(3);
                assert_eq!(
                    filter.next(direction),
                    [LibraryFilter::All, LibraryFilter::Nes, LibraryFilter::Snes]
                        [expected as usize]
                );
            }
        }
    }

    #[test]
    fn filter_change_retains_visible_identity_or_resets_and_clears_focus() {
        let entries = items();
        let (mut filter, mut selected, mut focus) = (LibraryFilter::All, 3, Some(2));
        change_filter(
            &entries,
            family,
            key,
            &mut filter,
            LibraryFilter::Nes,
            &mut selected,
            &mut focus,
        );
        assert_eq!((filter, selected, focus), (LibraryFilter::Nes, 1, None));
        // The selected NES identity remains at its new position across switches.
        change_filter(
            &entries,
            family,
            key,
            &mut filter,
            LibraryFilter::All,
            &mut selected,
            &mut focus,
        );
        assert_eq!(selected, 3);
        // Selected entry excluded from the destination resets to zero.
        change_filter(
            &entries,
            family,
            key,
            &mut filter,
            LibraryFilter::Snes,
            &mut selected,
            &mut focus,
        );
        assert_eq!(selected, 0);
        // Invalid selection and empty input are safe and reset to zero.
        selected = usize::MAX;
        change_filter(
            &entries,
            family,
            key,
            &mut filter,
            LibraryFilter::All,
            &mut selected,
            &mut focus,
        );
        assert_eq!((selected, focus), (0, None));
        selected = 7;
        change_filter::<Item, &str>(
            &[],
            family,
            key,
            &mut filter,
            LibraryFilter::Nes,
            &mut selected,
            &mut focus,
        );
        assert_eq!((selected, focus), (0, None));
    }

    #[test]
    fn controller_actions_preserve_launcher_results_and_filtered_navigation() {
        let entries = items();
        let (mut filter, mut selected, mut focus) = (LibraryFilter::All, 0, None);
        assert_eq!(
            pad_action(
                Action::Cancel,
                &entries,
                family,
                key,
                &mut filter,
                &mut selected,
                &mut focus
            ),
            Some(-1)
        );
        assert_eq!(
            pad_action(
                Action::Confirm,
                &entries,
                family,
                key,
                &mut filter,
                &mut selected,
                &mut focus
            ),
            Some(0)
        );
        focus = Some(2);
        assert_eq!(
            pad_action(
                Action::Confirm,
                &entries,
                family,
                key,
                &mut filter,
                &mut selected,
                &mut focus
            ),
            Some(2)
        );
        assert_eq!(
            pad_action(
                Action::Import,
                &entries,
                family,
                key,
                &mut filter,
                &mut selected,
                &mut focus
            ),
            Some(1)
        );
        assert_eq!(
            pad_action(
                Action::OpenFolder,
                &entries,
                family,
                key,
                &mut filter,
                &mut selected,
                &mut focus
            ),
            Some(2)
        );
        assert_eq!(
            pad_action(
                Action::FilterNext,
                &entries,
                family,
                key,
                &mut filter,
                &mut selected,
                &mut focus
            ),
            None
        );
        assert_eq!((filter, selected, focus), (LibraryFilter::Nes, 0, None));
        assert_eq!(
            pad_action(
                Action::LibraryNext,
                &entries,
                family,
                key,
                &mut filter,
                &mut selected,
                &mut focus
            ),
            None
        );
        assert_eq!(selected, 1);
        let mut empty_selected = 99;
        assert_eq!(
            pad_action(
                Action::LibraryNext,
                &[],
                family,
                key,
                &mut filter,
                &mut empty_selected,
                &mut focus
            ),
            None
        );
        assert_eq!(empty_selected, 0);
    }
}
