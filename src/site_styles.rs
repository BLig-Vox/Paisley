/// Site-specific presentation only: no extension spoofing, request blocking,
/// storage changes, or changes to JavaScript/TLS security settings.
pub fn content_manager() -> webkit::UserContentManager {
    let manager = webkit::UserContentManager::new();
    let stylesheet = webkit::UserStyleSheet::new(
        include_str!("../assets/duckduckgo.css"),
        webkit::UserContentInjectedFrames::TopFrame,
        webkit::UserStyleLevel::User,
        &["https://duckduckgo.com/*", "https://*.duckduckgo.com/*"],
        &[],
    );
    manager.add_style_sheet(&stylesheet);
    manager
}
