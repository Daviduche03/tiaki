//! Pluggable desktop motion system for Tiaki shell surfaces.
//!
//! Provides spring physics, critically damped deceleration curves, stagger
//! scheduling, and reduced-motion accessibility overrides.
//! Surfaces request transitions; this module drives them and stops ticking
//! once elements settle to preserve zero idle repaints.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gtk::glib;
use gtk::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    /// Critically damped spring feel (rapid start, smooth settle).
    Decel,
    /// Gentle spring with minimal overshoot.
    Spring,
    /// Direct linear ramp for simple fades.
    Linear,
}

impl Curve {
    pub fn sample(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Curve::Linear => t,
            // Cubic bezier approx: 1 - (1 - t)^3
            Curve::Decel => 1.0 - (1.0 - t).powi(3),
            // Critically damped spring simulation: 1 - e^(-6t) * cos(3t)
            Curve::Spring => {
                let decay = (-6.0 * t).exp();
                1.0 - decay * (3.0 * t).cos()
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct MotionConfig {
    pub duration_ms: u64,
    pub scale: f64,
    pub reduced_motion: bool,
}

impl Default for MotionConfig {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/desktop".to_string());
        let reduced = std::path::Path::new(&format!("{home}/.config/tiaki/reduce-motion")).exists();
        Self {
            duration_ms: if reduced { 0 } else { 200 },
            scale: if reduced { 0.0 } else { 1.0 },
            reduced_motion: reduced,
        }
    }
}

/// Calculate staggered delay for lists, grids, and icon clusters.
pub fn stagger_ms(index: usize, step_ms: u64) -> u64 {
    (index as u64) * step_ms
}

/// Animate a widget's opacity from 0.0 to 1.0 using the motion curves.
pub fn fade_in(widget: &impl IsA<gtk::Widget>, duration_ms: u64, curve: Curve) {
    let config = MotionConfig::default();
    if config.reduced_motion || duration_ms == 0 {
        widget.set_opacity(1.0);
        return;
    }

    let widget_ref = widget.clone().upcast::<gtk::Widget>();
    widget_ref.set_opacity(0.0);
    let start = Instant::now();
    let duration = (duration_ms as f64 * config.scale) as u128;

    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        let elapsed = start.elapsed().as_millis();
        let progress = (elapsed as f64 / duration as f64).min(1.0);
        let eased = curve.sample(progress);
        widget_ref.set_opacity(eased);

        if progress >= 1.0 {
            widget_ref.set_opacity(1.0);
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

/// Slide a widget along the Y-axis from an offset to its natural allocation.
pub fn slide_y(
    widget: &impl IsA<gtk::Widget>,
    start_offset: i32,
    duration_ms: u64,
    curve: Curve,
) {
    let config = MotionConfig::default();
    if config.reduced_motion || duration_ms == 0 {
        widget.set_margin_top(0);
        return;
    }

    let widget_ref = widget.clone().upcast::<gtk::Widget>();
    widget_ref.set_margin_top(start_offset);
    let start = Instant::now();
    let duration = (duration_ms as f64 * config.scale) as u128;

    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
        let elapsed = start.elapsed().as_millis();
        let progress = (elapsed as f64 / duration as f64).min(1.0);
        let eased = curve.sample(progress);
        let current_offset = ((1.0 - eased) * (start_offset as f64)) as i32;
        widget_ref.set_margin_top(current_offset);

        if progress >= 1.0 {
            widget_ref.set_margin_top(0);
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}
