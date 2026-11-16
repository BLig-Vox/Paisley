//! Application actions in a docked right sidebar.
use gtk::prelude::*;
use std::{cell::RefCell, rc::Rc};

pub struct MenuSheet {
    parent: gtk::ApplicationWindow,
    dock: gtk::Box,
    sheet: gtk::Box,
    secondary_sheet: gtk::Box,
    close: gtk::Button,
    back: gtk::Button,
    sections: Vec<(String, gtk::Button)>,
    title: gtk::Label,
    stack: gtk::Stack,
    secondary: RefCell<Option<gtk::Widget>>,
    retained: RefCell<Option<Box<dyn std::any::Any>>>,
    previous_focus: RefCell<Option<gtk::glib::WeakRef<gtk::Widget>>>,
}
impl MenuSheet {
    pub fn new(parent: &gtk::ApplicationWindow) -> Rc<Self> {
        // Downloads installs the common horizontal content layout first.
        let dock = parent
            .child()
            .and_downcast::<gtk::Box>()
            .expect("sidebar layout");
        let sheet = gtk::Box::new(gtk::Orientation::Vertical, 12);
        sheet.add_css_class("downloads-sheet");
        sheet.add_css_class("menu-sheet");
        sheet.set_widget_name("paisley-menu-sheet");
        sheet.set_width_request(300);
        sheet.set_hexpand(false);
        sheet.set_vexpand(true);
        sheet.set_visible(false);
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let back = gtk::Button::from_icon_name("lucide-x-symbolic");
        back.add_css_class("flat");
        back.set_tooltip_text(Some("Close section (Esc)"));
        back.set_visible(false);
        let title = crate::download_ui::label("", "downloads-heading");
        heading.append(&crate::download_ui::label("Paisley", "downloads-heading"));
        let close = gtk::Button::from_icon_name("lucide-x-symbolic");
        close.add_css_class("flat");
        close.set_tooltip_text(Some("Close menu (Esc)"));
        heading.append(&close);
        sheet.append(&heading);
        sheet.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let actions = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let mut sections = Vec::new();
        for (index, (icon, title, shortcut, action)) in [
            ("settings", "Settings", "", "settings"),
            ("users", "Profiles", "", "profiles"),
            ("shield", "Site protection", "", "site-blocking"),
            (
                "bookmark-plus",
                "Bookmark this page",
                "Ctrl+D",
                "bookmark-current",
            ),
            ("bookmark", "Bookmarks", "", "bookmarks"),
            ("rotate-ccw-clock", "History", "Ctrl+H", "history"),
            ("download", "Downloads", "Ctrl+J", "downloads"),
            ("code-xml", "Developer tools", "F12", "inspector"),
        ]
        .into_iter()
        .enumerate()
        {
            if index == 0 || index == 2 || index == 7 {
                let section = crate::download_ui::label(
                    match index {
                        0 => "PREFERENCES",
                        2 => "BROWSING",
                        _ => "TOOLS",
                    },
                    "menu-section",
                );
                actions.append(&section);
            }
            if index == 2 || index == 7 {
                let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
                separator.set_margin_top(8);
                separator.set_margin_bottom(8);
                actions.append(&separator);
            }
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.set_margin_top(8);
            row.set_margin_bottom(8);
            let icon = gtk::Image::from_icon_name(&format!("lucide-{icon}-symbolic"));
            icon.set_pixel_size(18);
            row.append(&icon);
            row.append(&crate::download_ui::label(title, "menu-sheet-label"));
            let key = gtk::Label::new(Some(shortcut));
            key.add_css_class("downloads-detail");
            row.append(&key);
            if matches!(
                action,
                "settings"
                    | "profiles"
                    | "site-blocking"
                    | "bookmark-current"
                    | "bookmarks"
                    | "history"
                    | "downloads"
            ) {
                let arrow = gtk::Image::from_icon_name("lucide-chevron-right-symbolic");
                arrow.set_pixel_size(14);
                arrow.add_css_class("dim-label");
                row.append(&arrow);
            }
            let button = gtk::Button::builder()
                .child(&row)
                .action_name(format!("win.{action}"))
                .build();
            button.add_css_class("flat");
            button.add_css_class("menu-row");
            sections.push((title.to_owned(), button.clone()));
            actions.append(&button);
        }
        let scroller = gtk::ScrolledWindow::builder()
            .child(&actions)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let stack = gtk::Stack::builder()
            .vexpand(true)
            .hexpand(true)
            .hhomogeneous(false)
            .vhomogeneous(false)
            .transition_type(gtk::StackTransitionType::SlideLeftRight)
            .transition_duration(160)
            .build();
        sheet.append(&scroller);
        let secondary_sheet = gtk::Box::new(gtk::Orientation::Vertical, 12);
        secondary_sheet.add_css_class("downloads-sheet");
        secondary_sheet.add_css_class("secondary-sheet");
        secondary_sheet.set_width_request(420);
        secondary_sheet.set_vexpand(true);
        secondary_sheet.set_hexpand(false);
        secondary_sheet.set_visible(false);
        let secondary_heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        secondary_heading.append(&title);
        secondary_heading.append(&back);
        secondary_sheet.append(&secondary_heading);
        secondary_sheet.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        secondary_sheet.append(&stack);
        dock.append(&secondary_sheet);
        dock.append(&sheet);
        let this = Rc::new(Self {
            parent: parent.clone(),
            dock,
            sheet,
            secondary_sheet,
            close,
            back,
            sections,
            title,
            stack,
            secondary: RefCell::new(None),
            retained: RefCell::new(None),
            previous_focus: RefCell::new(None),
        });
        let weak = Rc::downgrade(&this);
        this.close.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.dismiss();
            }
        });
        let weak = Rc::downgrade(&this);
        this.back.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.back();
            }
        });
        for surface in [&this.sheet, &this.secondary_sheet] {
            let keys = gtk::EventControllerKey::new();
            keys.set_propagation_phase(gtk::PropagationPhase::Capture);
            let weak = Rc::downgrade(&this);
            keys.connect_key_pressed(move |_, key, _, _| {
                if key == gtk::gdk::Key::Escape
                    && let Some(this) = weak.upgrade()
                    && this.escape()
                {
                    return gtk::glib::Propagation::Stop;
                }
                gtk::glib::Propagation::Proceed
            });
            surface.add_controller(keys);
        }
        this
    }
    pub fn toggle(&self) {
        if self.dismiss() {
            return;
        }
        *self.previous_focus.borrow_mut() =
            gtk::prelude::GtkWindowExt::focus(&self.parent).map(|widget| widget.downgrade());
        self.sheet.set_visible(true);
        self.close.grab_focus();
    }
    pub fn show_page(&self, title: &str, child: &impl IsA<gtk::Widget>, keep: impl std::any::Any) {
        if !self.sheet.is_visible() {
            *self.previous_focus.borrow_mut() =
                gtk::prelude::GtkWindowExt::focus(&self.parent).map(|widget| widget.downgrade());
        }
        self.back();

        self.title.set_text(title);
        self.select(title);
        self.back.set_visible(true);
        self.stack.add_named(child, Some("secondary"));
        *self.secondary.borrow_mut() = Some(child.clone().upcast());
        *self.retained.borrow_mut() = Some(Box::new(keep));
        self.stack.set_visible_child_name("secondary");
        self.sheet.set_visible(true);
        self.secondary_sheet.set_visible(true);
        self.back.grab_focus();
    }
    pub fn back(&self) {
        self.secondary_sheet.set_visible(false);
        if let Some(child) = self.secondary.borrow_mut().take() {
            self.stack.remove(&child);
        }
        self.retained.borrow_mut().take();
        self.title.set_text("");
        self.select("");
        self.back.set_visible(false);

        if self.sheet.is_visible() {
            self.close.grab_focus();
        }
    }
    pub fn select(&self, title: &str) {
        for (section, button) in &self.sections {
            if section == title {
                button.add_css_class("selected-section");
            } else {
                button.remove_css_class("selected-section");
            }
        }
    }
    pub fn escape(&self) -> bool {
        if self.secondary.borrow().is_some() {
            self.back();
            true
        } else {
            self.dismiss()
        }
    }
    pub fn dismiss(&self) -> bool {
        if !self.sheet.is_visible() {
            return false;
        }
        self.sheet.set_visible(false);
        self.back();
        if let Some(focus) = self
            .previous_focus
            .borrow_mut()
            .take()
            .and_then(|weak| weak.upgrade())
        {
            focus.grab_focus();
        }
        true
    }
}
impl Drop for MenuSheet {
    fn drop(&mut self) {
        if self.sheet.parent().is_some() {
            self.dock.remove(&self.sheet);
        }
        if self.secondary_sheet.parent().is_some() {
            self.dock.remove(&self.secondary_sheet);
        }
    }
}
