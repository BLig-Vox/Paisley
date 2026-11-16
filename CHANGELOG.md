# Changelog

## 0.1.1 — Logo, desktop integration and notifications

- Pink Paisley logo in the repository, desktop launcher and packaged assets.
- Bundled `paisley --install-desktop` command for user-local application-menu integration after Cargo installation.
- Native notification bell, recent activity, download alerts and optional Linux desktop notifications.
- Explicit website notification origin permissions in normal profiles; private windows keep notifications in the app.
- Address-bar site information, TLS certificate details, per-site protection and confirmed domain-scoped cookie/storage clearing.
- Preserved existing profiles and session storage paths.

See docs/NOTIFICATIONS.md and docs/SITE_INFORMATION.md for behavior and limits.

## 0.1.0 — Paisley

First Paisley prerelease, continuing the Browny project. The executable is `paisley` and the Cargo package is `vox-paisley`; existing `browny` profile directories remain compatible.

- Docked application menu with secondary sheets for settings, profiles, site protection, bookmarks and history.
- Searchable profile management and inline bookmark editing.
- Downloads in a right sidebar, with received/total sizes and preserved paused/failed progress.
- Managed public direct downloads with bounded concurrency, resumable checkpoints, optional publisher hashes and safe finalization.
- Right-docked Web Inspector, tab favicons and middle-click tab closing.
- Persistent per-profile favicon cache, same-origin fallback, live row updates and domain initials for missing icons; private caches stay in memory.
- Documented GStreamer video playback and multilingual/color emoji font requirements.
- Preserved existing profile isolation, session restoration, bounded history and memory reporting.

Paisley remains an early browser. Web compatibility, codec/DRM availability and platform support depend on WebKitGTK and the operating system. The request blocker is a small starter rule set. Native permissions remain denied when the requesting frame cannot be verified. The opt-in MCP runtime has its own documented requirements and restrictions.
