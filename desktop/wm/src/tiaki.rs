//! Tiaki's window frame.
//!
//! A window is a panel from the same material as the bar, Control Center and
//! the dock — not a separate language. The frame copies those CSS values
//! exactly instead of inventing a WM aesthetic:
//!
//! - Ink `#151413` title bars on every window (the bar is ink on every
//!   wallpaper).
//! - 1px hairlines, paper at 14% unfocused (`--rule-on-ink`). Focus is the
//!   same lamp inset the launcher uses for a selected cell: lamp at 55%
//!   (`rgba(208,138,44,0.55)`), never a solid amber perimeter.
//! - Soft shadows (`0 18px 44px rgba(0,0,0,0.55)` like the dock), never a
//!   hard offset.
//! - Radii follow the hierarchy that already ships: floating panels 3px,
//!   windows 10px, dock 14px. Buttons are 3px like every bar button.
//! - Type is IBM Plex Sans at 12.5px / 500, the bar's own size and weight.
//!
//! Spec: `docs/design/brand/color.md`. CSS: `ui/shell/css/`.

#![allow(dead_code)]

use shojiwm_rs::prelude::*;

// Core. Exact values from the spec.
pub const PAPER: &str = "#E6E0D4";
pub const INK: &str = "#151413";
pub const LAMP_ON_INK: &str = "#D08A2C";

// Neutrals.
pub const INK_RAISED: &str = "#1B1917";
pub const INK_3: &str = "#665F54";

// Structure. Hairlines are paper at low alpha, never a grey value.
pub const RULE_ON_INK: &str = "#E6E0D424"; // 14% — every panel in the shell
pub const FOCUS_EDGE: &str = "#E6E0D452"; // paper at 32% — focused hairline
pub const FOCUS_RING: &str = "#D08A2C8C"; // lamp at 55% — switcher selection only
pub const PAPER_WASH: &str = "#E6E0D412"; // 7% — bar-button hover
pub const PAPER_PRESS: &str = "#E6E0D41C"; // 11% — bar-button active
pub const PAPER_EDGE: &str = "#E6E0D42E"; // 18% — dock-border hover
pub const PAPER_DIM: &str = "#E6E0D49E"; // 62% — unfocused title, menu-button text

/// No shadow. Windows sit flush on the wallpaper; focus is the hairline
/// brightening only.
pub fn lift() -> Vec<BoxShadow> {
    vec![]
}

/// No shadow, focused or not.
pub fn rest() -> Vec<BoxShadow> {
    vec![]
}

/// The switcher-selected card lifts the way the launcher marks selection:
/// a lamp ring at 55%, but only while the switcher is open.
pub fn selected() -> Vec<BoxShadow> {
    vec![BoxShadow {
        spread: 3.0,
        ..shadow(0.0, 0.0, 0.0, hex(FOCUS_RING))
    }]
}

/// No title bar is drawn. The constant stays so geometry that used to
/// reserve title-bar space keeps compiling as zero.
pub const TITLEBAR_HEIGHT: f64 = 0.0;

/// The frame border is the panels' 1px hairline. The resize grab area is
/// separate (`resize_hit_area`), so a thin border costs nothing in usability.
pub const BORDER_PX: f64 = 1.0;

/// Windows sit between the dock (14px) and the small panels (3px).
pub const FRAME_RADIUS: f64 = 10.0;

/// Every button in the shell — bar, Control Center, notifications — is 3px.
pub const BUTTON_RADIUS: f64 = 3.0;

/// Tooltips are panels: same radius, same shadow as Control Center.
pub const PANEL_RADIUS: f64 = 3.0;

pub const UI_FONT: &str = "IBM Plex Sans";
pub const MONO_FONT: &str = "IBM Plex Mono";
