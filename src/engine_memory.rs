//! WebKit-native cache reclamation; never discard live tabs to fake a lower total.

pub fn pressure_settings() -> webkit::MemoryPressureSettings {
    let mut settings = webkit::MemoryPressureSettings::new();
    // Soft policy reference per engine process, not an app-wide allocation cap.
    // Conservative cleanup starts at 128 MiB; strict cleanup at 256 MiB.
    settings.set_memory_limit(512);
    settings.set_conservative_threshold(0.25);
    settings.set_strict_threshold(0.5);
    settings.set_poll_interval(10.0);
    // Retain the default: this policy never kills a page at a fixed threshold.
    settings.set_kill_threshold(0.0);
    settings
}

pub fn context() -> webkit::WebContext {
    let context = webkit::WebContext::builder()
        .memory_pressure_settings(&pressure_settings())
        .build();
    // Keep useful browsing caches, with less retained content than WebBrowser.
    context.set_cache_model(webkit::CacheModel::DocumentBrowser);
    context
}

pub fn configure_network() {
    // Static WebKit API: configure once, before any NetworkSession exists.
    webkit::NetworkSession::set_memory_pressure_settings(&mut pressure_settings());
}
