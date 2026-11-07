# Browny multi-tab browser

## Intent and scope

Replace the Iced greeting with a Linux-first Rust browser using GTK4 and WebKitGTK, as selected by the user. The user explicitly requested multi-tab support and immediate implementation. Keep the default window at 1280 × 800.

## Approaches

Iced with an embedded native webview requires unnecessary event-loop/window integration. Wry uses different engines on different operating systems. Direct GTK4 plus WebKitGTK is the selected foundation: Rust owns the browser interface; WebKit owns rendering and JavaScript. Use the GTK4-compatible WebKitGTK 6.0 API through the `webkit6` crate, not GTK3 bindings.

## Initial behavior

- Open on a local Hello, world! welcome page without external requests.
- Add, switch, and close tabs; each has its own WebView, title, loading indicator, and history. Closing the last tab opens a new welcome tab.
- A shared address entry and Back, Forward, Reload/Stop, Home, and New tab controls act only on the active tab. Background loads must not overwrite the active toolbar.
- Accept HTTP/HTTPS URLs, default public hostnames to HTTPS, and use HTTP for loopback development addresses. Plain search text goes to DuckDuckGo only on submission. Reject executable/local-file schemes in the address entry.
- Support Ctrl+L, Ctrl+T, Ctrl+W, Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+R/F5, Alt+Left/Right, and Escape.
- Show load failures without overriding TLS validation or engine security defaults. Open user-initiated new-window links in tabs. Cancel unsupported downloads with a visible explanation.

## Boundaries

This is a browser foundation, not a production-hardened daily browser. No bookmarks, extensions, password manager, download manager, session restore, or persistent browsing history UI. Use an ephemeral shared network session for this first version, if supported by the selected bindings; document the resulting cookie/storage lifetime. Do not install or upgrade system packages without consent.

## Visual direction

Quiet native GTK chrome with slate ink (#243746), paper-white (#ffffff), cool surface (#eef3f6), teal focus (#166a72), and error red (#a12e34). System sans-serif for native controls; a large serif greeting on the local page. Native navigation header above a scrollable tab strip; web content fills the remaining window. Native focus outlines and tooltips, restrained styling, no remote fonts/assets. The welcome page is the one expressive element; avoid decorative cards or metrics.

## Verification

Unit tests cover navigation normalization, scheme rejection, and searches. A real GTK/WebKit integration smoke test covers tab lifecycle, per-tab history, active/background state, local HTTP loads, and failures, with a graphical session required. Run formatting, tests, Clippy, and a real build. Report environment blockers accurately.
