# Wayland window identity fix

> Implement inline with superpowers:executing-plans.

**Goal:** Match running Wayland windows to site.blig.Paisley.desktop and its pink icon while preserving profile routing and private isolation.

**Architecture:** Keep profile-specific GApplication/D-Bus IDs and existing application flags. Apply the shared desktop ID to each mapped Wayland toplevel through GDK's public Wayland API. Install that hook at application window-added so profile-error windows also receive it. Keep the canonical desktop filename/Icon and ensure user installation and packaged template agree. X11 remains supported without forcing a Wayland downcast.

**Constraints:** No profile data format changes. No single shared GApplication ID. External-link handling remains HTTP(S)-only and goes to the correct normal profile. Private windows retain NON_UNIQUE. Every compilation uses a hard 3,000,000,000 byte cgroup limit, MemorySwapMax=0 and CARGO_BUILD_JOBS=1. Desktop/test launches and installations use Bot Screen.

- [x] Add failing focused tests for desktop identity, application routing and decimal 3 GB build limit.
- [x] Add the Wayland map hook using gdk4-wayland; preserve application IDs/flags and cover installer/template identity.
- [x] Build under the verified cap and reproduce before/after under a nested Wayland compositor. Verify multiple normal/private profiles, cookies, repeated-instance link forwarding, desktop/icon lookup and remapping.
- [x] Run focused regressions, obtain a fresh review, document evidence and exact remaining desktop limitations.

**Verification limits:** Weston can verify native Wayland IDs and desktop/icon resolution but does not reproduce GNOME/KDE/COSMIC dock presentation. Report that distinction honestly. CEF completeness is outside this narrowly specified identity fix; the hook must be backend-independent.


## Rulings and evidence

- Work in the existing shared checkout, preserving the existing Bot Screen workflow. No release, shared-branch push or default-browser replacement is part of this local fix.
- Use map, not realize: the realized GDK surface precedes the native xdg_toplevel, and the setter is a no-op until the latter exists. The first realize-only attempt failed the actual native matrix and was replaced based on GTK source and protocol traces.
- Keep the packaged desktop entry semantics unchanged; its existing canonical identity and %U argument were correct.
- Fresh review found no product blocker and identified missing SIGTERM cleanup in the test runner. Added the handler, watched its focused test fail then pass, and exercised real signal interruption with clean owned-process shutdown. Follow-up review found no blockers.
- Verification and limits: docs/WAYLAND_IDENTITY.md and docs/testing/2026-11-17/wayland-identity/.

- Follow-up authorization: the user explicitly requested publishing this fix to Cargo and GitHub. Release it as 0.2.0-alpha.2 with the existing optional-CEF scope.
