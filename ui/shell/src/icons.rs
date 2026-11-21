//! Drawing entry points: the brand mark, app art, status glyphs, separators.

use gtk::prelude::*;

use crate::{glyphs, theme};

/// The Tiaki mark: the beacon. Drawn, not an asset, so it scales with the
/// theme and takes the ink of whatever surface it sits on.
pub fn logo_mark(size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    area.connect_draw(move |widget, cr| {
        let side = widget.allocated_width().min(widget.allocated_height()) as f64;
        if side <= 0.0 {
            return gtk::glib::Propagation::Proceed;
        }
        let _ = cr.save();
        cr.scale(side / 100.0, side / 100.0);
        let (r, g, b) = theme::PAPER;
        cr.set_source_rgb(r, g, b);

        // roof, lantern, gallery
        cr.move_to(50.0, 7.0);
        cr.line_to(64.0, 22.0);
        cr.line_to(36.0, 22.0);
        cr.close_path();
        let _ = cr.fill();
        cr.rectangle(40.0, 22.0, 20.0, 11.0);
        let _ = cr.fill();
        cr.rectangle(33.5, 33.0, 33.0, 6.0);
        let _ = cr.fill();

        // tower: three banded segments, real gaps
        for &(l0, r0, l1, r1, y0, y1) in &[
            (38.0, 62.0, 35.61, 64.39, 39.0, 50.7),
            (34.18, 65.82, 31.81, 68.19, 57.7, 69.3),
            (30.39, 69.61, 28.0, 72.0, 76.3, 88.0),
        ] {
            cr.move_to(l0, y0);
            cr.line_to(r0, y0);
            cr.line_to(r1, y1);
            cr.line_to(l1, y1);
            cr.close_path();
            let _ = cr.fill();
        }

        // plinth
        cr.rectangle(21.0, 88.0, 58.0, 8.0);
        let _ = cr.fill();

        let _ = cr.restore();
        gtk::glib::Propagation::Proceed
    });
    area
}

/// An app's icon as a pixbuf, or None when the file is missing.
pub fn app_icon_pixbuf(art: &str, size: i32) -> Option<gtk::gdk_pixbuf::Pixbuf> {
    let primary = format!("/usr/share/tiaki/icons/{art}.svg");
    let path = if std::path::Path::new(&primary).exists() {
        primary
    } else {
        format!("desktop/icons/{art}.svg")
    };
    gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, size, size, true).ok()
}

pub fn app_icon(art: &str, size: i32) -> gtk::Widget {
    let primary = format!("/usr/share/tiaki/icons/{art}.svg");
    let fallback = format!("desktop/icons/{art}.svg");
    let path = if std::path::Path::new(&primary).exists() {
        primary
    } else {
        fallback
    };
    // A missing icon must never take the shell down: the dock, switcher and
    // overview all draw icons for windows whose apps they have never heard of.
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, size, size, true).ok();
    if pixbuf.is_none() {
        eprintln!("tiaki-shell: missing app icon {path}");
    }
    let image = gtk::Image::from_pixbuf(pixbuf.as_ref());
    theme::class(&image, "app-icon-image");
    image.upcast()
}

pub fn status_icon(name: &str, size: i32) -> gtk::DrawingArea {
    glyphs::status_icon(name, size)
}

pub fn separator(orientation: gtk::Orientation) -> gtk::Separator {
    let separator = gtk::Separator::new(orientation);
    theme::class(&separator, "dock-sep");
    separator
}
