//! Focused native identity regression; run the ignored case in nested Wayland.
use gtk::prelude::*;
#[path = "../src/desktop_install.rs"]
mod desktop_install;
use desktop_install::{ICON_NAME, configure_application};

#[test]
#[ignore = "Run via scripts/test-wayland-identity.py in its isolated Wayland session"]
fn wayland_recreated_surface() {
    gtk::init().expect("Wayland display");
    assert!(
        gtk::gdk::Display::default()
            .unwrap()
            .is::<gdk_wayland::WaylandDisplay>()
    );
    let id = "site.blig.Paisley.IdentityRemapTest";
    let app = gtk::Application::new(Some(id), gtk::gio::ApplicationFlags::NON_UNIQUE);
    app.register(None::<&gtk::gio::Cancellable>).unwrap();
    configure_application(&app);
    let window = gtk::ApplicationWindow::builder()
        .application(&app)
        .title("Identity remap regression")
        .build();
    let context = gtk::glib::MainContext::default();
    let settle = || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !window.is_mapped() && std::time::Instant::now() < deadline {
            context.iteration(false);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(window.is_mapped());
        gtk::prelude::WidgetExt::display(&window).flush();
    };
    window.present();
    settle();
    let old_surface = window.surface().unwrap();
    window.set_visible(false);
    gtk::prelude::WidgetExt::unrealize(&window);
    window.present();
    settle();
    assert_ne!(old_surface, window.surface().unwrap());
    assert_eq!(app.application_id().as_deref(), Some(id));
    assert_eq!(window.icon_name().as_deref(), Some(ICON_NAME));
    window.close();
    gtk::prelude::WidgetExt::display(&window).flush();
}
