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
        matches!(self, Self::Attack | Self::OffHandAttack)
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

#[derive(Resource)]
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

    fn binding_pressed(binding: Binding, keys: &ButtonInput<KeyCode>) -> bool {
        matches!(binding, Binding::Key(key) if keys.pressed(key))
    }

    fn binding_just_pressed(binding: Binding, keys: &ButtonInput<KeyCode>) -> bool {
        matches!(binding, Binding::Key(key) if keys.just_pressed(key))
    }

    pub(crate) fn pressed(&self, control: Control, keys: &ButtonInput<KeyCode>) -> bool {
        self.bindings(control)
            .any(|binding| Self::binding_pressed(binding, keys))
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
    fn assignment_rejects_reserved_keys_and_mouse_on_non_attack_controls() {
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
                Control::Interact,
                KeybindSlot::Primary,
                Binding::Mouse(MouseButton::Middle)
            ),
            Err("Press a keyboard key for this action.".into())
        );
        assert!(keybinds.get(Control::Jump) == Binding::Key(KeyCode::Space));
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
        assert!(keybinds.pressed(Control::Jump, &primary_keys));
        assert!(keybinds.just_pressed(Control::Jump, &primary_keys));

        let mut secondary_keys = ButtonInput::<KeyCode>::default();
        secondary_keys.press(KeyCode::KeyJ);
        assert!(keybinds.pressed(Control::Jump, &secondary_keys));
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
        assert!(!keybinds.pressed(Control::Jump, &secondary_keys));
        assert!(keybinds.pressed(Control::Jump, &primary_keys));
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
        assert!(Keybinds::default().pressed(Control::Forward, &keys));
        assert!(Keybinds::default().just_pressed(Control::Forward, &keys));
        assert!(!Keybinds::default().pressed(Control::Attack, &keys));
    }
}
