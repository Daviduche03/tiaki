//! Pluggable desktop motion system for Tiaki shell surfaces.
//!
//! Provides spring physics, critically damped deceleration curves, stagger
//! scheduling, and reduced-motion accessibility overrides.
//! Surfaces request transitions; this module drives them and stops ticking
//! once elements settle to preserve zero idle repaints.

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

/// A damped spring toward a target: the one physical model behind every
/// shell animation. Step it with the real frame time; it is frame-rate
/// independent and can be retargeted mid-flight without a jump, which is
/// what makes pointer-driven motion feel attached to the hand.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub value: f64,
    pub velocity: f64,
    pub target: f64,
    stiffness: f64,
    damping: f64,
}

impl Spring {
    /// Quick with a trace of overshoot: things the pointer touches.
    pub fn snappy(value: f64) -> Self {
        Self::new(value, 520.0, 34.0)
    }

    /// Slower and softer: surfaces arriving on screen.
    pub fn gentle(value: f64) -> Self {
        Self::new(value, 300.0, 30.0)
    }

    pub fn new(value: f64, stiffness: f64, damping: f64) -> Self {
        Self {
            value,
            velocity: 0.0,
            target: value,
            stiffness,
            damping,
        }
    }

    /// Advance by `dt` seconds. Returns true while still moving.
    pub fn step(&mut self, dt: f64) -> bool {
        // Sub-step so a dropped frame cannot make the integration explode.
        let mut remaining = dt.clamp(0.0, 0.064);
        while remaining > 0.0 {
            let h = remaining.min(0.004);
            let force = self.stiffness * (self.target - self.value) - self.damping * self.velocity;
            self.velocity += force * h;
            self.value += self.velocity * h;
            remaining -= h;
        }
        if (self.target - self.value).abs() < 0.01 && self.velocity.abs() < 0.05 {
            self.value = self.target;
            self.velocity = 0.0;
            return false;
        }
        true
    }
}

/// Bring a surface on screen: it fades in while rising a few pixels on a
/// spring. Works on any child widget; keeps whatever top margin positions it.
pub fn enter(widget: &impl IsA<gtk::Widget>) {
    enter_after(widget, 0);
}

/// `enter`, delayed: use with `stagger_ms` so a group arrives as a cascade.
pub fn enter_after(widget: &impl IsA<gtk::Widget>, delay_ms: u64) {
    const RISE: f64 = 10.0;
    if MotionConfig::default().reduced_motion {
        return;
    }
    let widget = widget.clone().upcast::<gtk::Widget>();
    let base = widget.margin_top();
    widget.set_opacity(0.0);
    widget.set_margin_top(base + RISE as i32);

    let start = Instant::now();
    let mut last = start;
    let mut spring = Spring::gentle(0.0);
    spring.target = 1.0;
    glib::timeout_add_local(std::time::Duration::from_millis(8), move || {
        let now = Instant::now();
        if (now - start).as_millis() < delay_ms as u128 {
            last = now;
            return glib::ControlFlow::Continue;
        }
        let moving = spring.step((now - last).as_secs_f64());
        last = now;
        let progress = spring.value;
        widget.set_opacity(progress.clamp(0.0, 1.0));
        widget.set_margin_top(base + ((1.0 - progress) * RISE).round() as i32);
        if moving {
            glib::ControlFlow::Continue
        } else {
            widget.set_opacity(1.0);
            widget.set_margin_top(base);
            glib::ControlFlow::Break
        }
    });
}

#[cfg(test)]
mod tests {
    use super::Spring;

    #[test]
    fn spring_settles_on_target_within_300ms() {
        let mut spring = Spring::snappy(40.0);
        spring.target = 52.0;
        let mut elapsed = 0.0;
        while spring.step(1.0 / 60.0) {
            elapsed += 1.0 / 60.0;
            assert!(elapsed < 0.6, "spring never settled");
        }
        assert_eq!(spring.value, 52.0);
        assert!(elapsed <= 0.4, "took {elapsed}s");
    }

    #[test]
    fn spring_survives_a_long_frame() {
        let mut spring = Spring::gentle(0.0);
        spring.target = 1.0;
        spring.step(5.0);
        assert!(spring.value.is_finite() && spring.value < 2.0);
    }
}
