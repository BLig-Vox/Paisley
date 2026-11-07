# Profile and settings management implementation

**Goal:** Implement the remaining native profile catalog/display-name/deletion operations and practical settings reset/import/export, without altering stable profile IDs, private-mode persistence or automation authority.

**Scope:** Local browser features and owned fixtures. No host packages, personal-state tests, live sites, subprocess/profile launches during implementation verification, commits, publication, license selection or parent CLAUDE.md changes.

## Decisions

- Discover bounded existing profile IDs under configured profile roots rather than inventing a new identity scheme or moving existing data. Display names are separate bounded versioned metadata; rename changes presentation, not identity/path. Closed/unlocked profiles only for rename/deletion. Native explicit destructive confirmation; partial multi-root errors must be visible, not described as a transaction.
- Settings interchange is an explicit version-1, at-most-64-KiB typed patch. Reject unknown/duplicate/future/null/wrong-type fields, unsafe home URLs and noncanonical/duplicate/over-limit exception origins. Omitted fields retain their current values.
- Default exports contain only supported boolean preferences, omitting homepage, site exceptions, local paths and unused automation flags. The interface explains those omissions; do not pretend it is a complete backup. No import can enable MCP or grant site permissions.
- Import/reset change a visible draft; Save is the only application/persistence action. Reset needs confirmation, especially because defaults enable history recording. Existing browser history/site data are not deleted by reset.
- Settings file handling is bounded, no shell, no following a final symlink/nonregular import, and export must not overwrite an existing file. Native chooser, local paths only, error shown without secret file content. Private export is an explicit user-requested file operation, never automatic profile persistence.
- Reuse the existing single GTK smoke thread for native regression helpers. Keep all GUI/page traffic in owned fixtures or an OS-enforced private network namespace; don't rely on default-home assertions to avoid public requests.

## Work

- [x] Catalog model verified; leader-owned nested-root/maintenance/FIFO/display-name regressions observed failing, fixed and passing, with bounds/corruption/private/lock cases covered.
- [x] Native catalog listing, closed-profile rename and confirmed deletion, with clear errors and stable IDs.
- [x] Settings patch/export regressions observed failing, implemented and passing.
- [x] Searchable supported settings and native reset/import/export draft workflow integrated and exercised. Native chooser end-to-end selection/portal matrix remains separate coverage.
- [x] Leader source inspection, integration fixes and full headless/graphical/lint/build/format checks.
- [ ] Independent source review of this new feature. The original worker was cancelled before hand-back; no replacement was launched without authorization.
- [x] README/roadmap/evidence updated, including unsupported options, scope limitations and remaining gates.

## Ownership

The delegate initially owned `profile.rs`, `profile_catalog.rs`, catalog module export and `settings_ui.rs`. It was cancelled before hand-back; a resume request was refused, and no replacement agent was launched. The leader read the saved partial files and finished/verified them directly, including settings-transfer UI and native test integration. The original worker's complete RED verification record is unavailable; no claim is made that every inherited scaffold followed TDD.

## Evidence ledger

- The five settings-transfer tests were observed failing on unimplemented export/import/file behavior, then passed after bounded strict schema/file implementation. Unknown/default-only negative cases initially passed a rejecting stub; positive patch/file behavior tests demonstrate that the implementation actually accepts valid data.
- Native settings control regression failed on missing Import/Export/Reset controls. Real widgets and shared chooser file/draft handlers were then integrated. The graphical smoke exercises native reset Cancel/Confirm, draft-only import/export, unknown-version rejection without mutation, explicit Save, close without Save, and nonempty search matches.
- Actual owned catalog UI rename and native delete Cancel/Confirm are exercised without launching profile subprocesses. Stable ID/display-name reopening, locks, private no-write, corrupt/future metadata, bounded profiles/names/records, symlinks and invalid IDs are covered headlessly.
- A nested-root test reproduced deletion removing an unrelated owned profile's data; preflight root ancestry refusal fixed it. FIFO metadata was observed waiting for a writer; no-follow/nonblocking profile record opens now reject it promptly. Reopened label initially returned the ID instead of the renamed label, then passed after own-profile metadata integration.
- Startup and catalog operations initially ignored a held maintenance lock. A stable root-local maintenance lock now coordinates them and is retained through deletion, outside the removable profile directory. The initial GREEN run exposed a test setup collision because normal startup now creates this lock; setup was corrected to open the existing file without truncation. No claim of hostile-filesystem, transactional multi-root deletion or mixed-version coordination.
- Native screenshot inspection exposed the Save button below the viewport. The allocation regression failed, then passed after placing Save/status outside the scrolling form. Both final PNGs under `target/profile-settings-verification-b/` were visually read.
- Fresh final verification: 41 library + 15 binary headless tests (56 total); 27 Python tests; all-target strict Clippy; debug/release builds; complete isolated GTK smoke (30.78s, fatal criticals) and actual HTTP/HTTPS MCP smokes passed. Accessibility-bus warnings are expected with service-free private D-Bus; earlier credential-socket warnings remain unexplained. Native file-chooser/portal selection was not driven end-to-end.
- `scripts/smoke_isolated.py` saves the already-tested namespace recipe as a reproducible test-only runner. It keeps all public networking unavailable even when ordinary smoke asserts the DuckDuckGo default. No engine sandbox flag, permission denial, MCP backend boundary, host package, personal profile or parent guidance file was changed.
- Remaining: independent feature review, broader native chooser/private-window/display/accessibility matrices, large-profile maintenance responsiveness, interrupted-delete/recovery/migration workflows and the rest of the roadmap. All uncompleted work stays explicitly open.

References: [behavior and verification](../../PROFILE_SETTINGS.md), [roadmap](../../../ROADMAP.md).
