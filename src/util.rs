use crossterm::event::KeyCode;
use std::time::Duration;

pub fn keycode_display(code: &KeyCode) -> String {
    match code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Up => "↑".into(),
        KeyCode::Down => "↓".into(),
        _ => "?".into(),
    }
}

pub fn format_duration(duration: Duration) -> String {
    let ms = duration.as_millis();

    if ms < 1000 {
        return format!("{}ms", ms);
    }

    let seconds = duration.as_secs();

    if seconds < 60 {
        return format!("{}s", seconds);
    }

    let minutes = seconds / 60;

    if minutes < 60 {
        return format!("{}m", minutes);
    }

    let hours = minutes / 60;

    if hours < 24 {
        return format!("{}h", hours);
    }

    let days = hours / 24;

    format!("{}d", days)
}
