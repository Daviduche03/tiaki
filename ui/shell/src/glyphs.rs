#![allow(unused_must_use)]

//! Status glyphs, drawn. App marks live in `desktop/icons/*.svg` and are
//! loaded by `icons::app_icon` — nothing draws them here.

use gtk::cairo::{Context, LineCap, LineJoin};
use gtk::glib;
use gtk::prelude::*;

const TAU: f64 = std::f64::consts::TAU;

pub fn status_icon(name: &str, size: i32) -> gtk::DrawingArea {
    status_icon_tinted(name, size, (1.0, 1.0, 1.0))
}

pub fn status_icon_tinted(name: &str, size: i32, color: (f64, f64, f64)) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    area.set_halign(gtk::Align::Center);
    area.set_valign(gtk::Align::Center);
    crate::theme::class(&area, "status-glyph");
    let name = name.to_string();
    area.connect_draw(move |widget, cr| {
        let side = widget.allocated_width().min(widget.allocated_height()) as f64;
        cr.save();
        cr.scale(side / 64.0, side / 64.0);
        draw_status(cr, &name, color);
        cr.restore();
        glib::Propagation::Proceed
    });
    area
}

// ---------------------------------------------------------------------------
// Geometry helpers

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(
        x + r,
        y + h - r,
        r,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x + r,
        y + r,
        r,
        std::f64::consts::PI,
        1.5 * std::f64::consts::PI,
    );
    cr.close_path();
}

// ---------------------------------------------------------------------------
// Fluent icons for Settings
//
// Windows 11's Settings sidebar uses filled, two-tone Fluent icons rather than
// monochrome symbolic glyphs. These are drawn on the same 64×64 grid with the
// same optical sizing, so they stay crisp at 20 px and scale with the shell.

pub fn fluent_icon(name: &str, size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    area.set_halign(gtk::Align::Center);
    area.set_valign(gtk::Align::Center);
    crate::theme::class(&area, "fluent-icon");
    let name = name.to_string();
    area.connect_draw(move |widget, cr| {
        let side = widget.allocated_width().min(widget.allocated_height()) as f64;
        if side <= 0.0 {
            return glib::Propagation::Proceed;
        }
        cr.save();
        cr.scale(side / 64.0, side / 64.0);
        draw_fluent(cr, &name);
        cr.restore();
        glib::Propagation::Proceed
    });
    area
}

const F_BLUE: (f64, f64, f64) = (0.30, 0.76, 1.00);
const F_ORANGE: (f64, f64, f64) = (1.00, 0.68, 0.30);
const F_TEAL: (f64, f64, f64) = (0.24, 0.84, 0.76);
const F_GREY: (f64, f64, f64) = (0.74, 0.76, 0.80);
const F_YELLOW: (f64, f64, f64) = (1.00, 0.83, 0.40);

fn fill(cr: &Context, color: (f64, f64, f64)) {
    cr.set_source_rgba(color.0, color.1, color.2, 1.0);
}

fn stroke(cr: &Context, color: (f64, f64, f64), width: f64) {
    cr.set_source_rgba(color.0, color.1, color.2, 0.96);
    cr.set_line_width(width);
}

fn cut(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.set_operator(gtk::cairo::Operator::Clear);
    rounded_rect(cr, x, y, w, h, r);
    let _ = cr.fill();
    cr.set_operator(gtk::cairo::Operator::Over);
}

fn draw_fluent(cr: &Context, name: &str) {
    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(LineJoin::Round);
    match name {
        "home" => {
            fill(cr, F_ORANGE);
            cr.move_to(32.0, 6.0);
            cr.line_to(58.0, 29.0);
            cr.line_to(50.0, 29.0);
            cr.line_to(50.0, 55.0);
            cr.line_to(14.0, 55.0);
            cr.line_to(14.0, 29.0);
            cr.line_to(6.0, 29.0);
            cr.close_path();
            let _ = cr.fill();
            cut(cr, 26.0, 37.0, 12.0, 18.0, 2.0);
        }
        "system" => {
            fill(cr, F_BLUE);
            rounded_rect(cr, 6.0, 11.0, 52.0, 35.0, 4.0);
            let _ = cr.fill();
            cut(cr, 11.0, 16.0, 42.0, 25.0, 2.0);
            fill(cr, F_BLUE);
            rounded_rect(cr, 26.0, 46.0, 12.0, 4.0, 1.0);
            let _ = cr.fill();
            rounded_rect(cr, 18.0, 52.0, 28.0, 5.0, 2.5);
            let _ = cr.fill();
        }
        "bluetooth" => {
            stroke(cr, F_BLUE, 5.6);
            cr.move_to(32.0, 10.0);
            cr.line_to(32.0, 54.0);
            cr.move_to(32.0, 10.0);
            cr.line_to(45.0, 21.0);
            cr.line_to(19.0, 43.0);
            cr.move_to(32.0, 54.0);
            cr.line_to(45.0, 43.0);
            cr.line_to(19.0, 21.0);
            let _ = cr.stroke();
        }
        "network" | "wifi" => {
            stroke(cr, F_BLUE, 5.4);
            for radius in [11.0, 19.0, 27.0] {
                cr.arc(
                    32.0,
                    45.0,
                    radius,
                    -0.78 * std::f64::consts::PI,
                    -0.22 * std::f64::consts::PI,
                );
                let _ = cr.stroke();
            }
            fill(cr, F_BLUE);
            cr.arc(32.0, 45.0, 3.8, 0.0, TAU);
            let _ = cr.fill();
        }
        "personalization" => {
            fill(cr, F_ORANGE);
            cr.set_line_width(10.0);
            cr.set_source_rgba(F_ORANGE.0, F_ORANGE.1, F_ORANGE.2, 1.0);
            cr.move_to(21.0, 44.0);
            cr.line_to(43.0, 22.0);
            let _ = cr.stroke();
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.92);
            cr.set_line_width(10.0);
            cr.move_to(41.0, 20.0);
            cr.line_to(47.0, 26.0);
            let _ = cr.stroke();
            cr.move_to(9.0, 55.0);
            cr.line_to(22.0, 53.0);
            cr.line_to(12.0, 42.0);
            cr.close_path();
            let _ = cr.fill();
        }
        "apps" => {
            for (x, y, color) in [
                (10.0, 10.0, F_BLUE),
                (34.0, 10.0, F_BLUE),
                (10.0, 34.0, F_BLUE),
                (34.0, 34.0, F_YELLOW),
            ] {
                fill(cr, color);
                rounded_rect(cr, x, y, 20.0, 20.0, 4.0);
                let _ = cr.fill();
            }
        }
        "accounts" => {
            fill(cr, F_TEAL);
            cr.arc(32.0, 20.0, 10.0, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(32.0, 52.0, 19.0, std::f64::consts::PI, TAU);
            cr.close_path();
            let _ = cr.fill();
        }
        "time" => {
            stroke(cr, F_BLUE, 5.0);
            cr.arc(32.0, 32.0, 22.0, 0.0, TAU);
            let _ = cr.stroke();
            cr.move_to(32.0, 18.0);
            cr.line_to(32.0, 33.0);
            cr.line_to(43.0, 38.0);
            let _ = cr.stroke();
        }
        "gaming" => {
            fill(cr, F_GREY);
            rounded_rect(cr, 7.0, 22.0, 50.0, 23.0, 11.0);
            let _ = cr.fill();
            cr.arc(17.0, 43.0, 8.0, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(47.0, 43.0, 8.0, 0.0, TAU);
            let _ = cr.fill();
            cut(cr, 16.0, 30.0, 13.0, 4.0, 1.0);
            cut(cr, 20.5, 25.5, 4.0, 13.0, 1.0);
            cut(cr, 41.0, 27.0, 5.0, 5.0, 2.5);
            cut(cr, 48.0, 32.0, 5.0, 5.0, 2.5);
            cut(cr, 41.0, 37.0, 5.0, 5.0, 2.5);
        }
        "accessibility" => {
            fill(cr, F_BLUE);
            cr.arc(32.0, 12.0, 6.5, 0.0, TAU);
            let _ = cr.fill();
            cr.set_line_width(7.0);
            cr.set_source_rgba(F_BLUE.0, F_BLUE.1, F_BLUE.2, 1.0);
            cr.move_to(32.0, 22.0);
            cr.line_to(32.0, 38.0);
            cr.move_to(14.0, 27.0);
            cr.line_to(50.0, 27.0);
            cr.move_to(32.0, 38.0);
            cr.line_to(22.0, 54.0);
            cr.move_to(32.0, 38.0);
            cr.line_to(42.0, 54.0);
            let _ = cr.stroke();
        }
        "privacy" => {
            fill(cr, F_GREY);
            cr.move_to(32.0, 6.0);
            cr.line_to(53.0, 15.0);
            cr.line_to(50.0, 36.0);
            cr.line_to(32.0, 55.0);
            cr.line_to(14.0, 36.0);
            cr.line_to(11.0, 15.0);
            cr.close_path();
            let _ = cr.fill();
            cut(cr, 28.0, 22.0, 8.0, 8.0, 4.0);
            cut(cr, 30.0, 29.0, 4.0, 11.0, 2.0);
        }
        "update" => {
            stroke(cr, F_BLUE, 5.2);
            cr.arc(
                32.0,
                32.0,
                20.0,
                -0.55 * std::f64::consts::PI,
                1.05 * std::f64::consts::PI,
            );
            let _ = cr.stroke();
            fill(cr, F_BLUE);
            cr.move_to(51.0, 14.0);
            cr.line_to(53.0, 29.0);
            cr.line_to(39.0, 25.0);
            cr.close_path();
            let _ = cr.fill();
        }
        "device" => {
            fill(cr, F_BLUE);
            rounded_rect(cr, 8.0, 12.0, 48.0, 31.0, 3.0);
            let _ = cr.fill();
            cut(cr, 13.0, 17.0, 38.0, 21.0, 2.0);
            fill(cr, F_BLUE);
            rounded_rect(cr, 5.0, 47.0, 54.0, 5.0, 2.5);
            let _ = cr.fill();
        }
        "usb" => {
            fill(cr, F_BLUE);
            rounded_rect(cr, 22.0, 8.0, 20.0, 20.0, 3.0);
            let _ = cr.fill();
            rounded_rect(cr, 27.0, 28.0, 10.0, 10.0, 2.0);
            let _ = cr.fill();
            stroke(cr, F_BLUE, 5.0);
            cr.move_to(32.0, 38.0);
            cr.line_to(32.0, 54.0);
            let _ = cr.stroke();
            cr.set_source_rgba(0.12, 0.12, 0.13, 1.0);
            rounded_rect(cr, 27.0, 11.0, 4.0, 7.0, 1.0);
            let _ = cr.fill();
            rounded_rect(cr, 33.0, 11.0, 4.0, 7.0, 1.0);
            let _ = cr.fill();
        }
        "keyboard" => {
            stroke(cr, F_GREY, 4.0);
            rounded_rect(cr, 6.0, 18.0, 52.0, 28.0, 4.0);
            let _ = cr.stroke();
            fill(cr, F_GREY);
            for row in 0..2 {
                for col in 0..7 {
                    rounded_rect(
                        cr,
                        12.0 + col as f64 * 6.2,
                        25.0 + row as f64 * 6.5,
                        3.4,
                        3.4,
                        0.8,
                    );
                    let _ = cr.fill();
                }
            }
            rounded_rect(cr, 20.0, 38.0, 24.0, 3.4, 1.2);
            let _ = cr.fill();
        }
        "ethernet" => {
            fill(cr, F_BLUE);
            rounded_rect(cr, 16.0, 10.0, 32.0, 28.0, 3.0);
            let _ = cr.fill();
            cut(cr, 22.0, 16.0, 20.0, 16.0, 1.0);
            fill(cr, F_BLUE);
            rounded_rect(cr, 28.0, 38.0, 8.0, 8.0, 1.5);
            let _ = cr.fill();
            stroke(cr, F_BLUE, 4.0);
            cr.move_to(32.0, 46.0);
            cr.line_to(32.0, 56.0);
            let _ = cr.stroke();
        }
        "back" => {
            stroke(cr, (1.0, 1.0, 1.0), 5.2);
            cr.move_to(40.0, 12.0);
            cr.line_to(22.0, 32.0);
            cr.line_to(40.0, 52.0);
            let _ = cr.stroke();
        }
        "chevron_right" => {
            stroke(cr, (0.78, 0.78, 0.78), 5.2);
            cr.move_to(24.0, 14.0);
            cr.line_to(42.0, 32.0);
            cr.line_to(24.0, 50.0);
            let _ = cr.stroke();
        }
        "chevron_down" => {
            stroke(cr, (0.78, 0.78, 0.78), 5.2);
            cr.move_to(14.0, 25.0);
            cr.line_to(32.0, 43.0);
            cr.line_to(50.0, 25.0);
            let _ = cr.stroke();
        }
        "chevron_up" => {
            stroke(cr, (0.78, 0.78, 0.78), 5.2);
            cr.move_to(14.0, 43.0);
            cr.line_to(32.0, 25.0);
            cr.line_to(50.0, 43.0);
            let _ = cr.stroke();
        }
        "win_min" => {
            stroke(cr, (1.0, 1.0, 1.0), 4.4);
            cr.move_to(16.0, 32.0);
            cr.line_to(48.0, 32.0);
            let _ = cr.stroke();
        }
        "win_max" => {
            stroke(cr, (1.0, 1.0, 1.0), 3.8);
            rounded_rect(cr, 18.0, 18.0, 28.0, 28.0, 1.0);
            let _ = cr.stroke();
        }
        "win_close" => {
            stroke(cr, (1.0, 1.0, 1.0), 4.2);
            cr.move_to(18.0, 18.0);
            cr.line_to(46.0, 46.0);
            cr.move_to(46.0, 18.0);
            cr.line_to(18.0, 46.0);
            let _ = cr.stroke();
        }
        "search" => {
            stroke(cr, (0.78, 0.78, 0.78), 5.0);
            cr.arc(27.0, 27.0, 13.0, 0.0, TAU);
            let _ = cr.stroke();
            cr.move_to(36.5, 36.5);
            cr.line_to(49.0, 49.0);
            let _ = cr.stroke();
        }
        "sound" => {
            fill(cr, F_BLUE);
            cr.move_to(10.0, 25.0);
            cr.line_to(20.0, 25.0);
            cr.line_to(33.0, 14.0);
            cr.line_to(33.0, 50.0);
            cr.line_to(20.0, 39.0);
            cr.line_to(10.0, 39.0);
            cr.close_path();
            let _ = cr.fill();
            stroke(cr, F_BLUE, 4.6);
            cr.arc(
                35.0,
                32.0,
                11.0,
                -0.42 * std::f64::consts::PI,
                0.42 * std::f64::consts::PI,
            );
            let _ = cr.stroke();
            cr.arc(
                35.0,
                32.0,
                19.0,
                -0.42 * std::f64::consts::PI,
                0.42 * std::f64::consts::PI,
            );
            let _ = cr.stroke();
        }
        "power" => {
            stroke(cr, F_BLUE, 5.2);
            cr.arc(
                32.0,
                33.0,
                20.0,
                -0.32 * std::f64::consts::PI,
                1.32 * std::f64::consts::PI,
            );
            let _ = cr.stroke();
            cr.move_to(32.0, 8.0);
            cr.line_to(32.0, 30.0);
            let _ = cr.stroke();
        }
        "storage" => {
            fill(cr, F_BLUE);
            rounded_rect(cr, 8.0, 14.0, 48.0, 18.0, 3.0);
            let _ = cr.fill();
            rounded_rect(cr, 8.0, 34.0, 48.0, 18.0, 3.0);
            let _ = cr.fill();
            cut(cr, 16.0, 21.0, 6.0, 4.0, 2.0);
            cut(cr, 16.0, 41.0, 6.0, 4.0, 2.0);
        }
        "info" => {
            stroke(cr, F_BLUE, 4.8);
            cr.arc(32.0, 32.0, 22.0, 0.0, TAU);
            let _ = cr.stroke();
            fill(cr, F_BLUE);
            cr.arc(32.0, 19.0, 3.2, 0.0, TAU);
            let _ = cr.fill();
            rounded_rect(cr, 29.5, 27.0, 5.0, 18.0, 2.5);
            let _ = cr.fill();
        }
        "brush" => {
            stroke(cr, F_BLUE, 6.0);
            cr.move_to(45.0, 10.0);
            cr.line_to(54.0, 19.0);
            let _ = cr.stroke();
            fill(cr, F_BLUE);
            cr.move_to(37.0, 19.0);
            cr.line_to(49.0, 31.0);
            cr.line_to(42.0, 38.0);
            cr.line_to(30.0, 26.0);
            cr.close_path();
            let _ = cr.fill();
            cr.move_to(30.0, 26.0);
            cr.line_to(42.0, 38.0);
            cr.curve_to(38.0, 48.0, 28.0, 52.0, 14.0, 55.0);
            cr.curve_to(20.0, 42.0, 24.0, 33.0, 30.0, 26.0);
            cr.close_path();
            let _ = cr.fill();
        }
        "gear" => {
            fill(cr, (1.0, 1.0, 1.0));
            cr.save();
            cr.translate(32.0, 32.0);
            for index in 0..8 {
                cr.save();
                cr.rotate(index as f64 / 8.0 * TAU);
                rounded_rect(cr, -3.2, -22.0, 6.4, 9.0, 2.0);
                let _ = cr.fill();
                cr.restore();
            }
            cr.restore();
            cr.arc(32.0, 32.0, 16.0, 0.0, TAU);
            let _ = cr.fill();
            cut(cr, 26.0, 26.0, 12.0, 12.0, 6.0);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Status glyphs

fn draw_status(cr: &Context, name: &str, color: (f64, f64, f64)) {
    cr.set_line_cap(LineCap::Round);
    cr.set_line_join(LineJoin::Round);
    let white = (color.0, color.1, color.2, 0.95);

    match name {
        "wifi" => {
            cr.save();
            cr.translate(0.0, -4.0);
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(5.0);
            for (radius, start, end) in [
                (
                    10.0,
                    -0.75 * std::f64::consts::PI,
                    -0.25 * std::f64::consts::PI,
                ),
                (
                    18.0,
                    -0.78 * std::f64::consts::PI,
                    -0.22 * std::f64::consts::PI,
                ),
                (
                    26.0,
                    -0.80 * std::f64::consts::PI,
                    -0.20 * std::f64::consts::PI,
                ),
            ] {
                cr.arc(32.0, 46.0, radius, start, end);
                let _ = cr.stroke();
            }
            cr.arc(32.0, 46.0, 3.6, 0.0, TAU);
            let _ = cr.fill();
            cr.restore();
        }
        "audio" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.move_to(12.0, 25.0);
            cr.line_to(20.0, 25.0);
            cr.line_to(30.0, 16.0);
            cr.line_to(30.0, 48.0);
            cr.line_to(20.0, 39.0);
            cr.line_to(12.0, 39.0);
            cr.close_path();
            let _ = cr.fill();
            cr.set_line_width(4.0);
            for radius in [8.0, 15.0] {
                cr.arc(31.0, 32.0, radius, -0.85, 0.85);
                let _ = cr.stroke();
            }
        }
        "battery" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(3.6);
            rounded_rect(cr, 10.0, 15.0, 38.0, 34.0, 5.0);
            let _ = cr.stroke();
            rounded_rect(cr, 50.0, 23.0, 4.0, 18.0, 1.6);
            let _ = cr.fill();
            rounded_rect(cr, 15.0, 20.0, 24.0, 24.0, 3.0);
            let _ = cr.fill();
        }
        "search" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.6);
            cr.arc(27.0, 27.0, 11.5, 0.0, TAU);
            let _ = cr.stroke();
            cr.set_line_width(5.4);
            cr.move_to(35.5, 35.5);
            cr.line_to(46.0, 46.0);
            let _ = cr.stroke();
        }
        "bell" | "notifications" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.move_to(32.0, 13.0);
            cr.curve_to(40.0, 13.0, 42.0, 21.0, 42.0, 29.0);
            cr.curve_to(42.0, 37.0, 46.0, 40.0, 47.0, 44.0);
            cr.line_to(17.0, 44.0);
            cr.curve_to(18.0, 40.0, 22.0, 37.0, 22.0, 29.0);
            cr.curve_to(22.0, 21.0, 24.0, 13.0, 32.0, 13.0);
            cr.close_path();
            let _ = cr.fill();
            cr.arc(32.0, 49.5, 4.2, 0.0, TAU);
            let _ = cr.fill();
        }
        "control" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.0);
            cr.move_to(12.0, 22.0);
            cr.line_to(52.0, 22.0);
            cr.move_to(12.0, 42.0);
            cr.line_to(52.0, 42.0);
            let _ = cr.stroke();
            cr.arc(42.0, 22.0, 4.6, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(24.0, 42.0, 4.6, 0.0, TAU);
            let _ = cr.fill();
        }
        "inference" => {
            cr.set_source_rgb(0.62, 0.95, 0.70);
            cr.arc(32.0, 32.0, 9.0, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(32.0, 32.0, 14.0, 0.0, TAU);
            cr.set_source_rgba(0.62, 0.95, 0.70, 0.28);
            cr.set_line_width(2.0);
            let _ = cr.stroke();
        }
        "bluetooth" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.0);
            cr.move_to(32.0, 12.0);
            cr.line_to(32.0, 52.0);
            cr.move_to(32.0, 12.0);
            cr.line_to(44.0, 22.0);
            cr.line_to(20.0, 42.0);
            cr.move_to(32.0, 52.0);
            cr.line_to(44.0, 42.0);
            cr.line_to(20.0, 22.0);
            let _ = cr.stroke();
        }
        "brightness" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.arc(32.0, 32.0, 10.0, 0.0, TAU);
            let _ = cr.fill();
            cr.set_line_width(3.4);
            for index in 0..8 {
                let angle = index as f64 / 8.0 * TAU;
                cr.move_to(32.0 + 16.0 * angle.cos(), 32.0 + 16.0 * angle.sin());
                cr.line_to(32.0 + 22.0 * angle.cos(), 32.0 + 22.0 * angle.sin());
            }
            let _ = cr.stroke();
        }
        "lock" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.0);
            cr.arc(
                32.0,
                27.0,
                9.0,
                std::f64::consts::PI,
                2.0 * std::f64::consts::PI,
            );
            let _ = cr.stroke();
            cr.move_to(23.0, 27.0);
            cr.line_to(23.0, 34.0);
            cr.move_to(41.0, 27.0);
            cr.line_to(41.0, 34.0);
            let _ = cr.stroke();
            rounded_rect(cr, 19.0, 33.0, 26.0, 20.0, 5.0);
            let _ = cr.fill();
        }
        "power" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.2);
            cr.arc(
                32.0,
                32.0,
                14.0,
                -0.75 * std::f64::consts::PI,
                0.75 * std::f64::consts::PI,
            );
            let _ = cr.stroke();
            cr.move_to(32.0, 14.0);
            cr.line_to(32.0, 32.0);
            let _ = cr.stroke();
        }
        "monitor" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(3.6);
            rounded_rect(cr, 10.0, 14.0, 44.0, 30.0, 4.0);
            let _ = cr.stroke();
            cr.move_to(26.0, 50.0);
            cr.line_to(38.0, 50.0);
            let _ = cr.stroke();
            cr.move_to(32.0, 44.0);
            cr.line_to(32.0, 50.0);
            let _ = cr.stroke();
        }
        "image" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(3.6);
            rounded_rect(cr, 10.0, 14.0, 44.0, 36.0, 5.0);
            let _ = cr.stroke();
            cr.arc(22.0, 25.0, 3.4, 0.0, TAU);
            let _ = cr.fill();
            cr.move_to(12.0, 44.0);
            cr.line_to(26.0, 30.0);
            cr.line_to(34.0, 38.0);
            cr.line_to(40.0, 32.0);
            cr.line_to(52.0, 44.0);
            cr.close_path();
            let _ = cr.fill();
        }
        "moon" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.arc(32.0, 32.0, 16.0, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(39.0, 26.0, 13.0, 0.0, TAU);
            cr.set_operator(gtk::cairo::Operator::Clear);
            let _ = cr.fill();
            cr.set_operator(gtk::cairo::Operator::Over);
        }
        "info" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.0);
            cr.arc(32.0, 32.0, 16.5, 0.0, TAU);
            let _ = cr.stroke();
            cr.arc(32.0, 23.0, 2.6, 0.0, TAU);
            let _ = cr.fill();
            cr.set_line_width(4.4);
            cr.move_to(32.0, 30.0);
            cr.line_to(32.0, 43.0);
            let _ = cr.stroke();
        }
        "hdr" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(3.0);
            rounded_rect(cr, 5.0, 20.0, 54.0, 24.0, 4.0);
            let _ = cr.stroke();
            cr.select_font_face(
                "Inter",
                gtk::cairo::FontSlant::Normal,
                gtk::cairo::FontWeight::Bold,
            );
            cr.set_font_size(13.0);
            if let Ok(extents) = cr.text_extents("HDR") {
                cr.move_to(32.0 - extents.width() / 2.0, 32.0 + extents.height() / 2.0);
                let _ = cr.show_text("HDR");
            }
        }
        "palette" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(3.4);
            cr.arc(32.0, 32.0, 18.0, 0.0, TAU);
            let _ = cr.stroke();
            cr.arc(26.0, 25.0, 2.4, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(39.0, 26.0, 2.4, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(23.0, 37.0, 2.4, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(38.0, 38.0, 2.4, 0.0, TAU);
            let _ = cr.fill();
            cr.set_line_width(3.4);
            cr.move_to(32.0, 50.0);
            cr.curve_to(28.0, 44.0, 32.0, 40.0, 36.0, 43.0);
            let _ = cr.stroke();
        }
        "spark" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.move_to(32.0, 10.0);
            cr.curve_to(35.0, 25.0, 39.0, 29.0, 54.0, 32.0);
            cr.curve_to(39.0, 35.0, 35.0, 39.0, 32.0, 54.0);
            cr.curve_to(29.0, 39.0, 25.0, 35.0, 10.0, 32.0);
            cr.curve_to(25.0, 29.0, 29.0, 25.0, 32.0, 10.0);
            cr.close_path();
            let _ = cr.fill();
        }
        "clock" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.0);
            cr.arc(32.0, 32.0, 17.0, 0.0, TAU);
            let _ = cr.stroke();
            cr.move_to(32.0, 32.0);
            cr.line_to(32.0, 21.0);
            cr.move_to(32.0, 32.0);
            cr.line_to(40.0, 35.0);
            let _ = cr.stroke();
        }
        "gear" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.save();
            cr.translate(32.0, 32.0);
            for index in 0..8 {
                cr.save();
                cr.rotate(index as f64 / 8.0 * TAU);
                rounded_rect(cr, -3.0, -21.0, 6.0, 8.0, 2.0);
                let _ = cr.fill();
                cr.restore();
            }
            cr.restore();
            cr.arc(32.0, 32.0, 15.0, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(32.0, 32.0, 6.0, 0.0, TAU);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.0);
            let _ = cr.fill_preserve();
            cr.set_operator(gtk::cairo::Operator::Clear);
            let _ = cr.fill_preserve();
            cr.set_operator(gtk::cairo::Operator::Over);
        }
        // ---------------------------------------------------------------
        // Settings marks. Same 64-grid, same stroke language: paper lines,
        // no fills except dots. Added when Settings was rebuilt in Tiaki's
        // material so no page reaches for the old two-tone Fluent set.
        "home" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(5.0);
            cr.move_to(10.0, 30.0);
            cr.line_to(32.0, 12.0);
            cr.line_to(54.0, 30.0);
            cr.move_to(17.0, 27.0);
            cr.line_to(17.0, 52.0);
            cr.line_to(47.0, 52.0);
            cr.line_to(47.0, 27.0);
            let _ = cr.stroke();
        }
        "globe" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.0);
            cr.arc(32.0, 32.0, 19.0, 0.0, TAU);
            let _ = cr.stroke();
            cr.move_to(13.0, 32.0);
            cr.line_to(51.0, 32.0);
            cr.move_to(32.0, 13.0);
            cr.curve_to(22.0, 22.0, 22.0, 42.0, 32.0, 51.0);
            cr.curve_to(42.0, 42.0, 42.0, 22.0, 32.0, 13.0);
            let _ = cr.stroke();
        }
        "apps" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            for (x, y) in [(18.0, 18.0), (46.0, 18.0), (18.0, 46.0), (46.0, 46.0)] {
                rounded_rect(cr, x - 8.0, y - 8.0, 16.0, 16.0, 3.0);
                let _ = cr.stroke();
            }
        }
        "person" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.5);
            cr.arc(32.0, 22.0, 9.0, 0.0, TAU);
            let _ = cr.stroke();
            cr.move_to(14.0, 52.0);
            cr.curve_to(14.0, 40.0, 22.0, 35.0, 32.0, 35.0);
            cr.curve_to(42.0, 35.0, 50.0, 40.0, 50.0, 52.0);
            let _ = cr.stroke();
        }
        "gamepad" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.5);
            cr.move_to(12.0, 24.0);
            cr.line_to(20.0, 24.0);
            cr.line_to(24.0, 20.0);
            cr.line_to(40.0, 20.0);
            cr.line_to(44.0, 24.0);
            cr.line_to(52.0, 24.0);
            cr.curve_to(52.0, 40.0, 44.0, 48.0, 32.0, 48.0);
            cr.curve_to(20.0, 48.0, 12.0, 40.0, 12.0, 24.0);
            let _ = cr.stroke();
            cr.move_to(22.0, 30.0);
            cr.line_to(22.0, 38.0);
            cr.move_to(18.0, 34.0);
            cr.line_to(26.0, 34.0);
            let _ = cr.stroke();
            cr.arc(41.0, 32.0, 1.8, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(45.0, 38.0, 1.8, 0.0, TAU);
            let _ = cr.fill();
        }
        "a11y" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.5);
            cr.arc(32.0, 13.0, 4.5, 0.0, TAU);
            let _ = cr.fill();
            cr.move_to(12.0, 24.0);
            cr.line_to(52.0, 24.0);
            cr.move_to(32.0, 24.0);
            cr.line_to(32.0, 40.0);
            cr.move_to(32.0, 40.0);
            cr.line_to(22.0, 52.0);
            cr.move_to(32.0, 40.0);
            cr.line_to(42.0, 52.0);
            let _ = cr.stroke();
        }
        "shield" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.5);
            cr.move_to(32.0, 10.0);
            cr.line_to(48.0, 16.0);
            cr.line_to(48.0, 30.0);
            cr.curve_to(48.0, 42.0, 40.0, 49.0, 32.0, 52.0);
            cr.curve_to(24.0, 49.0, 16.0, 42.0, 16.0, 30.0);
            cr.line_to(16.0, 16.0);
            cr.close_path();
            let _ = cr.stroke();
        }
        "update" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(5.0);
            cr.arc(32.0, 32.0, 17.0, -0.4, 3.6);
            let _ = cr.stroke();
            cr.move_to(32.0, 6.0);
            cr.line_to(32.0, 18.0);
            cr.line_to(42.0, 12.0);
            cr.close_path();
            let _ = cr.fill();
        }
        "drive" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.0);
            rounded_rect(cr, 10.0, 20.0, 44.0, 24.0, 3.0);
            let _ = cr.stroke();
            cr.arc(47.0, 32.0, 2.0, 0.0, TAU);
            let _ = cr.fill();
        }
        "device" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.0);
            rounded_rect(cr, 18.0, 8.0, 28.0, 40.0, 3.0);
            let _ = cr.stroke();
            cr.move_to(28.0, 52.0);
            cr.line_to(36.0, 52.0);
            let _ = cr.stroke();
        }
        "usb" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.5);
            cr.move_to(26.0, 12.0);
            cr.line_to(26.0, 50.0);
            cr.line_to(16.0, 40.0);
            cr.move_to(26.0, 50.0);
            cr.line_to(36.0, 40.0);
            let _ = cr.stroke();
            cr.arc(26.0, 12.0, 3.2, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(14.0, 36.0, 3.2, 0.0, TAU);
            let _ = cr.fill();
            cr.arc(38.0, 36.0, 3.2, 0.0, TAU);
            let _ = cr.fill();
        }
        "keyboard" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.0);
            rounded_rect(cr, 8.0, 20.0, 48.0, 24.0, 3.0);
            let _ = cr.stroke();
            cr.move_to(24.0, 32.0);
            cr.line_to(40.0, 32.0);
            let _ = cr.stroke();
        }
        "ethernet" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.5);
            cr.move_to(32.0, 10.0);
            cr.line_to(32.0, 26.0);
            cr.move_to(32.0, 34.0);
            cr.line_to(32.0, 54.0);
            let _ = cr.stroke();
            rounded_rect(cr, 20.0, 22.0, 24.0, 16.0, 2.0);
            let _ = cr.stroke();
        }
        "brush" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(4.5);
            cr.move_to(18.0, 46.0);
            cr.line_to(38.0, 16.0);
            cr.line_to(48.0, 26.0);
            cr.line_to(28.0, 52.0);
            cr.line_to(18.0, 52.0);
            cr.close_path();
            let _ = cr.stroke();
        }
        "chevron_right" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(5.0);
            cr.move_to(24.0, 14.0);
            cr.line_to(40.0, 32.0);
            cr.line_to(24.0, 50.0);
            let _ = cr.stroke();
        }
        "chevron_down" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(5.0);
            cr.move_to(14.0, 24.0);
            cr.line_to(32.0, 40.0);
            cr.line_to(50.0, 24.0);
            let _ = cr.stroke();
        }
        "chevron_up" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(5.0);
            cr.move_to(14.0, 40.0);
            cr.line_to(32.0, 24.0);
            cr.line_to(50.0, 40.0);
            let _ = cr.stroke();
        }
        "back" => {
            cr.set_source_rgba(white.0, white.1, white.2, white.3);
            cr.set_line_width(5.0);
            cr.move_to(40.0, 14.0);
            cr.line_to(22.0, 32.0);
            cr.line_to(40.0, 50.0);
            let _ = cr.stroke();
        }
        _ => {}
    }
}

/// The Settings icon language: every page, row and hero resolves through
/// here, so nothing reaches for the old two-tone Fluent set. Names arrive
/// with or without the legacy `fluent:` prefix.
pub fn settings_icon(name: &str) -> &str {
    match name.strip_prefix("fluent:").unwrap_or(name) {
        "sound" => "audio",
        "system" => "gear",
        "time" => "clock",
        "network" => "globe",
        "personalization" => "palette",
        "privacy" => "shield",
        "accounts" => "person",
        "gaming" => "gamepad",
        "accessibility" => "a11y",
        "storage" => "drive",
        other => other,
    }
}
