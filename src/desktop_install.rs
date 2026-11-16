//! User-local desktop integration, available even from a Cargo-installed binary.
use std::{env, fs, io, path::PathBuf};

pub const ICON_NAME: &str = "site.blig.Paisley";
const ICON: &[u8] = include_bytes!("../assets/branding/paisley-logo-stand.png");
const ENTRY: &str = include_str!("../packaging/site.blig.Paisley.desktop");

pub fn install() -> io::Result<()> {
    let data = env::var_os("XDG_DATA_HOME")
        .filter(|value| PathBuf::from(value).is_absolute())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or_else(|| io::Error::other("HOME or an absolute XDG_DATA_HOME is required"))?;
    let executable = env::current_exe()?;
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
    fs::write(icons.join("site.blig.Paisley.png"), ICON)?;
    fs::write(applications.join("site.blig.Paisley.desktop"), entry)?;
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
