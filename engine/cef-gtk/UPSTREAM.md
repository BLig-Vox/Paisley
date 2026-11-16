# Upstream provenance

servo-gtk by Ignacio Casal Quinteiro and contributors.
Source: https://github.com/nacho/servo-gtk
Revision: c78c94079337803173fd40e69f5f7cd62347cac7
Retrieved: 2026-11-17. License: MPL-2.0; LICENSE and source notices retained.

Paisley modifications: Rust-only build, bounded IPC and packet validation,
profile-specific runner storage, deterministic renderer shutdown, and browser
integration events. Changes are covered by MPL-2.0 where they modify upstream files.
The upstream example is retained as a native engine diagnostic, not Paisley's UI.

The production migration now targets CEF. The Servo renderer, C ABI, resources, and example apps are removed from this fork. Common GTK widget/protocol code retains its MPL notices. The new CEF runner uses cef-rs; upstream's key press logging is removed. Proto wire type names remain internal compatibility names.
