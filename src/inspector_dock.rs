//! Host the native WebKit inspector beside its tab instead of below the page.
use gtk::prelude::*;
use webkit::prelude::*;

pub fn install(view: &webkit::WebView, split: &gtk::Paned) {
    let Some(inspector) = view.inspector() else {
        return;
    };
    // Weak references avoid a cycle through the tab's WebView and inspector.
    let weak = split.downgrade();
    inspector.connect_attach(move |inspector| {
        let (Some(split), Some(frontend)) = (weak.upgrade(), inspector.web_view()) else {
            return false;
        };
        if let Some(parent) = frontend.parent() {
            return parent == split;
        }
        frontend.set_hexpand(true);
        frontend.set_vexpand(true);
        split.set_end_child(Some(&frontend));
        // Start around 40% of the available width; the divider remains movable.
        let width = split.width().max(1);
        let tools_width = (width * 2 / 5).clamp(320, 600).min(width / 2);
        split.set_position(width - tools_width);
        frontend.set_visible(true);
        true
    });

    let weak = split.downgrade();
    inspector.connect_detach(move |_| {
        let Some(split) = weak.upgrade() else {
            return false;
        };
        // WebKit keeps the frontend alive while detaching, and creates its own
        // separate window when the user explicitly requests an undocked view.
        split.set_end_child(gtk::Widget::NONE);
        true
    });

    let weak = split.downgrade();
    inspector.connect_closed(move |_| {
        if let Some(split) = weak.upgrade() {
            split.set_end_child(gtk::Widget::NONE);
        }
    });
}
