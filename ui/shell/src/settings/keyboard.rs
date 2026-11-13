use super::widgets::*;
use gtk::prelude::*;
use std::rc::Rc;

pub(crate) fn keyboard_page() -> gtk::Box {
    let page = page_shell();

    // 1. System & Window Shortcuts
    section(&page, "System & Window Shortcuts");
    let (_shortcuts_card, shortcuts_body) = card(&page);

    let shortcuts = [
        ("Application Launcher", "⌘ Space", "tiaki-shell --launcher"),
        ("Terminal Emulator", "⌘ Return", "foot"),
        ("Application Switcher", "⌘ Tab", "tiaki-shell --switcher"),
        ("Window Overview (Exposé)", "⌘ Up / ⌘ M", "tiaki-shell --overview"),
        ("Lock Screen", "⌃ ⌘ Q / ⌘ L", "swaylock -f"),
        ("Close Active Window", "⌘ W / ⌘ Q", "swaymsg kill"),
        ("Toggle Fullscreen", "⌘ F", "swaymsg fullscreen toggle"),
        ("Hide to Scratchpad", "⌘ H", "swaymsg move scratchpad"),
        ("Toggle Scratchpad", "⌘ ⇧ H", "swaymsg scratchpad show"),
        ("Switch Workspaces (1–5)", "⌘ 1 – ⌘ 5", "swaymsg workspace number N"),
        ("Open Settings", "⌘ ,", "tiaki-shell --settings"),
        ("Log Out / Exit Session", "⌘ ⇧ E", "swaymsg exit"),
    ];

    for (action_name, key_combo, cli_cmd) in shortcuts {
        let (_, right) = row(
            &shortcuts_body,
            Some("fluent:keyboard"),
            action_name,
            &format!("CLI equivalent: {}", cli_cmd),
        );
        let key_badge = pill_button(key_combo);
        key_badge.set_sensitive(false);
        right.pack_start(&key_badge, false, false, 0);
    }

    // 2. Input Sources & Layout
    section(&page, "Input Sources");
    let (_input_card, input_body) = card(&page);

    let (_, layout_right) = row(
        &input_body,
        Some("fluent:globe"),
        "Keyboard Layout",
        "Active xkb key mapping for physical keyboard",
    );
    let layout_drop = dropdown(
        "English (US)",
        vec![
            ("English (US)".to_string(), Rc::new(|| {})),
            ("English (UK)".to_string(), Rc::new(|| {})),
            ("German (QWERTZ)".to_string(), Rc::new(|| {})),
            ("French (AZERTY)".to_string(), Rc::new(|| {})),
            ("Spanish (Modern)".to_string(), Rc::new(|| {})),
        ],
    );
    layout_right.pack_start(&layout_drop, false, false, 0);

    let (_, compose_right) = row(
        &input_body,
        None,
        "Compose Key",
        "Key for entering special unicode characters and symbols",
    );
    let compose_drop = dropdown(
        "Right Alt",
        vec![
            ("Disabled".to_string(), Rc::new(|| {})),
            ("Right Alt".to_string(), Rc::new(|| {})),
            ("Caps Lock".to_string(), Rc::new(|| {})),
        ],
    );
    compose_right.pack_start(&compose_drop, false, false, 0);

    // 3. Typing & Repeat Behavior
    section(&page, "Typing & Repeat");
    let (_repeat_card, repeat_body) = card(&page);

    let (_, repeat_right) = row(
        &repeat_body,
        None,
        "Key Repeat",
        "Hold down a key to repeat it repeatedly",
    );
    let repeat_switch = toggle_switch(true, |_| {});
    repeat_right.pack_start(&repeat_switch, false, false, 0);

    let (_, delay_right) = row(
        &repeat_body,
        None,
        "Delay Until Repeat",
        "How long to hold before repeat triggers (default: 300ms)",
    );
    slider(&delay_right, 70.0, |_| {});

    let (_, speed_right) = row(
        &repeat_body,
        None,
        "Key Repeat Speed",
        "Rate of repeated key events (default: 50 cps)",
    );
    slider(&speed_right, 85.0, |_| {});

    let (_, cli_right) = row(
        &repeat_body,
        Some("fluent:keyboard"),
        "Command Line Input Config",
        "Query or configure inputs dynamically via Sway IPC",
    );
    value_text(&cli_right, "swaymsg input '*' repeat_rate 50");

    page
}
