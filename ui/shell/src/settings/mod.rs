//! Settings, in Tiaki's material: ink panels, paper text, hairlines.
//!
//! A sidebar with a profile and setting search, a breadcrumb page title,
//! and full-width card groups whose rows carry a trailing control.
//! The sidebar pages are tree data; every control below them reads or
//! changes the real machine.
//!
//! Layout: `run` and navigation live here; `widgets` holds the card/row
//! control primitives; `data` reads the live machine; one file per page
//! (`home`, `system`, `display`, ...) renders Tiaki content.

use std::cell::RefCell;
use std::rc::Rc;

use agentic_ui_tree::Node;
use gtk::gdk;
use gtk::prelude::*;

use crate::{glyphs, model, single, theme};

mod about;
mod accessibility;
mod accounts;
mod apps;
mod bluetooth;
mod data;
mod display;
mod gaming;
mod home;
mod network;
mod personalization;
mod power;
mod privacy;
mod render;
mod sound;
mod storage;
mod system;
mod time;
mod update;
mod widgets;

use self::widgets::avatar;

/// Stylesheets owned by this module, in cascade order.
/// Add a file to `ui/shell/css/` and list it here to grow them.
pub const STYLE_SHEETS: &[&str] = &[include_str!("../../css/settings.css")];

pub fn run(root: &Node, start_page: Option<&str>) {
    let Some(_instance) = single::Instance::acquire("tiaki-settings", "settings", false) else {
        return;
    };

    let pages = pages_from(root);
    let first = pages
        .iter()
        .find(|page| !page.hidden)
        .cloned()
        .unwrap_or(Page {
            label: "Home".into(),
            action: "settings.page.home".into(),
            icon: "home".into(),
            parent: None,
            hidden: false,
        });

    // A normal app window, managed by Sway like every other app. The macOS
    // top bar stays system chrome and traffic lights come from tiaki-chrome;
    // the only in-app header is the breadcrumb row, whose back button appears
    // on subpages exactly as Windows 11 does it.
    let window = theme::base_window("settings-window");
    window.set_title("Settings");
    // A normal app window: resizable, and explicitly sized. `base_window`
    // turns resizing off, which would make GTK adopt the sidebar's natural
    // height (all 17 pages counted before the hidden ones collapse).
    window.set_resizable(true);
    window.set_default_size(1180, 780);
    window.resize(1180, 780);

    let panel = gtk::EventBox::new();
    panel.set_visible_window(true);
    theme::class(&panel, "settings-panel");

    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add(&shell);

    // The tiaki-chrome titlebar is an overlay that sits on the top ~30 px
    // of every app window. Reserve that space so no Settings content hides
    // underneath it. (Terminals get away with it through internal padding;
    // Settings has rows at y=0, so it spacers explicitly.)
    let chrome_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    chrome_spacer.set_size_request(-1, 34);
    shell.pack_start(&chrome_spacer, false, false, 0);

    // ------------------------------------------------------------- body
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);

    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar.set_size_request(332, -1);
    theme::class(&sidebar, "settings-sidebar");

    let profile = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    profile.set_margin_top(18);
    profile.set_margin_bottom(20);
    profile.set_margin_start(20);
    profile.set_margin_end(20);
    profile.pack_start(&avatar(48), false, false, 0);

    let who = gtk::Box::new(gtk::Orientation::Vertical, 2);
    who.set_valign(gtk::Align::Center);
    let name = gtk::Label::new(Some("Ada"));
    name.set_xalign(0.0);
    theme::class(&name, "settings-profile-name");
    let principal = gtk::Label::new(Some("user:ada"));
    principal.set_xalign(0.0);
    theme::class(&principal, "settings-profile-sub");
    who.pack_start(&name, false, false, 0);
    who.pack_start(&principal, false, false, 0);
    profile.pack_start(&who, true, true, 0);
    sidebar.pack_start(&profile, false, false, 0);

    let search_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    search_box.set_margin_start(20);
    search_box.set_margin_end(20);
    search_box.set_margin_bottom(16);
    theme::class(&search_box, "settings-search");
    let search = gtk::Entry::new();
    search.set_has_frame(false);
    search.set_placeholder_text(Some("Find a setting"));
    search.set_valign(gtk::Align::Center);
    theme::class(&search, "settings-search-entry");
    search_box.pack_start(&search, true, true, 0);
    search_box.pack_end(&glyphs::status_icon_tinted(glyphs::settings_icon("search"), 15, (0.56, 0.54, 0.51)), false, false, 0);
    sidebar.pack_start(&search_box, false, false, 0);

    let nav_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    nav_box.set_margin_start(12);
    nav_box.set_margin_end(12);
    theme::class(&nav_box, "settings-navbox");
    sidebar.pack_start(&nav_box, true, true, 0);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_hexpand(true);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.set_margin_top(24);
    header.set_margin_start(32);
    header.set_margin_end(40);
    header.set_margin_bottom(16);
    // The back arrow sits in the content area, left of the breadcrumb.
    // It is only enabled on subpages (Display, Sound, ...).
    let back = gtk::Button::new();
    back.set_relief(gtk::ReliefStyle::None);
    back.set_size_request(36, 36);
    back.set_valign(gtk::Align::Center);
    back.set_tooltip_text(Some("Back"));
    back.set_sensitive(false);
    theme::class(&back, "settings-back");
    back.add(&glyphs::status_icon_tinted(glyphs::settings_icon("back"), 15, (0.902, 0.878, 0.831)));
    header.pack_start(&back, false, false, 0);
    let crumb = gtk::Label::new(None);
    crumb.set_xalign(0.0);
    crumb.set_valign(gtk::Align::Center);
    theme::class(&crumb, "settings-title");
    header.pack_start(&crumb, true, true, 0);
    content.pack_start(&header, false, false, 0);

    let scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    theme::class(&scroll, "settings-scroll");
    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(120);
    scroll.add(&stack);
    content.pack_start(&scroll, true, true, 0);

    let buttons: Rc<RefCell<Vec<(String, gtk::Button)>>> = Rc::new(RefCell::new(Vec::new()));
    let nav = Nav {
        stack: stack.clone(),
        crumb: crumb.clone(),
        back: back.clone(),
        pages: Rc::new(pages.clone()),
        buttons: buttons.clone(),
        current: Rc::new(RefCell::new(String::new())),
    };

    // Pages are built once, then the sidebar is built from the same list.
    for page in &pages {
        let widget = page_content(&page.action, root, &nav);
        stack.add_named(&widget, &page.action);
    }

    for page in pages.iter() {
        let button = gtk::Button::new();
        button.set_relief(gtk::ReliefStyle::None);
        button.set_size_request(-1, 44);
        theme::class(&button, "settings-nav");

        let line = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        line.set_margin_start(14);
        line.set_margin_end(12);
        line.pack_start(&glyphs::status_icon_tinted(glyphs::settings_icon(&page.icon), 20, (0.902, 0.878, 0.831)), false, false, 0);
        let label = gtk::Label::new(Some(&page.label));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        theme::class(&label, "settings-nav-label");
        line.pack_start(&label, true, true, 0);
        button.add(&line);

        let nav_clone = nav.clone();
        let action = page.action.clone();
        button.connect_clicked(move |_| nav_clone.go(&action));
        nav_box.pack_start(&button, false, false, 0);
        buttons.borrow_mut().push((page.action.clone(), button));
    }

    // Search filters the sidebar to matching pages.
    let buttons_for_search = buttons.clone();
    let pages_for_search = nav.pages.clone();
    search.connect_changed(move |entry| {
        let query = entry.text().to_lowercase();
        for (action, button) in buttons_for_search.borrow().iter() {
            let Some(page) = pages_for_search.iter().find(|page| page.action == *action) else {
                continue;
            };
            let matches = page.label.to_lowercase().contains(&query);
            // Subpages stay out of the sidebar until a search names them.
            let visible = if query.is_empty() {
                !page.hidden
            } else {
                matches
            };
            if visible {
                button.show_all();
            } else {
                button.hide();
            }
        }
    });

    body.pack_start(&sidebar, false, false, 0);
    body.pack_start(&content, true, true, 0);
    shell.pack_start(&body, true, true, 0);
    window.add(&panel);

    // --------------------------------------------------------- behaviors
    let nav_for_back = nav.clone();
    back.connect_clicked(move |_| nav_for_back.back());

    let window_for_key = window.clone();
    window.connect_key_press_event(move |_, event| {
        if event.keyval() == gdk::keys::constants::Escape {
            window_for_key.close();
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });

    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    // Subpages are built and styled like any other page, but stay out of the
    // sidebar until the setting search names them.
    for (action, button) in buttons.borrow().iter() {
        if nav.page(action).map(|page| page.hidden).unwrap_or(false) {
            button.hide();
        }
    }
    // Re-assert the size after the hidden sidebar pages collapse, so the
    // window never opens at the taller pre-hide natural height.
    window.resize(1180, 780);
    window.present();
    match start_page {
        Some(page) if nav.page(page).is_some() => nav.go(page),
        _ => nav.go(&first.action),
    }
    gtk::main();
}

// --------------------------------------------------------------- the pages

#[derive(Clone)]
pub(crate) struct Page {
    label: String,
    action: String,
    icon: String,
    parent: Option<String>,
    hidden: bool,
}

#[derive(Clone)]
pub(crate) struct Nav {
    stack: gtk::Stack,
    crumb: gtk::Label,
    back: gtk::Button,
    pages: Rc<Vec<Page>>,
    buttons: Rc<RefCell<Vec<(String, gtk::Button)>>>,
    current: Rc<RefCell<String>>,
}

impl Nav {
    fn page(&self, action: &str) -> Option<&Page> {
        self.pages.iter().find(|page| page.action == action)
    }

    pub(crate) fn go(&self, action: &str) {
        let Some(page) = self.page(action) else {
            return;
        };
        self.stack.set_visible_child_name(action);
        for (candidate, button) in self.buttons.borrow().iter() {
            if candidate == action {
                button.style_context().add_class("active");
            } else {
                button.style_context().remove_class("active");
            }
        }
        self.crumb.set_markup(&crumb_markup(page));
        self.back.set_sensitive(page.parent.is_some());
        *self.current.borrow_mut() = action.to_string();
    }

    pub(crate) fn back(&self) {
        let current = self.current.borrow().clone();
        if let Some(parent) = self.page(&current).and_then(|page| page.parent.clone()) {
            self.go(&parent);
        }
    }
}

fn crumb_markup(page: &Page) -> String {
    let label = gtk::glib::markup_escape_text(&page.label);
    match &page.parent {
        Some(parent) => format!(
            "<span foreground=\"#a39c8d\" weight=\"normal\">{}</span><span foreground=\"#665f54\"> › </span><span foreground=\"#e6e0d4\" weight=\"600\">{}</span>",
            gtk::glib::markup_escape_text(parent),
            label
        ),
        None => format!("<span foreground=\"#e6e0d4\" weight=\"600\">{label}</span>"),
    }
}

fn pages_from(root: &Node) -> Vec<Page> {
    if let Some(panel) = model::find_by_id(root, "settings") {
        let pages: Vec<Page> = panel
            .children
            .iter()
            .filter(|child| child.kind == "button")
            .map(|child| {
                let action = child.prop_str("action").unwrap_or("noop").to_string();
                let parent = child
                    .prop_str("parent")
                    .map(str::to_string)
                    .or_else(|| default_parent(&action));
                let hidden = child.prop_bool("hidden").unwrap_or(parent.is_some());
                Page {
                    label: child.prop_str("label").unwrap_or("Page").to_string(),
                    action,
                    icon: child.prop_str("icon").unwrap_or("gear").to_string(),
                    parent,
                    hidden,
                }
            })
            .collect();
        if !pages.is_empty() {
            return pages;
        }
    }
    fallback_pages()
}

fn default_parent(action: &str) -> Option<String> {
    match action {
        "settings.page.display"
        | "settings.page.sound"
        | "settings.page.power"
        | "settings.page.storage"
        | "settings.page.about" => Some("System".to_string()),
        _ => None,
    }
}

fn fallback_pages() -> Vec<Page> {
    let page = |label: &str, action: &str, icon: &str, parent: Option<&str>, hidden: bool| Page {
        label: label.to_string(),
        action: action.to_string(),
        icon: icon.to_string(),
        parent: parent.map(str::to_string),
        hidden,
    };
    vec![
        page("Home", "settings.page.home", "home", None, false),
        page("System", "settings.page.system", "system", None, false),
        page(
            "Bluetooth & devices",
            "settings.page.bluetooth",
            "bluetooth",
            None,
            false,
        ),
        page(
            "Network & internet",
            "settings.page.network",
            "network",
            None,
            false,
        ),
        page(
            "Personalization",
            "settings.page.personalization",
            "personalization",
            None,
            false,
        ),
        page("Apps", "settings.page.apps", "apps", None, false),
        page(
            "Accounts",
            "settings.page.accounts",
            "accounts",
            None,
            false,
        ),
        page("Time & language", "settings.page.time", "time", None, false),
        page("Gaming", "settings.page.gaming", "gaming", None, false),
        page(
            "Accessibility",
            "settings.page.accessibility",
            "accessibility",
            None,
            false,
        ),
        page(
            "Privacy & security",
            "settings.page.privacy",
            "privacy",
            None,
            false,
        ),
        page(
            "System update",
            "settings.page.update",
            "update",
            None,
            false,
        ),
        page(
            "Display",
            "settings.page.display",
            "system",
            Some("System"),
            true,
        ),
        page(
            "Sound",
            "settings.page.sound",
            "system",
            Some("System"),
            true,
        ),
        page(
            "Power",
            "settings.page.power",
            "system",
            Some("System"),
            true,
        ),
        page(
            "Storage",
            "settings.page.storage",
            "system",
            Some("System"),
            true,
        ),
        page(
            "About",
            "settings.page.about",
            "system",
            Some("System"),
            true,
        ),
    ]
}

fn page_content(action: &str, root: &Node, nav: &Nav) -> gtk::Widget {
    match action {
        "settings.page.home" => home::home_page(root, nav).upcast(),
        "settings.page.bluetooth" => render::render_content(root, nav, action)
            .unwrap_or_else(bluetooth::bluetooth_page)
            .upcast(),
        "settings.page.network" => render::render_content(root, nav, action)
            .unwrap_or_else(network::network_page)
            .upcast(),
        "settings.page.personalization" => {
            personalization::personalization_page(root, nav).upcast()
        }
        "settings.page.apps" => apps::apps_page(root).upcast(),
        "settings.page.accounts" => accounts::accounts_page().upcast(),
        "settings.page.time" => render::render_content(root, nav, action)
            .unwrap_or_else(|| time::time_page(root))
            .upcast(),
        "settings.page.system" => render::render_content(root, nav, action)
            .unwrap_or_else(|| system::system_page(nav))
            .upcast(),
        "settings.page.about" => render::render_content(root, nav, action)
            .unwrap_or_else(|| about::about_page())
            .upcast(),
        "settings.page.gaming" => render::render_content(root, nav, action)
            .unwrap_or_else(|| gaming::gaming_page())
            .upcast(),
        "settings.page.accessibility" => accessibility::accessibility_page().upcast(),
        "settings.page.privacy" => privacy::privacy_page().upcast(),
        "settings.page.update" => render::render_content(root, nav, action)
            .unwrap_or_else(update::update_page)
            .upcast(),
        "settings.page.display" => display::display_page().upcast(),
        "settings.page.sound" => sound::sound_page().upcast(),
        "settings.page.power" => power::power_page().upcast(),
        "settings.page.storage" => storage::storage_page().upcast(),
        _ => home::home_page(root, nav).upcast(),
    }
}

// -------------------------------------------------------------- primitives
