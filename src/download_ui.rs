//! Presentation helpers for the download sheet; no file contents are inspected.
use gtk::prelude::*;
use std::path::Path;

pub fn icon_name(filename: &str) -> &'static str {
    let extension = Path::new(filename)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "avif" | "bmp" | "ico" => {
            "lucide-image-symbolic"
        }
        "mp4" | "webm" | "mkv" | "mov" | "avi" | "m4v" => "lucide-video-symbolic",
        "mp3" | "wav" | "flac" | "ogg" | "opus" | "m4a" | "aac" => "lucide-music-symbolic",
        "zip" | "gz" | "xz" | "zst" | "bz2" | "tar" | "7z" | "rar" => "lucide-archive-symbolic",
        "pdf" | "txt" | "md" | "doc" | "docx" | "odt" | "rtf" | "csv" | "xls" | "xlsx" => {
            "lucide-file-text-symbolic"
        }
        "rs" | "py" | "js" | "ts" | "html" | "css" | "json" | "toml" | "yaml" | "yml" | "sh" => {
            "lucide-file-code-symbolic"
        }
        "deb" | "rpm" | "appimage" | "flatpakref" | "exe" | "msi" | "apk" => {
            "lucide-package-symbolic"
        }
        _ => "lucide-file-symbolic",
    }
}

pub fn icon(filename: &str) -> gtk::Image {
    gtk::Image::builder()
        .icon_name(icon_name(filename))
        .pixel_size(28)
        .valign(gtk::Align::Start)
        .margin_top(4)
        .build()
}

pub fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .build();
    label.add_css_class(class);
    label
}

pub fn date(timestamp: u64) -> String {
    i64::try_from(timestamp)
        .ok()
        .and_then(|time| gtk::glib::DateTime::from_unix_local(time).ok())
        .and_then(|date| date.format("%b %e, %Y · %I:%M %p").ok())
        .map(|text| text.to_string())
        .unwrap_or_else(|| "Date unavailable".into())
}

pub fn source(uri: Option<&str>) -> String {
    uri.and_then(|uri| url::Url::parse(uri).ok())
        .and_then(|uri| uri.host_str().map(str::to_owned))
        .unwrap_or_else(|| "Source unavailable".into())
}

pub fn install_styles(display: &gtk::gdk::Display) {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("../assets/downloads.css"));
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

pub fn cell(text: &str, width: i32) -> gtk::Label {
    let value = label(text, "downloads-detail");
    value.set_hexpand(false);
    value.set_width_request(width);
    value.set_valign(gtk::Align::Center);
    value.set_tooltip_text(Some(text));
    value
}

/// Status messages should occupy space only when they have content.
pub fn hide_when_empty(label: &gtk::Label) {
    label.set_visible(!label.text().is_empty());
    label.connect_notify_local(Some("label"), |label, _| {
        label.set_visible(!label.text().is_empty());
    });
}
