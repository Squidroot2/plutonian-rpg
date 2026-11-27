use bevy::prelude::*;
use strum::VariantArray;

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
pub(crate) enum Control {
    Forward,
    Backward,
    Left,
    Right,
    ToggleRun,
    Sprint,
    Jump,
    Dodge,
    Interact,
    Inventory,
    QuestLog,
    Drop,
    MenuUp,
    MenuDown,
    MenuLeft,
    MenuRight,
    Attack,
    OffHandAttack,
    Holster,
}

impl Control {
    const MENU: [Self; 5] = [
        Self::MenuUp,
        Self::MenuDown,
        Self::MenuLeft,
        Self::MenuRight,
        Self::Drop,
    ];
    const MENU_SHORTCUTS: [Self; 2] = [Self::Inventory, Self::QuestLog];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Forward => "Move forward",
            Self::Backward => "Move backward",
            Self::Left => "Strafe left",
            Self::Right => "Strafe right",
            Self::ToggleRun => "Toggle walk / run",
            Self::Sprint => "Sprint",
            Self::Jump => "Jump",
            Self::Dodge => "Dodge",
            Self::Interact => "Interact",
            Self::Inventory => "Inventory",
            Self::QuestLog => "Quest Log",
            Self::Drop => "Drop item",
            Self::MenuUp => "Menu Up",
            Self::MenuDown => "Menu Down",
            Self::MenuLeft => "Menu Left",
            Self::MenuRight => "Menu Right",
            Self::Attack => "Main-hand attack",
            Self::OffHandAttack => "Off-hand attack",
            Self::Holster => "Holster / Draw Weapons",
        }
    }

    pub(crate) fn accepts_mouse(self) -> bool {
        self.keybind_category() == KeybindCategory::Gameplay
    }

    pub(crate) fn keybind_category(self) -> KeybindCategory {
        if Self::MENU_SHORTCUTS.contains(&self) {
            KeybindCategory::MenuShortcuts
        } else if Self::MENU.contains(&self) {
            KeybindCategory::Menu
        } else {
            KeybindCategory::Gameplay
        }
    }

    fn conflicts_with(self, other: Self) -> bool {
        let left = self.conflict_group();
        let right = other.conflict_group();
        left == ConflictGroup::Exclusive || right == ConflictGroup::Exclusive || left == right
    }

    fn conflict_group(self) -> ConflictGroup {
        if Self::MENU.contains(&self) {
            ConflictGroup::Menu
        } else if Self::MENU_SHORTCUTS.contains(&self) {
            ConflictGroup::Exclusive
        } else {
            ConflictGroup::Gameplay
        }
    }
}

impl Control {
    /// Stable snake-case key for the `[keybinds]` table in `config.toml`.
    /// This is the file format, distinct from [`Control::label`] presentation text.
    pub(crate) fn config_key(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Backward => "backward",
            Self::Left => "left",
            Self::Right => "right",
            Self::ToggleRun => "toggle_run",
            Self::Sprint => "sprint",
            Self::Jump => "jump",
            Self::Dodge => "dodge",
            Self::Interact => "interact",
            Self::Inventory => "inventory",
            Self::QuestLog => "quest_log",
            Self::Drop => "drop",
            Self::MenuUp => "menu_up",
            Self::MenuDown => "menu_down",
            Self::MenuLeft => "menu_left",
            Self::MenuRight => "menu_right",
            Self::Attack => "attack",
            Self::OffHandAttack => "off_hand_attack",
            Self::Holster => "holster",
        }
    }

    pub(crate) fn from_config_key(name: &str) -> Option<Self> {
        match name {
            "forward" => Some(Self::Forward),
            "backward" => Some(Self::Backward),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "toggle_run" => Some(Self::ToggleRun),
            "sprint" => Some(Self::Sprint),
            "jump" => Some(Self::Jump),
            "dodge" => Some(Self::Dodge),
            "interact" => Some(Self::Interact),
            "inventory" => Some(Self::Inventory),
            "quest_log" => Some(Self::QuestLog),
            "drop" => Some(Self::Drop),
            "menu_up" => Some(Self::MenuUp),
            "menu_down" => Some(Self::MenuDown),
            "menu_left" => Some(Self::MenuLeft),
            "menu_right" => Some(Self::MenuRight),
            "attack" => Some(Self::Attack),
            "off_hand_attack" => Some(Self::OffHandAttack),
            "holster" => Some(Self::Holster),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeybindCategory {
    Gameplay,
    Menu,
    MenuShortcuts,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConflictGroup {
    Gameplay,
    Menu,
    /// Some keys are active in both gameplay and menu contexts, so its binding
    /// must remain unique across both groups.
    Exclusive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeybindSlot {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Binding {
    Key(KeyCode),
    Mouse(MouseButton),
}

impl Binding {
    pub(crate) fn just_pressed(
        self,
        keys: &ButtonInput<KeyCode>,
        mouse: &ButtonInput<MouseButton>,
    ) -> bool {
        match self {
            Self::Key(key) => keys.just_pressed(key),
            Self::Mouse(button) => mouse.just_pressed(button),
        }
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Key(key) => match key {
                KeyCode::ControlLeft => "Left Ctrl".into(),
                KeyCode::ControlRight => "Right Ctrl".into(),
                KeyCode::ShiftLeft => "Left Shift".into(),
                KeyCode::ShiftRight => "Right Shift".into(),
                KeyCode::AltLeft => "Left Alt".into(),
                KeyCode::AltRight => "Right Alt".into(),
                KeyCode::CapsLock => "Caps Lock".into(),
                _ => {
                    let name = format!("{key:?}");
                    name.strip_prefix("Key")
                        .or_else(|| name.strip_prefix("Digit"))
                        .filter(|suffix| suffix.len() == 1)
                        .unwrap_or(&name)
                        .into()
                }
            },
            Self::Mouse(MouseButton::Left) => "Left click".into(),
            Self::Mouse(MouseButton::Right) => "Right click".into(),
            Self::Mouse(MouseButton::Middle) => "Middle click".into(),
            Self::Mouse(button) => format!("Mouse {button:?}"),
        }
    }
}

const KEY_CONFIG_NAMES: &[(&str, KeyCode)] = &[
    ("Backquote", KeyCode::Backquote),
    ("Backslash", KeyCode::Backslash),
    ("BracketLeft", KeyCode::BracketLeft),
    ("BracketRight", KeyCode::BracketRight),
    ("Comma", KeyCode::Comma),
    ("Digit0", KeyCode::Digit0),
    ("Digit1", KeyCode::Digit1),
    ("Digit2", KeyCode::Digit2),
    ("Digit3", KeyCode::Digit3),
    ("Digit4", KeyCode::Digit4),
    ("Digit5", KeyCode::Digit5),
    ("Digit6", KeyCode::Digit6),
    ("Digit7", KeyCode::Digit7),
    ("Digit8", KeyCode::Digit8),
    ("Digit9", KeyCode::Digit9),
    ("Equal", KeyCode::Equal),
    ("IntlBackslash", KeyCode::IntlBackslash),
    ("IntlRo", KeyCode::IntlRo),
    ("IntlYen", KeyCode::IntlYen),
    ("KeyA", KeyCode::KeyA),
    ("KeyB", KeyCode::KeyB),
    ("KeyC", KeyCode::KeyC),
    ("KeyD", KeyCode::KeyD),
    ("KeyE", KeyCode::KeyE),
    ("KeyF", KeyCode::KeyF),
    ("KeyG", KeyCode::KeyG),
    ("KeyH", KeyCode::KeyH),
    ("KeyI", KeyCode::KeyI),
    ("KeyJ", KeyCode::KeyJ),
    ("KeyK", KeyCode::KeyK),
    ("KeyL", KeyCode::KeyL),
    ("KeyM", KeyCode::KeyM),
    ("KeyN", KeyCode::KeyN),
    ("KeyO", KeyCode::KeyO),
    ("KeyP", KeyCode::KeyP),
    ("KeyQ", KeyCode::KeyQ),
    ("KeyR", KeyCode::KeyR),
    ("KeyS", KeyCode::KeyS),
    ("KeyT", KeyCode::KeyT),
    ("KeyU", KeyCode::KeyU),
    ("KeyV", KeyCode::KeyV),
    ("KeyW", KeyCode::KeyW),
    ("KeyX", KeyCode::KeyX),
    ("KeyY", KeyCode::KeyY),
    ("KeyZ", KeyCode::KeyZ),
    ("Minus", KeyCode::Minus),
    ("Period", KeyCode::Period),
    ("Quote", KeyCode::Quote),
    ("Semicolon", KeyCode::Semicolon),
    ("Slash", KeyCode::Slash),
    ("AltLeft", KeyCode::AltLeft),
    ("AltRight", KeyCode::AltRight),
    ("Backspace", KeyCode::Backspace),
    ("CapsLock", KeyCode::CapsLock),
    ("ContextMenu", KeyCode::ContextMenu),
    ("ControlLeft", KeyCode::ControlLeft),
    ("ControlRight", KeyCode::ControlRight),
    ("Enter", KeyCode::Enter),
    ("SuperLeft", KeyCode::SuperLeft),
    ("SuperRight", KeyCode::SuperRight),
    ("ShiftLeft", KeyCode::ShiftLeft),
    ("ShiftRight", KeyCode::ShiftRight),
    ("Space", KeyCode::Space),
    ("Tab", KeyCode::Tab),
    ("Convert", KeyCode::Convert),
    ("KanaMode", KeyCode::KanaMode),
    ("Lang1", KeyCode::Lang1),
    ("Lang2", KeyCode::Lang2),
    ("Lang3", KeyCode::Lang3),
    ("Lang4", KeyCode::Lang4),
    ("Lang5", KeyCode::Lang5),
    ("NonConvert", KeyCode::NonConvert),
    ("Delete", KeyCode::Delete),
    ("End", KeyCode::End),
    ("Help", KeyCode::Help),
    ("Home", KeyCode::Home),
    ("Insert", KeyCode::Insert),
    ("PageDown", KeyCode::PageDown),
    ("PageUp", KeyCode::PageUp),
    ("ArrowDown", KeyCode::ArrowDown),
    ("ArrowLeft", KeyCode::ArrowLeft),
    ("ArrowRight", KeyCode::ArrowRight),
    ("ArrowUp", KeyCode::ArrowUp),
    ("NumLock", KeyCode::NumLock),
    ("Numpad0", KeyCode::Numpad0),
    ("Numpad1", KeyCode::Numpad1),
    ("Numpad2", KeyCode::Numpad2),
    ("Numpad3", KeyCode::Numpad3),
    ("Numpad4", KeyCode::Numpad4),
    ("Numpad5", KeyCode::Numpad5),
    ("Numpad6", KeyCode::Numpad6),
    ("Numpad7", KeyCode::Numpad7),
    ("Numpad8", KeyCode::Numpad8),
    ("Numpad9", KeyCode::Numpad9),
    ("NumpadAdd", KeyCode::NumpadAdd),
    ("NumpadBackspace", KeyCode::NumpadBackspace),
    ("NumpadClear", KeyCode::NumpadClear),
    ("NumpadClearEntry", KeyCode::NumpadClearEntry),
    ("NumpadComma", KeyCode::NumpadComma),
    ("NumpadDecimal", KeyCode::NumpadDecimal),
    ("NumpadDivide", KeyCode::NumpadDivide),
    ("NumpadEnter", KeyCode::NumpadEnter),
    ("NumpadEqual", KeyCode::NumpadEqual),
    ("NumpadHash", KeyCode::NumpadHash),
    ("NumpadMemoryAdd", KeyCode::NumpadMemoryAdd),
    ("NumpadMemoryClear", KeyCode::NumpadMemoryClear),
    ("NumpadMemoryRecall", KeyCode::NumpadMemoryRecall),
    ("NumpadMemoryStore", KeyCode::NumpadMemoryStore),
    ("NumpadMemorySubtract", KeyCode::NumpadMemorySubtract),
    ("NumpadMultiply", KeyCode::NumpadMultiply),
    ("NumpadParenLeft", KeyCode::NumpadParenLeft),
    ("NumpadParenRight", KeyCode::NumpadParenRight),
    ("NumpadStar", KeyCode::NumpadStar),
    ("NumpadSubtract", KeyCode::NumpadSubtract),
    ("Escape", KeyCode::Escape),
    ("Fn", KeyCode::Fn),
    ("FnLock", KeyCode::FnLock),
    ("PrintScreen", KeyCode::PrintScreen),
    ("ScrollLock", KeyCode::ScrollLock),
    ("Pause", KeyCode::Pause),
    ("BrowserBack", KeyCode::BrowserBack),
    ("BrowserFavorites", KeyCode::BrowserFavorites),
    ("BrowserForward", KeyCode::BrowserForward),
    ("BrowserHome", KeyCode::BrowserHome),
    ("BrowserRefresh", KeyCode::BrowserRefresh),
    ("BrowserSearch", KeyCode::BrowserSearch),
    ("BrowserStop", KeyCode::BrowserStop),
    ("Eject", KeyCode::Eject),
    ("LaunchApp1", KeyCode::LaunchApp1),
    ("LaunchApp2", KeyCode::LaunchApp2),
    ("LaunchMail", KeyCode::LaunchMail),
    ("MediaPlayPause", KeyCode::MediaPlayPause),
    ("MediaSelect", KeyCode::MediaSelect),
    ("MediaStop", KeyCode::MediaStop),
    ("MediaTrackNext", KeyCode::MediaTrackNext),
    ("MediaTrackPrevious", KeyCode::MediaTrackPrevious),
    ("Power", KeyCode::Power),
    ("Sleep", KeyCode::Sleep),
    ("AudioVolumeDown", KeyCode::AudioVolumeDown),
    ("AudioVolumeMute", KeyCode::AudioVolumeMute),
    ("AudioVolumeUp", KeyCode::AudioVolumeUp),
    ("WakeUp", KeyCode::WakeUp),
    ("Meta", KeyCode::Meta),
    ("Hyper", KeyCode::Hyper),
    ("Turbo", KeyCode::Turbo),
    ("Abort", KeyCode::Abort),
    ("Resume", KeyCode::Resume),
    ("Suspend", KeyCode::Suspend),
    ("Again", KeyCode::Again),
    ("Copy", KeyCode::Copy),
    ("Cut", KeyCode::Cut),
    ("Find", KeyCode::Find),
    ("Open", KeyCode::Open),
    ("Paste", KeyCode::Paste),
    ("Props", KeyCode::Props),
    ("Select", KeyCode::Select),
    ("Undo", KeyCode::Undo),
    ("Hiragana", KeyCode::Hiragana),
    ("Katakana", KeyCode::Katakana),
    ("F1", KeyCode::F1),
    ("F2", KeyCode::F2),
    ("F3", KeyCode::F3),
    ("F4", KeyCode::F4),
    ("F5", KeyCode::F5),
    ("F6", KeyCode::F6),
    ("F7", KeyCode::F7),
    ("F8", KeyCode::F8),
    ("F9", KeyCode::F9),
    ("F10", KeyCode::F10),
    ("F11", KeyCode::F11),
    ("F12", KeyCode::F12),
    ("F13", KeyCode::F13),
    ("F14", KeyCode::F14),
    ("F15", KeyCode::F15),
    ("F16", KeyCode::F16),
    ("F17", KeyCode::F17),
    ("F18", KeyCode::F18),
    ("F19", KeyCode::F19),
    ("F20", KeyCode::F20),
    ("F21", KeyCode::F21),
    ("F22", KeyCode::F22),
    ("F23", KeyCode::F23),
    ("F24", KeyCode::F24),
    ("F25", KeyCode::F25),
    ("F26", KeyCode::F26),
    ("F27", KeyCode::F27),
    ("F28", KeyCode::F28),
    ("F29", KeyCode::F29),
    ("F30", KeyCode::F30),
    ("F31", KeyCode::F31),
    ("F32", KeyCode::F32),
    ("F33", KeyCode::F33),
    ("F34", KeyCode::F34),
    ("F35", KeyCode::F35),
];

impl Binding {
    /// Stable human-readable name for the config file.
    /// This is the file format, distinct from [`Binding::label`] presentation text.
    pub(crate) fn config_name(self) -> String {
        match self {
            Self::Key(KeyCode::Unidentified(_)) => "Unidentified".into(),
            Self::Key(key) => format!("{key:?}"),
            Self::Mouse(MouseButton::Left) => "MouseLeft".into(),
            Self::Mouse(MouseButton::Right) => "MouseRight".into(),
            Self::Mouse(MouseButton::Middle) => "MouseMiddle".into(),
            Self::Mouse(MouseButton::Back) => "MouseBack".into(),
            Self::Mouse(MouseButton::Forward) => "MouseForward".into(),
            Self::Mouse(MouseButton::Other(button)) => format!("MouseOther({button})"),
        }
    }

    pub(crate) fn from_config_name(name: &str) -> Option<Self> {
        match name {
            "MouseLeft" => Some(Self::Mouse(MouseButton::Left)),
            "MouseRight" => Some(Self::Mouse(MouseButton::Right)),
            "MouseMiddle" => Some(Self::Mouse(MouseButton::Middle)),
            "MouseBack" => Some(Self::Mouse(MouseButton::Back)),
            "MouseForward" => Some(Self::Mouse(MouseButton::Forward)),
            _ => parse_config_key(name)
                .map(Self::Key)
                .or_else(|| parse_config_mouse_other(name).map(Self::Mouse)),
        }
    }
}

/// Look up a keyboard key by its stable config name.
fn parse_config_key(name: &str) -> Option<KeyCode> {
    KEY_CONFIG_NAMES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, key)| *key)
}

/// Parse `MouseOther(N)` for auxiliary mouse buttons.
fn parse_config_mouse_other(name: &str) -> Option<MouseButton> {
    name.strip_prefix("MouseOther(")
        .and_then(|rest| rest.strip_suffix(')'))
        .and_then(|digits| digits.parse::<u16>().ok())
        .map(MouseButton::Other)
}

impl Keybinds {
    /// Set both slots directly from validated config values.
    /// Unlike [`Keybinds::assign`], this skips conflict and reservation checks:
    /// the config file stores the bindings as-is and loads them verbatim.
    pub(crate) fn apply_config(
        &mut self,
        control: Control,
        primary: Binding,
        secondary: Option<Binding>,
    ) {
        self.0[control as usize] = (primary, secondary);
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Keybinds([(Binding, Option<Binding>); Control::VARIANTS.len()]);

impl Default for Keybinds {
    fn default() -> Self {
        use KeyCode::*;
        let mut bindings = [(Binding::Key(KeyCode::Space), None); Control::VARIANTS.len()];
        let keys = [
            (Control::Forward, KeyW),
            (Control::Backward, KeyS),
            (Control::Left, KeyA),
            (Control::Right, KeyD),
            (Control::ToggleRun, CapsLock),
            (Control::Sprint, ShiftLeft),
            (Control::Jump, Space),
            (Control::Dodge, crate::player::DODGE_KEY),
            (Control::Interact, KeyE),
            (Control::Inventory, KeyI),
            (Control::QuestLog, KeyL),
            (Control::Drop, KeyQ),
            (Control::MenuUp, KeyW),
            (Control::MenuDown, KeyS),
            (Control::MenuLeft, KeyA),
            (Control::MenuRight, KeyD),
            (Control::Holster, KeyX),
        ];
        for (control, key) in keys {
            bindings[control as usize] = (Binding::Key(key), None);
        }
        bindings[Control::Attack as usize] = (Binding::Mouse(MouseButton::Left), None);
        bindings[Control::OffHandAttack as usize] = (Binding::Mouse(MouseButton::Right), None);
        bindings[Control::MenuUp as usize].1 = Some(Binding::Key(ArrowUp));
        bindings[Control::MenuDown as usize].1 = Some(Binding::Key(ArrowDown));
        bindings[Control::MenuLeft as usize].1 = Some(Binding::Key(ArrowLeft));
        bindings[Control::MenuRight as usize].1 = Some(Binding::Key(ArrowRight));
        Self(bindings)
    }
}

impl Keybinds {
    pub(crate) fn get(&self, control: Control) -> Binding {
        self.0[control as usize].0
    }

    pub(crate) fn get_secondary(&self, control: Control) -> Option<Binding> {
        self.0[control as usize].1
    }

    /// Every binding held by a control: its primary followed by its
    /// secondary when one is set.
    pub(crate) fn bindings(&self, control: Control) -> impl Iterator<Item = Binding> {
        let (primary, secondary) = self.0[control as usize];
        [Some(primary), secondary].into_iter().flatten()
    }

    fn binding_just_pressed(binding: Binding, keys: &ButtonInput<KeyCode>) -> bool {
        matches!(binding, Binding::Key(key) if keys.just_pressed(key))
    }

    pub(crate) fn just_pressed(&self, control: Control, keys: &ButtonInput<KeyCode>) -> bool {
        self.bindings(control)
            .any(|binding| Self::binding_just_pressed(binding, keys))
    }

    /// Either-slot press across keyboard and mouse, for controls such as
    /// the attacks that accept mouse buttons in either slot.
    pub(crate) fn just_pressed_any(
        &self,
        control: Control,
        keys: &ButtonInput<KeyCode>,
        mouse: &ButtonInput<MouseButton>,
    ) -> bool {
        self.bindings(control)
            .any(|binding| binding.just_pressed(keys, mouse))
    }

    pub(crate) fn pressed_any(
        &self,
        control: Control,
        keys: &ButtonInput<KeyCode>,
        mouse: &ButtonInput<MouseButton>,
    ) -> bool {
        self.bindings(control).any(|binding| match binding {
            Binding::Key(key) => keys.pressed(key),
            Binding::Mouse(button) => mouse.pressed(button),
        })
    }

    pub(crate) fn clear_secondary(&mut self, control: Control) {
        self.0[control as usize].1 = None;
    }

    pub(crate) fn assign(
        &mut self,
        control: Control,
        slot: KeybindSlot,
        binding: Binding,
    ) -> Result<(), String> {
        match binding {
            Binding::Key(KeyCode::Unidentified(_)) => {
                return Err("That key could not be identified.".into());
            }
            Binding::Key(
                KeyCode::Escape
                | KeyCode::Tab
                | KeyCode::Enter
                | KeyCode::NumpadEnter
                | KeyCode::Delete,
            ) => {
                return Err("That key is reserved.".into());
            }
            Binding::Mouse(_) if !control.accepts_mouse() => {
                return Err("Press a keyboard key for this action.".into());
            }
            _ => {}
        }
        if let Some(other) = Control::VARIANTS.iter().find(|&&other| {
            other != control
                && control.conflicts_with(other)
                && self.bindings(other).any(|held| held == binding)
        }) {
            return Err(format!("Already assigned to {}.", other.label()));
        }
        let (primary, secondary) = self.0[control as usize];
        let other_slot = match slot {
            KeybindSlot::Primary => secondary,
            KeybindSlot::Secondary => Some(primary),
        };
        if other_slot == Some(binding) {
            return Err(format!("Already assigned to {}.", control.label()));
        }
        match slot {
            KeybindSlot::Primary => self.0[control as usize].0 = binding,
            KeybindSlot::Secondary => self.0[control as usize].1 = Some(binding),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_rejects_reserved_keys_and_mouse_on_menu_controls() {
        let mut keybinds = Keybinds::default();

        assert_eq!(
            keybinds.assign(
                Control::Jump,
                KeybindSlot::Primary,
                Binding::Key(KeyCode::Escape)
            ),
            Err("That key is reserved.".into())
        );
        assert_eq!(
            keybinds.assign(
                Control::Jump,
                KeybindSlot::Secondary,
                Binding::Key(KeyCode::Delete)
            ),
            Err("That key is reserved.".into())
        );
        assert_eq!(
            keybinds.assign(
                Control::MenuUp,
                KeybindSlot::Primary,
                Binding::Mouse(MouseButton::Middle)
            ),
            Err("Press a keyboard key for this action.".into())
        );
        assert!(keybinds.get(Control::Jump) == Binding::Key(KeyCode::Space));
    }

    #[test]
    fn mouse_assignment_follows_categories_in_both_slots() {
        for &control in Control::VARIANTS {
            for slot in [KeybindSlot::Primary, KeybindSlot::Secondary] {
                let mut bindings = Keybinds::default();
                let result = bindings.assign(control, slot, Binding::Mouse(MouseButton::Middle));
                if control.keybind_category() == KeybindCategory::Gameplay {
                    assert_eq!(result, Ok(()), "{control:?} {slot:?}");
                } else {
                    assert_eq!(result, Err("Press a keyboard key for this action.".into()));
                }
            }
        }
        assert_eq!(
            Keybinds::default().assign(
                Control::Forward,
                KeybindSlot::Secondary,
                Binding::Mouse(MouseButton::Left),
            ),
            Err("Already assigned to Main-hand attack.".into())
        );
    }

    #[test]
    fn mouse_and_keyboard_bindings_work_across_frames() {
        let mut bindings = Keybinds::default();
        bindings
            .assign(
                Control::Forward,
                KeybindSlot::Secondary,
                Binding::Mouse(MouseButton::Middle),
            )
            .expect("middle mouse is available for forward");
        let mut mouse = ButtonInput::<MouseButton>::default();
        let mut keys = ButtonInput::<KeyCode>::default();
        mouse.press(MouseButton::Middle);
        assert!(bindings.just_pressed_any(Control::Forward, &keys, &mouse));
        mouse.clear();
        assert!(bindings.pressed_any(Control::Forward, &keys, &mouse));
        assert!(!bindings.just_pressed_any(Control::Forward, &keys, &mouse));
        mouse.release(MouseButton::Middle);
        keys.press(KeyCode::KeyW);
        assert!(bindings.pressed_any(Control::Forward, &keys, &mouse));
    }

    #[test]
    fn assignment_checks_conflicts_within_and_across_contexts() {
        let mut keybinds = Keybinds::default();

        assert_eq!(
            keybinds.assign(
                Control::Forward,
                KeybindSlot::Primary,
                Binding::Key(KeyCode::KeyW)
            ),
            Ok(())
        );
        assert_eq!(
            keybinds.assign(
                Control::MenuDown,
                KeybindSlot::Primary,
                Binding::Key(KeyCode::KeyW)
            ),
            Err("Already assigned to Menu Up.".into())
        );
        assert_eq!(
            keybinds.assign(
                Control::Sprint,
                KeybindSlot::Primary,
                Binding::Key(KeyCode::KeyW)
            ),
            Err("Already assigned to Move forward.".into())
        );
        assert_eq!(
            keybinds.assign(
                Control::QuestLog,
                KeybindSlot::Primary,
                Binding::Key(KeyCode::KeyI)
            ),
            Err("Already assigned to Inventory.".into())
        );

        assert_eq!(
            keybinds.assign(
                Control::Forward,
                KeybindSlot::Primary,
                Binding::Key(KeyCode::KeyZ)
            ),
            Ok(())
        );
        assert_eq!(
            keybinds.assign(
                Control::Sprint,
                KeybindSlot::Primary,
                Binding::Key(KeyCode::KeyW)
            ),
            Ok(())
        );
    }

    #[test]
    fn either_slot_reports_pressed_and_conflicts_across_slots() {
        let mut keybinds = Keybinds::default();
        keybinds
            .assign(
                Control::Jump,
                KeybindSlot::Secondary,
                Binding::Key(KeyCode::KeyJ),
            )
            .expect("an unused key must bind as the jump secondary");

        let mut primary_keys = ButtonInput::<KeyCode>::default();
        primary_keys.press(KeyCode::Space);
        let mouse = ButtonInput::default();
        assert!(keybinds.pressed_any(Control::Jump, &primary_keys, &mouse));
        assert!(keybinds.just_pressed(Control::Jump, &primary_keys));

        let mut secondary_keys = ButtonInput::<KeyCode>::default();
        secondary_keys.press(KeyCode::KeyJ);
        assert!(keybinds.pressed_any(Control::Jump, &secondary_keys, &mouse));
        assert!(keybinds.just_pressed(Control::Jump, &secondary_keys));

        assert_eq!(
            keybinds.assign(
                Control::Sprint,
                KeybindSlot::Primary,
                Binding::Key(KeyCode::KeyJ)
            ),
            Err("Already assigned to Jump.".into())
        );
        assert_eq!(
            keybinds.assign(
                Control::Jump,
                KeybindSlot::Primary,
                Binding::Key(KeyCode::KeyJ)
            ),
            Err("Already assigned to Jump.".into())
        );

        keybinds.clear_secondary(Control::Jump);
        assert_eq!(keybinds.get_secondary(Control::Jump), None);
        assert!(!keybinds.pressed_any(Control::Jump, &secondary_keys, &mouse));
        assert!(keybinds.pressed_any(Control::Jump, &primary_keys, &mouse));
    }

    #[test]
    fn menu_directions_default_to_arrows_in_the_secondary_slot() {
        let keybinds = Keybinds::default();
        assert_eq!(
            keybinds.get_secondary(Control::MenuUp),
            Some(Binding::Key(KeyCode::ArrowUp))
        );
        assert_eq!(
            keybinds.get_secondary(Control::MenuDown),
            Some(Binding::Key(KeyCode::ArrowDown))
        );
        assert_eq!(
            keybinds.get_secondary(Control::MenuLeft),
            Some(Binding::Key(KeyCode::ArrowLeft))
        );
        assert_eq!(
            keybinds.get_secondary(Control::MenuRight),
            Some(Binding::Key(KeyCode::ArrowRight))
        );
        assert_eq!(keybinds.get_secondary(Control::Forward), None);
        assert_eq!(keybinds.get_secondary(Control::Backward), None);
        assert_eq!(keybinds.get_secondary(Control::Left), None);
        assert_eq!(keybinds.get_secondary(Control::Right), None);
    }

    #[test]
    fn bindings_report_just_pressed_for_the_matching_input_device() {
        let mut keys = ButtonInput::<KeyCode>::default();
        let mut mouse = ButtonInput::<MouseButton>::default();
        keys.press(KeyCode::KeyW);
        mouse.press(MouseButton::Left);

        assert!(Binding::Key(KeyCode::KeyW).just_pressed(&keys, &mouse));
        assert!(Binding::Mouse(MouseButton::Left).just_pressed(&keys, &mouse));
        assert!(!Binding::Key(KeyCode::KeyS).just_pressed(&keys, &mouse));
        assert!(!Binding::Mouse(MouseButton::Right).just_pressed(&keys, &mouse));
        assert!(Keybinds::default().pressed_any(Control::Forward, &keys, &mouse));
        assert!(Keybinds::default().just_pressed(Control::Forward, &keys));
        assert!(Keybinds::default().pressed_any(Control::Attack, &keys, &mouse));
    }
}
