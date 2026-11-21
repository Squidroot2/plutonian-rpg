use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Control {
    Forward,
    Backward,
    Left,
    Right,
    Sprint,
    Jump,
    Dodge,
    Interact,
    Inventory,
    Drop,
    Attack,
}

impl Control {
    pub(crate) const ALL: [Self; 11] = [
        Self::Forward,
        Self::Backward,
        Self::Left,
        Self::Right,
        Self::Sprint,
        Self::Jump,
        Self::Dodge,
        Self::Interact,
        Self::Inventory,
        Self::Drop,
        Self::Attack,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Forward => "Move forward",
            Self::Backward => "Move backward",
            Self::Left => "Strafe left",
            Self::Right => "Strafe right",
            Self::Sprint => "Sprint",
            Self::Jump => "Jump",
            Self::Dodge => "Dodge",
            Self::Interact => "Interact",
            Self::Inventory => "Inventory",
            Self::Drop => "Drop item",
            Self::Attack => "Attack",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Binding {
    Key(KeyCode),
    Mouse(MouseButton),
}

impl Binding {
    pub(crate) fn label(self) -> String {
        match self {
            Self::Key(key) => match key {
                KeyCode::ControlLeft => "Left Ctrl".into(),
                KeyCode::ControlRight => "Right Ctrl".into(),
                KeyCode::ShiftLeft => "Left Shift".into(),
                KeyCode::ShiftRight => "Right Shift".into(),
                KeyCode::AltLeft => "Left Alt".into(),
                KeyCode::AltRight => "Right Alt".into(),
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
pub(crate) struct Keybinds([Binding; Control::ALL.len()]);

impl Default for Keybinds {
    fn default() -> Self {
        use KeyCode::*;
        let mut bindings = [Binding::Mouse(MouseButton::Left); Control::ALL.len()];
        let keys = [
            (Control::Forward, KeyW),
            (Control::Backward, KeyS),
            (Control::Left, KeyA),
            (Control::Right, KeyD),
            (Control::Sprint, ShiftLeft),
            (Control::Jump, Space),
            (Control::Dodge, crate::player::DODGE_KEY),
            (Control::Interact, KeyE),
            (Control::Inventory, KeyI),
            (Control::Drop, KeyQ),
        ];
        for (control, key) in keys {
            bindings[control as usize] = Binding::Key(key);
        }
        Self(bindings)
    }
}

impl Keybinds {
    pub(crate) fn get(&self, control: Control) -> Binding {
        self.0[control as usize]
    }

    pub(crate) fn pressed(&self, control: Control, keys: &ButtonInput<KeyCode>) -> bool {
        matches!(self.get(control), Binding::Key(key) if keys.pressed(key))
    }

    pub(crate) fn just_pressed(&self, control: Control, keys: &ButtonInput<KeyCode>) -> bool {
        matches!(self.get(control), Binding::Key(key) if keys.just_pressed(key))
    }

    pub(crate) fn assign(&mut self, control: Control, binding: Binding) -> Result<(), String> {
        match binding {
            Binding::Key(KeyCode::Unidentified(_)) => {
                return Err("That key could not be identified.".into());
            }
            Binding::Key(
                KeyCode::Escape | KeyCode::Tab | KeyCode::Enter | KeyCode::NumpadEnter,
            ) => {
                return Err("That key is reserved.".into());
            }
            Binding::Mouse(_) if control != Control::Attack => {
                return Err("Press a keyboard key for this action.".into());
            }
            _ => {}
        }
        if let Some(other) = Control::ALL
            .into_iter()
            .find(|&other| other != control && self.get(other) == binding)
        {
            return Err(format!("Already assigned to {}.", other.label()));
        }
        self.0[control as usize] = binding;
        Ok(())
    }
}
