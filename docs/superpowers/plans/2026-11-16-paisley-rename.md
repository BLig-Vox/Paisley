# Paisley Rename Implementation Plan

**Goal:** Rename Browny to Paisley in code, packaging and Bot Screen.

**Architecture:** Rename the Cargo package to `vox-paisley`, explicitly name the
library and executable `paisley`, and update consumers together. Preserve
existing on-disk data paths and markers.

**Tech Stack:** Rust, GTK4, WebKitGTK, Python packaging and MCP adapter.

**Spec:** ../specs/2026-11-16-paisley-rename-design.md

## Constraints and review focus

- Preserve saved profile paths, session markers and maintenance lock names.
- Update Rust library imports and Python adapter environment variables together.
- Keep the checkout and GitHub remote unchanged.
- Keep packaging executable and desktop filename consistent.
- Do not add or run automated tests; this session has not requested them.

## Tasks

- [x] Rename Cargo identity, labels, adapters, fixtures and current docs.
- [x] Review remaining legacy strings and diff for data compatibility.
- [x] Attempt Bot Screen build; identify Pango 1.52 versus required 1.55 mismatch.
- [x] Build with workspace SDK and install `paisley` plus its Bot Screen launcher.
- [x] Launch in Bot Screen and observe Paisley branding in the window status.

## Evidence

Workspace `cargo build --locked` completed successfully. In Bot Screen, the
installed binary printed `Paisley` help, `ldd` reported no missing libraries,
and `desktop-file-validate` accepted the source and installed launchers. The
normal default-profile window opened and displayed Paisley in its RAM status.
Automated tests were not run. Source changes remain uncommitted; the remote
and checkout directory still use Browny.

