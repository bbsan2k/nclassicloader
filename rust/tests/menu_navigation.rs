//! Headless fake-host traces for the shared main-menu, toolbar, and suspend policies.
use clover_portable::frontend::{
    menu_input::{Button, Key, KeyEvent, menu_key, suspend_input},
    menu_navigation::{
        self as menu, Context as MenuContext, Effect as MenuEffect, MenuFocus, Panel,
    },
    suspend_navigation::{self as suspend, Effect as SuspendEffect, Input as SuspendInput},
    toolbar_navigation::{self as toolbar, Context as ToolbarContext, Effect as ToolbarEffect},
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Intent {
    Menu(MenuEffect),
    Toolbar(ToolbarEffect),
    Suspend(SuspendEffect),
    NativeAction(String),
}

struct FakeHost {
    menu_state: menu::State,
    menu_context: MenuContext,
    toolbar_state: toolbar::State,
    toolbar_context: ToolbarContext,
    suspend_state: suspend::State,
    actions: Vec<String>,
    languages: Vec<&'static str>,
    legal_line_count: usize,
    has_pending: bool,
    hover: bool,
    cancel_requested: bool,
    game: usize,
    intents: Vec<Intent>,
    resource_operations: usize,
}

impl FakeHost {
    fn new(panel: Panel) -> Self {
        Self {
            menu_state: menu::State {
                menu_focus: MenuFocus::Games,
                focus: 0,
                toolbar_index: 0,
            },
            menu_context: MenuContext {
                playing: false,
                owned_import_notice: false,
                panel,
                action_count: 0,
            },
            toolbar_state: toolbar::State {
                focus: 0,
                scroll: 0,
            },
            toolbar_context: ToolbarContext {
                page: 0,
                nes: false,
                display_mode: 1,
                language_count: 0,
                legal_line_count: 0,
            },
            suspend_state: suspend::State {
                slot: 0,
                focus: 0,
                controls: false,
            },
            actions: Vec::new(),
            languages: vec!["en", "fr", "de", "it", "ja", "es", "nl", "pt", "ru"],
            legal_line_count: 24,
            has_pending: false,
            hover: true,
            cancel_requested: false,
            game: 0,
            intents: Vec::new(),
            resource_operations: 0,
        }
    }

    fn tick(&mut self, event: KeyEvent) {
        let plan = menu::transition(self.menu_state, self.menu_context, event);
        self.menu_state = plan.state;
        if plan.clear_hover {
            self.hover = false;
        }
        let Some(effect) = plan.effect else { return };
        if effect == MenuEffect::DelegateToolbar {
            self.toolbar_tick(event);
            return;
        }
        if effect == MenuEffect::DelegateSuspend {
            self.suspend_tick(event);
            return;
        }
        self.intents.push(Intent::Menu(effect));
        match effect {
            MenuEffect::OpenProfiles => {
                self.menu_context.panel = Panel::Other;
                self.actions = vec!["close".into()];
                self.menu_context.action_count = self.actions.len();
            }
            MenuEffect::CancelImport => {
                self.cancel_requested = true;
                // A cancellation request is not an acknowledgement: the notice remains owned.
            }
            MenuEffect::MoveGame(direction) => {
                self.game = (self.game as i32 + direction).rem_euclid(5) as usize;
            }
            MenuEffect::OpenSuspend => {
                self.menu_context.panel = Panel::Resume;
                self.menu_state.menu_focus = MenuFocus::Games;
                self.menu_state.focus = 0;
                self.suspend_state.controls = false;
            }
            MenuEffect::OpenToolbar(page) => {
                self.menu_state.menu_focus = MenuFocus::Toolbar;
                self.menu_state.toolbar_index = page;
                self.menu_context.panel = Panel::Toolbar;
                self.toolbar_context.page = page;
                self.toolbar_state.focus = 0;
                self.toolbar_state.scroll = 0;
            }
            MenuEffect::CancelConfirmationAndOpenSuspend => {
                self.menu_context.panel = Panel::Resume;
                self.menu_state.menu_focus = MenuFocus::Games;
            }
            MenuEffect::SetSettingsPanel => self.menu_context.panel = Panel::Toolbar,
            MenuEffect::SetHelpToolbarPanel => self.menu_context.panel = Panel::Toolbar,
            MenuEffect::ClosePanel => self.menu_context.panel = Panel::None,
            MenuEffect::ActivateFocused(index) => {
                if let Some(action) = self.actions.get(index) {
                    self.native_action(action.clone());
                }
            }
            MenuEffect::Fullscreen
            | MenuEffect::LeaveGame
            | MenuEffect::SaveSlot
            | MenuEffect::LoadSlot
            | MenuEffect::CancelSuspend
            | MenuEffect::ConfirmDelete
            | MenuEffect::ConfirmSave
            | MenuEffect::BeginGame
            | MenuEffect::Sort
            | MenuEffect::ToggleSound => {}
            MenuEffect::DelegateSuspend | MenuEffect::DelegateToolbar => unreachable!(),
        }
    }

    fn toolbar_tick(&mut self, event: KeyEvent) {
        let mut context = self.toolbar_context;
        context.language_count = if context.page == 2
            && matches!(
                event.key,
                Key::Left | Key::Right | Key::Up | Key::Down | Key::Next
            ) {
            self.languages.len()
        } else {
            0
        };
        context.legal_line_count = if context.page == 3 && event.key == Key::Down {
            self.legal_line_count
        } else {
            0
        };
        let plan = toolbar::transition(self.toolbar_state, context, event);
        self.toolbar_state = plan.state;
        let Some(effect) = plan.effect else { return };
        self.intents.push(Intent::Toolbar(effect));
        match effect {
            ToolbarEffect::DismissPanel => self.menu_context.panel = Panel::None,
            ToolbarEffect::Help => {
                self.native_action("help".into());
                self.menu_context.panel = Panel::Help;
            }
            ToolbarEffect::ActivateClose => {
                self.native_action("close".into());
                self.menu_context.panel = Panel::None;
            }
            ToolbarEffect::FrameNone => self.native_action("frame:none".into()),
            ToolbarEffect::DisplayMode(mode) => self.native_action(format!("display:{mode}")),
            ToolbarEffect::EmulatorSettings => {
                self.native_action("emulator-settings".into());
                self.menu_context.panel = Panel::EmulatorSettings;
            }
            ToolbarEffect::ChooseLanguage(index) => {
                let action = self
                    .languages
                    .get(index)
                    .map(|language| format!("language:{language}"))
                    .unwrap_or_else(|| "close".into());
                self.native_action(action);
            }
        }
    }

    fn suspend_tick(&mut self, event: KeyEvent) {
        let pending = event.key == Key::Confirm
            && !event.repeat
            && !self.suspend_state.controls
            && self.has_pending;
        let input = suspend_input(
            event,
            pending,
            self.suspend_state.controls,
            self.toolbar_context.nes,
        );
        let (state, effects) = suspend::transition(self.suspend_state, input);
        self.suspend_state = state;
        for effect in effects {
            self.intents.push(Intent::Suspend(effect));
            if effect == SuspendEffect::Dismiss {
                self.menu_context.panel = Panel::None;
                self.menu_state.menu_focus = MenuFocus::Games;
            }
            // Effect intents deliberately stop here; this host owns no paths, saves, or core.
        }
    }

    fn native_action(&mut self, action: String) {
        self.intents.push(Intent::NativeAction(action));
    }
}

fn event(key: Key, repeat: bool) -> KeyEvent {
    KeyEvent { key, repeat }
}

#[test]
fn production_main_priority_traces_keep_non_io_intents_explicit() {
    let mut host = FakeHost::new(Panel::Resume);
    host.menu_context.playing = true;
    host.menu_context.owned_import_notice = true;
    host.tick(event(Key::Fullscreen, false));
    assert_eq!(host.intents, [Intent::Menu(MenuEffect::Fullscreen)]);
    assert!(host.hover); // Fullscreen wins before the idle hover-clear path.

    host.intents.clear();
    host.tick(event(Key::Back, true));
    host.tick(event(Key::SaveSlot, true));
    host.tick(event(Key::LoadSlot, true));
    assert!(host.intents.is_empty());
    host.tick(event(Key::SaveSlot, false));
    assert_eq!(host.intents, [Intent::Menu(MenuEffect::SaveSlot)]);

    host.menu_context.playing = false;
    host.menu_context.panel = Panel::Other;
    host.tick(event(Key::Back, true));
    assert!(!host.cancel_requested);
    assert_eq!(host.menu_context.panel, Panel::Other);
    assert!(host.menu_context.owned_import_notice);
    host.tick(event(Key::Back, false));
    assert!(host.cancel_requested);
    assert!(host.menu_context.owned_import_notice);
    assert_eq!(host.menu_context.panel, Panel::Other);
    assert_eq!(
        host.intents.last(),
        Some(&Intent::Menu(MenuEffect::CancelImport))
    );

    host.menu_context.owned_import_notice = false;
    host.menu_context.panel = Panel::Resume;
    host.tick(event(Key::Profiles, false));
    assert_eq!(host.menu_context.panel, Panel::Other);
    assert_eq!(
        host.intents.last(),
        Some(&Intent::Menu(MenuEffect::OpenProfiles))
    );
}

#[test]
fn game_toolbar_panel_back_and_confirm_have_distinct_host_routes() {
    let mut host = FakeHost::new(Panel::None);
    host.tick(event(Key::Left, false));
    assert_eq!(host.game, 4);
    assert_eq!(
        host.intents.last(),
        Some(&Intent::Menu(MenuEffect::MoveGame(-1)))
    );
    host.tick(event(Key::Up, false));
    assert_eq!(host.menu_state.menu_focus, MenuFocus::Toolbar);
    host.tick(event(Key::Left, false));
    assert_eq!(host.menu_state.toolbar_index, 4);
    host.tick(event(Key::Confirm, false));
    assert_eq!(host.menu_context.panel, Panel::Toolbar);
    assert_eq!(host.toolbar_context.page, 4);
    host.tick(event(Key::Back, false));
    assert_eq!(host.menu_context.panel, Panel::None);
    assert_eq!(
        host.intents.last(),
        Some(&Intent::Toolbar(ToolbarEffect::DismissPanel))
    );
    assert!(!host.intents.contains(&Intent::NativeAction("close".into())));

    host.tick(event(Key::Confirm, false));
    assert_eq!(host.menu_context.panel, Panel::Toolbar);
    host.tick(event(Key::Confirm, false));
    assert_eq!(
        host.intents.last(),
        Some(&Intent::NativeAction("close".into()))
    );
    assert_eq!(host.menu_context.panel, Panel::None);
}

#[test]
fn independent_toolbar_pages_wrap_and_resolve_language_intents_in_host() {
    let mut host = FakeHost::new(Panel::Toolbar);
    host.toolbar_context.page = 2;
    host.languages.truncate(8);
    host.toolbar_state.focus = 3;
    host.tick(event(Key::Down, false));
    assert_eq!(host.toolbar_state.focus, 3);
    host.languages.push("ru");
    host.toolbar_state.focus = 3;
    host.tick(event(Key::Down, false));
    assert_eq!(host.toolbar_state.focus, 8);
    host.toolbar_state.focus = 8;
    host.tick(event(Key::Right, false));
    assert_eq!(host.toolbar_state.focus, 7);
    host.toolbar_state.focus = 8;
    host.tick(event(Key::Next, false));
    assert_eq!(host.toolbar_state.focus, 0);
    host.toolbar_state.focus = 99;
    host.tick(event(Key::Confirm, false));
    assert_eq!(
        host.intents.last(),
        Some(&Intent::NativeAction("close".into()))
    );

    host.menu_context.panel = Panel::Toolbar;
    host.toolbar_context.page = 3;
    host.legal_line_count = 18;
    host.toolbar_state.scroll = usize::MAX;
    host.tick(event(Key::Down, false));
    assert_eq!(host.toolbar_state.scroll, 2);
    host.legal_line_count = 12;
    host.tick(event(Key::Down, false));
    assert_eq!(host.toolbar_state.scroll, 0);
}

#[test]
fn explicit_main_panel_characterization_fails_closed_without_native_authority() {
    let cases = [
        (
            Panel::None,
            MenuFocus::Games,
            Key::Left,
            0,
            None,
            Some(MenuEffect::MoveGame(-1)),
        ),
        (
            Panel::None,
            MenuFocus::Toolbar,
            Key::Confirm,
            0,
            None,
            Some(MenuEffect::OpenToolbar(0)),
        ),
        (
            Panel::Resume,
            MenuFocus::Games,
            Key::Back,
            0,
            None,
            Some(MenuEffect::DelegateSuspend),
        ),
        (
            Panel::Toolbar,
            MenuFocus::Games,
            Key::Other,
            0,
            None,
            Some(MenuEffect::DelegateToolbar),
        ),
        (
            Panel::ConfirmDelete,
            MenuFocus::Games,
            Key::Confirm,
            0,
            Some(0),
            Some(MenuEffect::CancelSuspend),
        ),
        (
            Panel::ConfirmDelete,
            MenuFocus::Games,
            Key::Confirm,
            1,
            Some(0),
            Some(MenuEffect::ConfirmDelete),
        ),
        (
            Panel::ConfirmOverwrite,
            MenuFocus::Games,
            Key::Confirm,
            usize::MAX,
            Some(0),
            None,
        ),
        (
            Panel::ConfirmOther,
            MenuFocus::Games,
            Key::Back,
            0,
            None,
            Some(MenuEffect::CancelConfirmationAndOpenSuspend),
        ),
        (
            Panel::Other,
            MenuFocus::Games,
            Key::Confirm,
            0,
            Some(1),
            Some(MenuEffect::ActivateFocused(0)),
        ),
        (
            Panel::Other,
            MenuFocus::Games,
            Key::Confirm,
            1,
            Some(1),
            None,
        ),
    ];
    for (panel, focus, key, initial_focus, action_count, expected) in cases {
        let mut context = MenuContext {
            playing: false,
            owned_import_notice: false,
            panel,
            action_count: action_count.unwrap_or(0),
        };
        if panel == Panel::Other && key == Key::Back {
            context.owned_import_notice = false;
        }
        let state = menu::State {
            menu_focus: focus,
            focus: initial_focus,
            toolbar_index: 0,
        };
        let plan = menu::transition(state, context, event(key, false));
        assert_eq!(
            plan.effect, expected,
            "{panel:?}, {focus:?}, {key:?}, focus={initial_focus}"
        );
    }

    let mut invalid = FakeHost::new(Panel::ConfirmDelete);
    invalid.menu_state.focus = usize::MAX;
    invalid.tick(event(Key::Confirm, false));
    assert!(
        !invalid
            .intents
            .contains(&Intent::Menu(MenuEffect::ConfirmDelete))
    );
    assert!(
        !invalid
            .intents
            .contains(&Intent::Menu(MenuEffect::ConfirmSave))
    );
    assert_eq!(invalid.resource_operations, 0);

    for (panel, focus, effect) in [
        (Panel::ConfirmOverwrite, 0, MenuEffect::CancelSuspend),
        (Panel::ConfirmOverwrite, 1, MenuEffect::ConfirmSave),
        (Panel::ConfirmDelete, 0, MenuEffect::CancelSuspend),
        (Panel::ConfirmDelete, 1, MenuEffect::ConfirmDelete),
    ] {
        let mut host = FakeHost::new(panel);
        host.menu_state.focus = focus;
        host.tick(event(Key::Confirm, false));
        assert_eq!(host.intents, [Intent::Menu(effect)]);
        assert_eq!(host.resource_operations, 0);
    }
}

#[test]
fn suspend_delegate_composes_key_translation_with_production_transition() {
    let mut host = FakeHost::new(Panel::Resume);
    host.toolbar_context.nes = false;
    host.has_pending = true;
    host.tick(event(Key::Confirm, false));
    assert_eq!(host.intents, [Intent::Suspend(SuspendEffect::RequestSave)]);

    host.intents.clear();
    host.has_pending = false;
    host.tick(event(Key::Right, true));
    assert_eq!(host.suspend_state.slot, 1);
    assert_eq!(host.intents, [Intent::Suspend(SuspendEffect::SaveSettings)]);

    host.intents.clear();
    host.tick(event(Key::Down, false));
    host.toolbar_context.nes = true;
    host.tick(event(Key::Confirm, false));
    assert!(host.suspend_state.controls);
    assert_eq!(
        host.intents.last(),
        Some(&Intent::Suspend(SuspendEffect::ActivateChooseSuspend))
    );

    host.intents.clear();
    host.tick(event(Key::Delete, true));
    host.tick(event(Key::SaveSuspend, true));
    assert!(host.intents.is_empty());
    host.tick(event(Key::Back, false));
    assert_eq!(host.intents, [Intent::Suspend(SuspendEffect::Dismiss)]);
    assert_eq!(host.menu_context.panel, Panel::None);

    assert_eq!(menu_key(Button::South, true), Some(Key::Confirm));
    let translated = suspend_input(event(Key::Delete, false), false, true, true);
    assert_eq!(translated, SuspendInput::Delete);
}

#[test]
fn unrelated_keys_clear_hover_without_touching_gameplay_state() {
    let mut host = FakeHost::new(Panel::None);
    host.hover = true;
    let gameplay_mask = [0x55_u16, 0xAA_u16];
    host.tick(event(Key::Other, false));
    assert!(!host.hover);
    assert_eq!(gameplay_mask, [0x55, 0xAA]);
    assert!(host.intents.is_empty());
}
