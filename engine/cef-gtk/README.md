# Paisley CEF GTK adapter

Prerelease GTK4 offscreen-rendering adapter for cef-rs 156.6.0 and CEF 156.0.30. Used by Paisley as the default `cef` backend. Servo is not linked or used; some internal protocol names retain historical Servo names.

Requires Rust, GTK4 4.12+, protobuf compiler and the exact matching CEF runtime. Set `CEF_PATH` for building and `LD_LIBRARY_PATH` plus `PAISLEY_CEF_RUNTIME` for running; a Cargo-installed executable alone is insufficient. Production keeps the Chromium sandbox enabled and must run as an ordinary user. The no-sandbox override is restricted to debug test-support builds, never ordinary release builds.

Call `run_as_runner_if_requested()` at the beginning of your executable before GTK initialization. Processes re-execute that executable, sharing one engine per profile.

Not a drop-in replacement for WebKitGTK: website notifications/permissions, IME composition, display scaling and opener-preserving popups remain incomplete. See the Paisley repository's `docs/CEF_PRERELEASE.md` for setup and limitations.

Adapted GTK integration files remain MPL-2.0. See LICENSE and UPSTREAM.md for provenance.

0.1.0-alpha.2 fixes punctuation virtual-key mapping (period must not become Delete) and accommodates bounded DevTools protocol bursts without the old 64-message disconnect.

0.1.0-alpha.4 separates focus from visibility, preserves held mouse buttons during motion, and introduces cancellable, deny-by-default desktop-video permission requests. End-to-end capture compatibility is unverified.
