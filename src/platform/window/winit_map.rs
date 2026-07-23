// ── winit helpers ─────────────────────────────────────────────────

pub(crate) fn map_mouse_button(btn: winit::event::MouseButton) -> crate::event::MouseButton {
    match btn {
        winit::event::MouseButton::Left => crate::event::MouseButton::Left,
        winit::event::MouseButton::Right => crate::event::MouseButton::Right,
        winit::event::MouseButton::Middle => crate::event::MouseButton::Middle,
        winit::event::MouseButton::Back => crate::event::MouseButton::Back,
        winit::event::MouseButton::Forward => crate::event::MouseButton::Forward,
        _ => crate::event::MouseButton::Other(0),
    }
}

pub(crate) fn map_touch_phase(phase: winit::event::TouchPhase) -> crate::event::GesturePhase {
    match phase {
        winit::event::TouchPhase::Started => crate::event::GesturePhase::Started,
        winit::event::TouchPhase::Moved => crate::event::GesturePhase::Moved,
        winit::event::TouchPhase::Ended => crate::event::GesturePhase::Ended,
        winit::event::TouchPhase::Cancelled => crate::event::GesturePhase::Cancelled,
    }
}

pub(crate) fn map_winit_key(logical: &winit::keyboard::Key) -> Option<crate::event::Key> {
    use winit::keyboard::{Key, NamedKey};
    Some(match logical {
        Key::Character(c) if c == "\r" => crate::event::Key::Enter,
        Key::Character(c) if c == "\t" => crate::event::Key::Tab,
        Key::Character(c) if c == "\u{8}" => crate::event::Key::Backspace,
        Key::Character(c) if c == "\u{1b}" => crate::event::Key::Escape,
        Key::Character(c) if c == " " => crate::event::Key::Space,
        Key::Character(c) => crate::event::Key::Character(c.to_string()),
        Key::Named(NamedKey::Enter) => crate::event::Key::Enter,
        Key::Named(NamedKey::Tab) => crate::event::Key::Tab,
        Key::Named(NamedKey::Backspace) => crate::event::Key::Backspace,
        Key::Named(NamedKey::Escape) => crate::event::Key::Escape,
        Key::Named(NamedKey::ArrowUp) => crate::event::Key::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => crate::event::Key::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => crate::event::Key::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => crate::event::Key::ArrowRight,
        _ => return None,
    })
}

pub(crate) fn map_winit_action_key(logical: &winit::keyboard::Key) -> crate::event::Key {
    use winit::keyboard::{Key, NamedKey};
    match logical {
        Key::Character(c) if c == "\t" => crate::event::Key::Tab,
        Key::Character(c) if c == " " => crate::event::Key::Space,
        Key::Character(c) if c == "\r" => crate::event::Key::Enter,
        Key::Character(c) if c == "\u{1b}" => crate::event::Key::Escape,
        Key::Named(NamedKey::Enter) => crate::event::Key::Enter,
        Key::Named(NamedKey::Tab) => crate::event::Key::Tab,
        Key::Named(NamedKey::Backspace) => crate::event::Key::Backspace,
        Key::Named(NamedKey::Delete) => crate::event::Key::Delete,
        Key::Named(NamedKey::Escape) => crate::event::Key::Escape,
        Key::Named(NamedKey::ArrowUp) => crate::event::Key::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => crate::event::Key::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => crate::event::Key::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => crate::event::Key::ArrowRight,
        Key::Named(NamedKey::Home) => crate::event::Key::Home,
        Key::Named(NamedKey::End) => crate::event::Key::End,
        _ => crate::event::Key::Character("?".into()),
    }
}
