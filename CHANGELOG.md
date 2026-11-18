# Changelog

## 0.2.0-alpha.6 — Focus, dragging and binary bundle

- Keep CEF focus with the selected GTK content widget, including DevTools.
- Forward held mouse buttons during motion so scrollbar and page dragging work.
- Add explicit, deny-by-default desktop-video capture permission requests; cancel prompts on navigation, shutdown and renderer replacement. Camera, microphone and capture audio remain unsupported by this bridge.
- Provide a Linux x86_64 prebuilt bundle containing both executables and the matching CEF runtime.
- Support a hard decimal 2 GB build cap and preserve a 4 GiB system memory reserve.

## 0.2.0-alpha.5 — Scrolling and close responsiveness

- Route wheel events to the last pointer location so nested panels can receive scrolling.
- Save the final session off the GTK close callback and wait for pending saves before exit.
- Add nested scrolling, shifted quote input and session persistence regression checks.

## 0.2.0-alpha.4 — Keyboard and inspector stability

- Fix period being interpreted as Delete in CEF inputs. Map punctuation and shifted digits to Chromium virtual keys, preserving separate Unicode text events.
- Raise the inspector relay message-count ceiling from 64 to 4096 while retaining the 4 MiB payload limit per queue, preventing ordinary startup/console bursts from closing DevTools.
- Regressions for text/email/textarea input, Delete and Backspace, punctuation, burst ordering and queue bounds.

## 0.2.0-alpha.3 — CEF by default

- Make CEF the default backend; deprecate WebKit behind explicit `legacy-webkit`.
- Add a runtime-aware `paisley` launcher and separate `paisley-engine` binary.
  Desktop integration and local packages retain the launcher for external links.
- Keep profile routing, private windows, residency limits and production sandbox
  policy unchanged. CEF parity gaps remain documented.


## 0.2.0-alpha.2 — Wayland desktop identity

- Running Wayland windows use the installed Paisley desktop identity and pink icon across profiles and private instances, while keeping profile-specific application routing.
- Optional hard decimal 3 GB build limit, zero build swap and one Cargo job, with kernel limit verification.

## 0.2.0-alpha.1 — CEF migration preview

- Optional CEF 156.0.30 backend, one engine per profile, bounded IPC and latest-frame queues.
- Conservative LRU sleeping of inactive eligible CEF tabs, with active pages, unknown activity, forms and media protected. Production thresholds remain five minutes idle, or thirty seconds under memory pressure.
- CEF downloads with received/total bytes, pause/resume/cancel, file-type icons and atomic no-overwrite publication; failed/cancelled staging cleanup.
- Native CEF dialogs, HTTP authentication and file inputs; offscreen dropdown painting; right-side real DevTools.
- CEF favicon capture, site-data clearing and strict cookie/WebRTC behavior for newly opened tabs.
- Kernel-enforced 2 GiB build supervisor, regression fixtures and whole-process PSS measurements.
- WebKitGTK remains the default and prebuilt backend. CEF needs a separate matching runtime; website notifications/permissions, IME, HiDPI and opener-preserving popups are incomplete. Secure non-root CEF deployment has not been validated. See docs/CEF_PRERELEASE.md.

### Shared browser fixes

- Smaller WebKit browsing caches, disabled full Back/Forward page snapshots and earlier native cache reclamation, without automatic live-tab discarding.

- Memory counter sampled about once per second, with newest-only delivery, decimal MiB, stale markers and cleared unavailable readings.

- HTTP/HTTPS command-line links and desktop scheme handling, forwarded into new tabs in the running profile.
- URL-aware desktop launcher and documented default-browser setup.

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
