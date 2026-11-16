//! User-local desktop integration, available even from a Cargo-installed binary.
use gtk::prelude::*;
use std::{env, fs, io, path::PathBuf};

pub const DESKTOP_ID: &str = "site.blig.Paisley";
pub const ICON_NAME: &str = DESKTOP_ID;
const ICON: &[u8] = include_bytes!("../assets/branding/paisley-logo-stand.png");
const ENTRY: &str = include_str!("../packaging/site.blig.Paisley.desktop");

/// GApplication IDs remain profile-specific for D-Bus routing. The compositor
/// needs the installed desktop-file ID instead, for every normal/private window.
pub fn configure_application(app: &gtk::Application) {
    app.connect_window_added(|_, window| {
        window.set_icon_name(Some(ICON_NAME));
        // GDK ignores set_application_id before the native Wayland toplevel
        // exists. GtkWindow's default map handler creates it; run afterwards,
        // before the first rendered buffer, and again on every remap.
        window.connect_map(apply_window_identity);
        if window.is_mapped() {
            apply_window_identity(window);
        }
    });
}

fn apply_window_identity(window: &gtk::Window) {
    #[cfg(target_os = "linux")]
    if let Some(toplevel) = window
        .surface()
        .and_then(|surface| surface.downcast::<gdk_wayland::WaylandToplevel>().ok())
    {
        toplevel.set_application_id(DESKTOP_ID);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = window;
}

fn launcher_executable(executable: PathBuf) -> io::Result<PathBuf> {
    if executable
        .file_name()
        .is_some_and(|name| name == "paisley-engine")
    {
        let launcher = executable.with_file_name("paisley");
        if !launcher.is_file() {
            return Err(io::Error::other(
                "Install both paisley and paisley-engine before registering the desktop launcher",
            ));
        }
        Ok(launcher)
    } else {
        // Integration fixtures and older single-binary installs keep their name.
        Ok(executable)
    }
}

pub fn install() -> io::Result<()> {
    let data = env::var_os("XDG_DATA_HOME")
        .filter(|value| PathBuf::from(value).is_absolute())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| io::Error::other("HOME or an absolute XDG_DATA_HOME is required"))?;
    let executable = launcher_executable(env::current_exe()?)?;
    let executable = executable
        .to_str()
        .ok_or_else(|| io::Error::other("Executable path must be valid UTF-8"))?;
    if executable.contains(['\n', '\r']) {
        return Err(io::Error::other("Executable path contains a line break"));
    }
    // Quote according to Desktop Entry Exec rules; percent is a field-code escape.
    let quoted = executable
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%");
    let entry = ENTRY.replace("Exec=paisley %U\n", &format!("Exec=\"{quoted}\" %U\n"));
    let applications = data.join("applications");
    let icons = data.join("icons/hicolor/256x256/apps");
    fs::create_dir_all(&applications)?;
    fs::create_dir_all(&icons)?;
    fs::write(icons.join(format!("{ICON_NAME}.png")), ICON)?;
    fs::write(applications.join(format!("{DESKTOP_ID}.desktop")), entry)?;
    // Optional cache tools: launchers can also discover files without these.
    let _ = std::process::Command::new("gtk-update-icon-cache")
        .args(["-f", "-t"])
        .arg(data.join("icons/hicolor"))
        .output();
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&applications)
        .output();
    println!("Installed Paisley launcher and logo in {}", data.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_template_matches_window_and_icon_identity() {
        assert_eq!(DESKTOP_ID, "site.blig.Paisley");
        assert!(
            ENTRY
                .lines()
                .any(|line| line == format!("Icon={ICON_NAME}"))
        );
        assert!(ENTRY.lines().any(|line| line == "Exec=paisley %U"));
        // Profile routing is handled by the CLI, not a shared D-Bus activator.
        assert!(!ENTRY.lines().any(|line| line == "DBusActivatable=true"));
        assert!(ICON.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
