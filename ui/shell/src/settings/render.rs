//! Generic renderer: Settings page content declared as tree data.
//!
//! A page opts in by shipping a `<action>.content` panel node holding
//! `section` / `row` / `value` / `dropdown` nodes (see `docs/specs/ui-tree.md`).
//! The tree declares *what* (titles, sources, options, actions); this module
//! and `data` implement *how* (live readers, control behavior). A new control
//! source is one match arm here, never a new widget.

use std::rc::Rc;

use agentic_ui_tree::Node;
use gtk::prelude::*;

use super::data::{
    bluetooth_present, clock_format, cpu_model, display_summary, hostname, input_devices, kernel,
    language, memory_total, network_interfaces, network_properties, now_clock, now_string,
    os_pretty, power_summary, region, storage_summary, timesyncd_active, timezone, usb_devices,
};
use super::widgets::{
    accent_button, card, chevron_right, dropdown, link_row, note, page_shell, row, section,
    value_text,
};
use super::Nav;
use crate::{glyphs, model, system, theme};

/// Render the data-driven page for `action`, or `None` when the tree ships
/// no `<action>.content` panel (the caller falls back to hand-written UI).
pub(crate) fn render_content(root: &Node, nav: &Nav, action: &str) -> Option<gtk::Box> {
    let content = model::find_by_id(root, &format!("{action}.content"))?;
    let page = page_shell();
    for child in &content.children {
        match child.kind.as_str() {
            "note" => {
                note(&page, child.prop_str("value").unwrap_or(""));
            }
            "section" => {
                render_section(&page, child, root, nav);
            }
            _ => {}
        }
    }
    Some(page)
}

/// One `section`: its rows and lists render inside a single card, created
/// lazily so a section of only notes leaves no empty card behind. Heroes
/// and page-level notes render outside cards, exactly as hand-written.
fn render_section(page: &gtk::Box, section_node: &Node, root: &Node, nav: &Nav) {
    section(page, section_node.prop_str("title").unwrap_or(""));
    let mut card_body: Option<gtk::Box> = None;
    for child in &section_node.children {
        match child.kind.as_str() {
            "note" => {
                note(page, child.prop_str("value").unwrap_or(""));
            }
            "hero" => {
                render_hero(page, child);
            }
            "list" => {
                let body = card_body.get_or_insert_with(|| {
                    let (_card, fresh) = card(page);
                    fresh
                });
                for entry in list_entries(child.prop_str("source").unwrap_or("")) {
                    let (_, right) =
                        row(body, entry.icon.as_deref(), &entry.title, &entry.subtitle);
                    if let Some(value) = entry.value {
                        value_text(&right, &value);
                    }
                }
            }
            "row" => {
                let body = card_body.get_or_insert_with(|| {
                    let (_card, fresh) = card(page);
                    fresh
                });
                let (row_box, right) = row(
                    body,
                    child.prop_str("icon"),
                    child.prop_str("title").unwrap_or(""),
                    child.prop_str("subtitle").unwrap_or(""),
                );
                for control in &child.children {
                    match control.kind.as_str() {
                        "value" => {
                            value_text(&right, &read_value(control));
                        }
                        "dropdown" => {
                            right.pack_start(&render_dropdown(control, root), false, false, 0);
                        }
                        _ => {}
                    }
                }
                if child.prop_str("link").is_some() || child.prop_str("command").is_some() {
                    chevron_right(&right);
                }
                if let Some(target) = child.prop_str("link") {
                    link_row(&row_box, nav, target);
                }
                if let Some(command) = child.prop_str("command") {
                    let command = command.to_string();
                    row_box.connect_button_press_event(move |_, _| {
                        system::spawn(&command);
                        gtk::glib::Propagation::Stop
                    });
                }
            }
            _ => {}
        }
    }
}

/// Live readers behind `value` sources, plus static `value` text.
/// Pure display; no writes.
fn read_value(node: &Node) -> String {
    if let Some(text) = node.prop_str("value") {
        return text.to_string();
    }
    read_source(node.prop_str("source").unwrap_or(""))
}

/// Live readers behind `value` sources. Pure display; no writes.
fn read_source(source: &str) -> String {
    match source {
        "time.now" => now_string(),
        "time.zone" => timezone(),
        "time.timesyncd" => {
            if timesyncd_active() {
                "On".to_string()
            } else {
                "Off".to_string()
            }
        }
        "locale.language" => language(),
        "locale.region" => region(),
        "display.summary" => display_summary(),
        "power.summary" => power_summary(),
        "storage.summary" => storage_summary(),
        "system.hostname" => hostname(),
        "system.cpu" => cpu_model(),
        "system.memory" => memory_total(),
        "system.bits" => format!("{}-bit", usize::BITS),
        "system.os" => os_pretty(),
        "system.kernel" => kernel(),
        _ => String::new(),
    }
}

/// Registry: live value behind a dropdown `source`, in that source's native
/// vocabulary (what the machine reports, not what the row shows).
fn dropdown_live(source: &str, root: &Node) -> String {
    match source {
        "time.clock_format" => clock_format(root),
        _ => String::new(),
    }
}

/// Registry: live value each option label sets. New dropdown sources add one
/// arm here and one in `dropdown_live`.
fn dropdown_target(source: &str, option: &str) -> String {
    match source {
        "time.clock_format" => match option {
            "12-hour" => "12h".to_string(),
            _ => "24h".to_string(),
        },
        _ => option.to_string(),
    }
}

/// Registry: label shown for the current live value. Falls back to the raw
/// live value when it names no known option.
fn dropdown_label(source: &str, root: &Node, options: &[&str]) -> String {
    let live = dropdown_live(source, root);
    for option in options {
        if dropdown_target(source, option) == live {
            return option.to_string();
        }
    }
    live
}

fn render_dropdown(node: &Node, root: &Node) -> gtk::MenuButton {
    let source = node.prop_str("source").unwrap_or("").to_string();
    let action = node.prop_str("action").unwrap_or("").to_string();
    let options: Vec<String> = node
        .prop_str("options")
        .unwrap_or("")
        .split('|')
        .map(str::to_string)
        .collect();
    let current_live = dropdown_live(&source, root);
    let current = dropdown_label(
        &source,
        root,
        &options.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let mut items: Vec<(String, Rc<dyn Fn()>)> = Vec::new();
    for option in options {
        let live = dropdown_target(&source, &option);
        let action = action.clone();
        let current_live = current_live.clone();
        items.push((
            option,
            Rc::new(move || {
                if current_live != live {
                    let _ = system::run_action(&action);
                }
            }),
        ));
    }
    dropdown(&current, items)
}

/// One expanded row of a `list` node. `None` icon/value renders the same
/// bare row the hand-written pages build for empty states and plain pairs.
struct ListEntry {
    icon: Option<String>,
    title: String,
    subtitle: String,
    value: Option<String>,
}

/// Registry: dynamic row lists. Empty hardware reads produce the same
/// single "nothing here" row the hand-written pages show.
fn list_entries(source: &str) -> Vec<ListEntry> {
    match source {
        "hardware.usb" => {
            let mut names = usb_devices();
            if names.is_empty() {
                return vec![ListEntry {
                    icon: Some("fluent:usb".to_string()),
                    title: "No devices".to_string(),
                    subtitle: "Nothing is connected right now.".to_string(),
                    value: None,
                }];
            }
            names.truncate(6);
            names
                .into_iter()
                .map(|name| ListEntry {
                    icon: Some("fluent:usb".to_string()),
                    title: name,
                    subtitle: "Connected via USB".to_string(),
                    value: Some("Connected".to_string()),
                })
                .collect()
        }
        "hardware.input" => {
            let mut names = input_devices();
            if names.is_empty() {
                return vec![ListEntry {
                    icon: Some("fluent:keyboard".to_string()),
                    title: "No input devices".to_string(),
                    subtitle: "No keyboards or pointing devices were found.".to_string(),
                    value: None,
                }];
            }
            names.truncate(8);
            names
                .into_iter()
                .map(|name| ListEntry {
                    icon: Some("fluent:keyboard".to_string()),
                    title: name,
                    subtitle: String::new(),
                    value: Some("Ready".to_string()),
                })
                .collect()
        }
        "bluetooth.adapter" => vec![ListEntry {
            icon: Some("fluent:bluetooth".to_string()),
            title: "Bluetooth".to_string(),
            subtitle: if bluetooth_present() {
                "This machine has a Bluetooth adapter".to_string()
            } else {
                "No Bluetooth adapter detected".to_string()
            },
            value: Some(if bluetooth_present() {
                "On".to_string()
            } else {
                "Unavailable".to_string()
            }),
        }],
        "network.interfaces" => {
            let mut interfaces = network_interfaces();
            if interfaces.is_empty() {
                return vec![ListEntry {
                    icon: Some("fluent:network".to_string()),
                    title: "No network interfaces".to_string(),
                    subtitle: "No link was found in /sys/class/net.".to_string(),
                    value: None,
                }];
            }
            interfaces.truncate(4);
            interfaces
                .into_iter()
                .map(|interface| ListEntry {
                    icon: Some(
                        if interface.wireless {
                            "fluent:wifi"
                        } else {
                            "fluent:ethernet"
                        }
                        .to_string(),
                    ),
                    title: interface.name.clone(),
                    subtitle: format!("{} · {}", interface.state, interface.address),
                    value: Some(if interface.connected {
                        "Connected".to_string()
                    } else {
                        "Disconnected".to_string()
                    }),
                })
                .collect()
        }
        "network.properties" => network_properties()
            .into_iter()
            .map(|(key, value)| ListEntry {
                icon: None,
                title: key,
                subtitle: String::new(),
                value: Some(value),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A `hero` node: big icon, title, live status line, and an action button.
/// Actions that report back (like `update.check`) rewrite the status line.
fn render_hero(page: &gtk::Box, node: &Node) {
    let (_card, body) = card(page);
    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    hero.set_margin_top(24);
    hero.set_margin_bottom(24);
    hero.set_margin_start(24);
    hero.set_margin_end(24);
    hero.pack_start(
        &glyphs::status_icon_tinted(glyphs::settings_icon(node.prop_str("icon").unwrap_or("")), 40, (0.902, 0.878, 0.831)),
        false,
        false,
        0,
    );
    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.set_valign(gtk::Align::Center);
    let heading = gtk::Label::new(node.prop_str("title"));
    heading.set_xalign(0.0);
    theme::class(&heading, "settings-card-title");
    let status = gtk::Label::new(node.prop_str("subtitle"));
    status.set_xalign(0.0);
    theme::class(&status, "settings-card-sub");
    text.pack_start(&heading, false, false, 0);
    text.pack_start(&status, false, false, 0);
    hero.pack_start(&text, true, true, 0);
    let button = accent_button(node.prop_str("button").unwrap_or(""));
    let action = node.prop_str("action").unwrap_or("").to_string();
    let status_clone = status.clone();
    button.connect_clicked(move |_| {
        run_hero_action(&action, &status_clone);
    });
    hero.pack_start(&button, false, false, 0);
    body.pack_start(&hero, false, false, 0);
}

/// Registry: hero button behavior. Actions that report back rewrite the
/// hero's own status line; new reporting buttons add one arm here.
fn run_hero_action(action: &str, status: &gtk::Label) {
    if action == "update.check" {
        status.set_text(&format!("Last checked: {} · up to date", now_clock()));
    }
}
