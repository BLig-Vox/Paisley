mod automation_runtime;
mod blocking;
mod browser;
mod browsing_ui;
mod desktop_notifications;
mod desktop_install;
mod download_ui;
mod downloads;
mod favicons;
mod inspector_dock;
mod library_ui;
mod managed_ui;
mod menu_sheet;
mod notifications;
mod session_store;
mod settings_ui;
mod site_info;
mod site_styles;

use gtk::prelude::*;

fn main() -> gtk::glib::ExitCode {
    if std::env::args().any(|arg| arg == "--mcp") {
        return paisley::mcp::run_stdio();
    }
    if std::env::args().any(|arg| arg == "--automation") {
        return automation_runtime::run();
    }
    let mut profile_id = "default".to_string();
    let mut private = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--profile" => match args.next() {
                Some(id) => profile_id = id,
                None => {
                    eprintln!("--profile requires an ID");
                    return gtk::glib::ExitCode::FAILURE;
                }
            },
            "--private" => private = true,
            "--install-desktop" if std::env::args().len() == 2 => {
                return match desktop_install::install() {
                    Ok(()) => gtk::glib::ExitCode::SUCCESS,
                    Err(error) => {
                        eprintln!("Could not install desktop launcher: {error}");
                        gtk::glib::ExitCode::FAILURE
                    }
                };
            }
            "--help" | "-h" => {
                println!(
                    "Paisley\nUsage: paisley [--profile ID] [--private]\n       paisley --install-desktop\n       paisley --mcp --allow-origin http://127.0.0.1:PORT\nNormal profiles persist site data and restore tabs. Private windows do not.\nMCP uses a separate isolated ephemeral fixture workspace; requires Wayland, Python 3, bubblewrap, dbus-run-session and WebKitWebDriver."
                );
                return gtk::glib::ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("Unknown option: {arg}");
                return gtk::glib::ExitCode::FAILURE;
            }
        }
    }
    if profile_id.is_empty()
        || profile_id.len() > 64
        || !profile_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        eprintln!("Profile IDs must be 1–64 ASCII letters, digits, underscores or hyphens");
        return gtk::glib::ExitCode::FAILURE;
    }
    let encoded: String = profile_id.bytes().map(|b| format!("{b:02x}")).collect();
    let id = format!(
        "site.blig.Paisley.p{encoded}{}",
        if private { ".Private" } else { "" }
    );
    let app = gtk::Application::builder()
        .application_id(&id)
        .flags(if private {
            gtk::gio::ApplicationFlags::NON_UNIQUE
        } else {
            gtk::gio::ApplicationFlags::FLAGS_NONE
        })
        .build();
    let failed = std::rc::Rc::new(std::cell::Cell::new(false));
    let output = std::rc::Rc::clone(&failed);
    app.connect_activate(move |app| {
        if let Some(window) = app.active_window() { window.present(); return; }
        let display = gtk::gdk::Display::default().expect("GTK display is available after activation");
        if let Err(error) = fonts_rs_lucide::Lucide::install_icon_theme(&display) {
            output.set(true);
            eprintln!("Could not register bundled Lucide icons: {error}");
            return;
        }
        gtk::Window::set_default_icon_name(desktop_install::ICON_NAME);
        crate::download_ui::install_styles(&display);
        match paisley::profile::Profile::open(&profile_id, private).and_then(|profile| browser::Browser::with_profile(app, profile)) {
            Ok(browser) => browser.present(),
            Err(error) => {
                output.set(true);
                eprintln!("Could not open profile: {error}");
                let label = gtk::Label::builder().label(format!("Could not open profile {profile_id}\n\n{error}\n\nExisting state has not been reset. Close other instances or repair the reported file before retrying.")).wrap(true).selectable(true).margin_start(24).margin_end(24).margin_top(24).margin_bottom(24).build();
                gtk::ApplicationWindow::builder().application(app).title("Paisley — profile error").default_width(600).child(&label).build().present();
            }
        }
    });
    let result = app.run_with_args(&["paisley"]);
    if failed.get() {
        gtk::glib::ExitCode::FAILURE
    } else {
        result
    }
}
