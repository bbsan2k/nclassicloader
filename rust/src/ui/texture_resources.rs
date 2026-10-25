use std::sync::atomic::{AtomicUsize, Ordering};

use crate::frontend::menu_scene::TextureId;

static NEXT_GENERATION: AtomicUsize = AtomicUsize::new(1);

fn allocate_generation(counter: &AtomicUsize) -> Result<usize, String> {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| "Texture owner generation counter exhausted".to_owned())
}

pub(super) struct TextureBindings {
    generation: usize,
    keys: Vec<String>,
    by_key: std::collections::HashMap<String, TextureId>,
}

impl TextureBindings {
    pub(super) fn generation(&self) -> usize {
        self.generation
    }
    pub(super) fn new() -> Result<Self, String> {
        Ok(Self::with_generation(allocate_generation(
            &NEXT_GENERATION,
        )?))
    }

    fn with_generation(generation: usize) -> Self {
        Self {
            generation,
            keys: Vec::new(),
            by_key: std::collections::HashMap::new(),
        }
    }

    pub(super) fn bind(&mut self, key: &str) -> TextureId {
        if let Some(id) = self.by_key.get(key) {
            return *id;
        }
        let id = TextureId {
            generation: self.generation,
            slot: self.keys.len(),
        };
        self.keys.push(key.to_owned());
        self.by_key.insert(key.to_owned(), id);
        id
    }

    pub(super) fn id(&self, key: &str) -> Option<TextureId> {
        self.by_key.get(key).copied()
    }

    pub(super) fn key(&self, id: TextureId) -> Option<&str> {
        if id.generation != self.generation {
            return None;
        }
        self.keys.get(id.slot).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::{TextureBindings, allocate_generation};
    use crate::frontend::menu_scene::TextureId;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn bindings_are_stable_and_reject_unknown_and_foreign_ids() {
        let mut bindings = TextureBindings::with_generation(10);
        let first = bindings.bind("atlas");
        assert_eq!(first, bindings.bind("atlas"));
        assert_eq!(bindings.id("atlas"), Some(first));
        assert_eq!(bindings.key(first), Some("atlas"));
        assert_eq!(bindings.id("missing"), None);
        assert_eq!(
            bindings.key(TextureId {
                generation: 10,
                slot: 1
            }),
            None
        );
        assert_eq!(
            bindings.key(TextureId {
                generation: 11,
                slot: 0
            }),
            None
        );

        let mut replacement = TextureBindings::with_generation(11);
        let fresh = replacement.bind("atlas");
        assert_ne!(fresh, first);
        assert_eq!(replacement.key(first), None);
    }

    #[test]
    fn generation_exhaustion_is_checked_without_touching_global_counter() {
        let counter = AtomicUsize::new(usize::MAX);
        assert!(allocate_generation(&counter).is_err());
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::Relaxed),
            usize::MAX
        );
        let counter = AtomicUsize::new(7);
        assert_eq!(allocate_generation(&counter).unwrap(), 7);
        assert_eq!(counter.load(std::sync::atomic::Ordering::Relaxed), 8);
        let counter = AtomicUsize::new(usize::MAX - 1);
        assert_eq!(allocate_generation(&counter).unwrap(), usize::MAX - 1);
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::Relaxed),
            usize::MAX
        );
        assert!(allocate_generation(&counter).is_err());
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::Relaxed),
            usize::MAX
        );
    }

    #[test]
    fn new_owners_have_unique_generations_and_reject_old_ids() {
        let mut first = TextureBindings::new().unwrap();
        let old = first.bind("synthetic");
        let mut second = TextureBindings::new().unwrap();
        let current = second.bind("synthetic");
        assert_ne!(old.generation, current.generation);
        assert_ne!(old, current);
        assert_eq!(second.key(old), None);
        assert_eq!(first.key(current), None);
    }
}
