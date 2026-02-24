use crossterm::event::KeyCode;
use std::collections::HashMap;

#[derive(Eq, PartialEq, Hash, Clone)]
pub enum KeybindType {
    Up,
    Down,
    Restart,
    Start,
    Stop,
    Enter,
    Quit,
    Escape,
    Update,
}

#[derive(Clone, Eq, PartialEq)]
pub enum KeybindContext {
    Main,
    Logs,
}

#[derive(Clone)]
pub struct Keybind {
    pub t: KeybindType,
    pub name: String,
    pub context: KeybindContext,
}

impl Keybind {
    pub fn new<T: Into<String>>(t: KeybindType, name: T) -> Self {
        Self {
            t,
            name: name.into(),
            context: KeybindContext::Main,
        }
    }

    pub fn new_logs<T: Into<String>>(t: KeybindType, name: T) -> Self {
        Self {
            t,
            name: name.into(),
            context: KeybindContext::Logs,
        }
    }
}

pub fn get_keybinds() -> HashMap<KeyCode, Keybind> {
    HashMap::from([
        (KeyCode::Up, Keybind::new(KeybindType::Up, "Up")),
        (KeyCode::Down, Keybind::new(KeybindType::Down, "Down")),
        (
            KeyCode::Char('r'),
            Keybind::new(KeybindType::Restart, "Restart"),
        ),
        (
            KeyCode::Char('s'),
            Keybind::new(KeybindType::Start, "Start"),
        ),
        (KeyCode::Char('x'), Keybind::new(KeybindType::Stop, "Stop")),
        (
            KeyCode::Enter,
            Keybind::new(KeybindType::Enter, "View Logs"),
        ),
        (KeyCode::Char('u'), Keybind::new(KeybindType::Update, "Update")),
        (KeyCode::Char('q'), Keybind::new(KeybindType::Quit, "Quit")),
        (
            KeyCode::Esc,
            Keybind::new_logs(KeybindType::Escape, "Escape"),
        ),
    ])
}
