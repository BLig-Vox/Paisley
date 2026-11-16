# Downloads Sheet Implementation Plan

**Goal:** Present Downloads as an in-browser sheet with readable file rows.

**Architecture:** Wrap browser content in a GTK Overlay and host a scoped sheet
and dismiss scrim. Keep transfer handling and persistence in Downloads; place
icon, date and source formatting in download_ui.rs with scoped CSS.

**Spec:** ../specs/2026-11-16-download-sheet-design.md

**Constraints:** No new dependency; no change to download file safety or history
format. No automated tests added or run without the user's request. Native GTK
visual inspection uses Bot Screen, not Playwright (this is not a web frontend).

- [x] Add file type icons, readable dates and scoped download styling.
- [x] Implement overlay sheet, scrim/Close/Escape, focus and sensitivity restoration.
- [x] Keep filename, status/progress and actions distinct in active and saved rows.
- [x] Adjust existing fixture window lookups to the sheet's parent browser.
- [x] Build source with workspace SDK (Bot Screen native Pango is too old).
- [x] Install and visually inspect empty and populated sheets on Bot Screen.
- [x] Inspect a smaller window, dismissal, Reveal and Clear records.
- [x] Resolve Escape conflict by dismissing Downloads before the browser Stop action.

Bot Screen was in human control during implementation. Installation and visual
review must wait for Return to agent; source compilation is not visual evidence.

## Observed result

Installed the build in Bot Screen. Inspected the empty sheet in the normal
profile and four populated rows (PNG, text, Rust, ZIP) in a disposable preview
profile with separate XDG directories. Observed at approximately 1280×800 and
814×531 actual window sizes. Rows fit, actions remained visible and content
scrolled in the smaller window. Reveal selected the preview PNG in Thunar.
Clearing preview records returned the empty state. Escape, close button and
scrim dismissal were exercised; the address field regained focus. Preview
records were isolated from the user's normal history. No automated tests ran.
Active transfer/cancel/retry behavior was not exercised during this visual review.
Native GTK was inspected via Bot Screen; Playwright does not inspect this GTK UI.
The default profile was reopened afterward. Source changes remain local.

## User correction: full content sheet, not floating

The initial overlay did not match the user's meaning of sheet. Replaced it with
a GTK Stack page spanning the browser content area. Removed the scrim, outer
margins, rounded card, shadow and disabled toolbar. Added aligned File, Size,
Status, Date and Actions columns to active and saved rows. Downloads toggles back
to the browser; Escape/Close also returns, and address navigation/new tabs dismiss
the sheet. Built successfully and observed populated rows on Bot Screen.
