# Bookmark import and export

Open **Bookmarks** from the browser menu. **Import bookmarks…** and **Export bookmarks…** use the native local-file chooser. This is Browny's bounded JSON interchange, not Netscape/Firefox/Chromium HTML, folder transfer, sync or a complete profile backup.

## Format

```json
{
  "version": 1,
  "bookmarks": [
    { "uri": "https://example.test/", "title": "Example" }
  ]
}
```

- At most **1 MiB** and **256 bookmarks** per file.
- URI: credential-free HTTP(S), at most 2,048 bytes before and after canonicalization. No file, JavaScript or data URLs. Canonically duplicate URLs within one file are rejected.
- Title: plain text, at most 256 Unicode characters, without control characters. Markup-looking titles stay literal text, not interpreted markup.
- Version, bookmark array and each URI/title are required. Unknown/duplicate fields, nulls, wrong types, future versions and malformed/oversized inputs fail. Schema errors give position/context without echoing file payloads.
- Export includes bookmark URLs and titles **only**, not history, visit timestamps, cookies, page contents or settings. Imported bookmarks have an unknown/zero timestamp rather than a fabricated visit time.

## Import behavior

Selecting a file validates it completely without changing the library. A native **Cancel-default confirmation** explains the operation. Confirming performs an additive merge against the **current** library, not the snapshot that existed when the file was selected:

- Existing canonical URLs keep their current title/timestamp and position.
- New bookmarks append in source order; reimporting the same file is idempotent.
- New bookmarks created while confirmation is pending are preserved.
- If the combined library exceeds count or conservative storage limits, import fails with no partial changes. It does not evict bookmarks or history to make space.
- Import never opens a page, navigates the browser, alters website data or grants automation/website permissions.
- The approval applies to the parsed file held in memory. Replacing the source file while consent is pending does not change that approved batch.

Normal imported bookmarks use ordinary profile-local background/final persistence. Private imports remain in memory and disappear on close. Source files are not modified. No existing records are deleted; removal stays a separate confirmed action.

## Export behavior and local files

The native export button first discloses that URLs/titles may contain sensitive information and that the resulting file is **unencrypted**. Even a private-window export is an explicit file save that survives closure. Cancel does not export.

Exports require a new destination; existing files or symlinks are not overwritten. Settings and bookmark transfer share a mode-0600 exclusive staging/write/sync/hard-link implementation, followed by staging removal. Import reads are bounded, regular-file-only, and no-follow/nonblocking on Linux; symlinks, directories and FIFOs are rejected. Parent directories must be trusted. Same-user hostile filesystem races, same-size source changes during a read, crash residue and removable/network-filesystem semantics are not a sandbox or recovery guarantee.

Closing the manager cancels pending native operations. Closing its owning browser also cancels them. File/confirmation callbacks check the cancellation token and manager/owner lifetime before writing or changing the library. No shell or automatic file opener is involved.

## Verification — 2026-11-07

The successful integration run below is historical evidence, not a reliability guarantee. A later [restart reliability investigation](RESTART_RELIABILITY.md) reproduced storage/load failures and an engine-reported NetworkProcess crash. It recorded a 30.35-second complete GTK pass without a storage/engine fix; later [Inspector verification](INSPECTOR.md) recorded another complete pass and then an Inspector-reopen failure. The intermittent failures remain unresolved. No release-readiness claim is made.

Five initial model/file tests failed on the unimplemented transfer functions, then passed after implementation. A later unsafe-export test and shared FIFO regression also pass. The headless suite now contains **63 passing Rust tests** (48 library + 15 binary); the single GTK smoke is ignored by default. **27 Python tests**, strict all-target Clippy, formatting and debug/release builds passed.

The complete isolated GTK smoke passed in **18.13 seconds** with fatal GLib criticals enabled. Both normal and private cases exercised real native import Cancel/Import, preserving concurrent bookmarks, source-file replacement while consent is pending, literal title rendering, current-capacity rejection without mutation, shared file import/export handlers, export-disclosure Cancel, manager/browser-close cancellation, normal restart metadata and private no-profile-write. Actual disk-full/permission-retry fixtures also passed. Native chooser/portal file-selection itself was not automated end-to-end.

Final native bookmark/export-disclosure PNGs under ignored `target/bookmark-transfer-verification-c/` were read and visually checked. `scripts/smoke_isolated.py --screenshots NEW_ABSOLUTE_DIRECTORY` now produces them along with settings/profile captures. Only owned fixtures are used; no public network or personal profiles are available in the test namespace. See [profile/settings verification](PROFILE_SETTINGS.md) for the runner command and dependencies.

Actual HTTP/HTTPS MCP smokes passed again, retaining trusted native Unicode/click/wheel input, PNG, origin/TLS checks and cleanup. They do not establish arbitrary-site compatibility or independent production-client interoperability.

### Failures retained, not hidden by green runs

- The intentional missing-control native RED failed and emitted a GTask-finalization critical during panic teardown. No failed run is treated as passing evidence.
- A close-pending-confirmation regression timed out. Instrumentation showed `close-request` with an uncancelled token; GTK visibility notification did not cancel it. Explicit close/destruction cancellation fixed that case.
- The owning-browser-close regression then showed a retained manager still reporting visible while its owner was hidden. A shared owner/cancellation check and owner-close cancellation fixed the demonstrated late-export case; both normal/private native tests passed afterward.
- One complete run failed the pre-existing immediate-restart localStorage assertion (`null` instead of `work`). Subsequent unchanged runs passed. The cause is **not established**; persistent-site-data restart reliability remains a release blocker, not a fixed bug.
- One HTTP MCP run failed `browser_tabs` when starting its second/disconnect workspace with `WebDriver: unknown error`, after the first workspace's input/Stop checks. The next HTTP and HTTPS runs passed. That startup failure remains unresolved; no automatic retry or less-isolated fallback was added.
- Missing-accessibility-bus warnings are expected in service-free D-Bus; credential-socket broken-pipe warnings remain unexplained. Neither is an accessibility/security validation claim.

Remaining: folder management, common browser HTML interchange, history transfer, full native chooser/portal/accessibility/display matrix, independent feature review, and restart/startup reliability investigation. The [full roadmap](../ROADMAP.md) is still in progress.
