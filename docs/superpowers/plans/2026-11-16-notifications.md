# Notifications Implementation Plan

**Goal:** Native in-app event center and explicitly authorized live website notifications.
**Architecture:** GTK event center plus profile-local exact-origin grants and per-tab WebKit contexts.
**Spec:** docs/superpowers/specs/2026-11-16-notifications-design.md
**Constraints:** No unsolicited automated tests. Preserve file contents unrelated to notifications, profile isolation and denied sensitive permissions. No publication requested for this change.

- [x] Add validated bounded notification origins to Settings, preserving import/export behavior.
- [x] Build bounded event center, unread bell, timed banner, notification lifecycle and compact sheet UI.
- [x] Integrate explicit site opt-in/reopen/revocation, per-tab contexts and real WebKit Notification delivery.
- [x] Wire download completion/failure and process failure events.
- [x] Build; install atomically inside Bot Screen; manually inspect isolated fixture flows and update capability documentation with observed limitations.
