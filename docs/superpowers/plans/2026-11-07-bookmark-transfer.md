# Bookmark transfer implementation

## Design and scope

Continue the requested roadmap directly, using existing Rust/GTK/WebKit architecture and owned fixtures. Implementation choice: a strict version-1 Browny JSON interchange, not HTML parsing, browser sync or a claimed Netscape bookmark format. A native file chooser selects a local import/export. Export explicitly discloses sensitive URLs/titles and private exports surviving closure. Import validates the entire bounded file, then requires Cancel-default native confirmation. No import opens a URL, touches site storage or replaces history. Existing bookmarks win canonical-URL conflicts; new bookmarks retain source order. Merge revalidates current library limits at confirmation time, avoiding stale-snapshot replacement or partial mutation. Over-capacity imports fail rather than evict bookmarks/history.

Alternatives considered: Netscape HTML gives broader interoperability but requires a carefully bounded HTML parser and folder semantics; replacing the entire library would be simpler but destroys unrelated history/bookmarks. Choose additive typed JSON for this bounded slice. These are implementation defaults, not additional user approvals. Folder management/HTML interchange remain explicit follow-up work.

File handling reuses a shared bounded no-follow/nonblocking regular-file reader and private exclusive export publication extracted from the tested settings transfer implementation. No overwrite, external launch, shell, packages, live sites or personal state. The complete schema is version plus bookmark URI/title records; timestamps/history are omitted. At most 256 entries and 1 MiB; titles are plain text, at most 256 Unicode characters without controls; URLs credential-free HTTP(S), at most 2048 bytes, canonicalized and duplicate-checked. Parse errors disclose schema position, not payload text.

## Work

- [x] Observe failing positive model/file tests, implement strict parsing, additive merge and export.
- [x] Extract shared file I/O with settings regressions kept green.
- [x] Native import/export controls, disclosure and Cancel-default import confirmation.
- [x] Single GTK-thread owned-file tests: Cancel/Import, invalid file unchanged, close before decision, private in-memory behavior and export; no chooser/portal end-to-end claim.
- [x] Full tests, strict lint/format and isolated GTK regression; intermittent restart/MCP startup failures remain recorded, not fixed.
- [x] Update behavior docs/README/roadmap with exact evidence and remaining limits.
- [ ] Independent source review. No replacement for the cancelled worker is authorized or launched.

## Evidence ledger

- Initial five bookmark model/file tests all failed on unimplemented positive parse/merge/export/file behavior. Strict serde schema, canonical URL checks, bounded plain titles, additive merge and private no-overwrite export then passed. Negative-only cases initially passed the rejecting stub; valid roundtrip cases prove the accepted path is implemented.
- Additional unsafe in-memory export and shared FIFO nonblocking regressions pass. Shared filesystem extraction retains all existing settings tests; the settings staging-cleanup assertion now checks the actual shared prefix.
- Native missing-control regression failed (`Import bookmarks…` absent); the panic's GTK task-finalization critical is not green evidence. Native controls/chooser callbacks were then implemented and exercised through the same file methods, not simulated DOM/UI source checks.
- Native tests cover normal/private import Cancel/Import, unchanged history/current duplicate titles, stable source order, simultaneous bookmark additions, source-file replacement after parse, invalid file unchanged, output no-overwrite, literal markup-looking title rendering, export disclosure Cancel, confirmation-time capacity refusal, normal restart persistence, private no-profile-write and explicit exported-file survival.
- Closing a manager initially retained its native consent. A visibility-notification cancellation attempt failed. Signal instrumentation showed `close-request` while the lifetime token remained uncancelled, followed by late destruction. Explicit close/destruction cancellation fixed that case.
- A browser-owner-close test then demonstrated that a retained manager could still report visible with an invisible owner and export a file. Unified lifetime/owner checks and owner-close cancellation fixed this observed late-operation case. Temporary trace logging was removed; normal/private native cases pass.
- Latest checks: 48 library + 15 binary headless tests (63 total), one GTK test ignored by default; 27 Python tests; strict all-target Clippy; Cargo formatting plus explicit benchmark-file rustfmt; debug/release builds. Complete isolated GTK smoke passed in 18.13s with fatal criticals. Actual HTTP/HTTPS MCP smokes passed on the subsequent run. New bookmark/export PNGs under `target/bookmark-transfer-verification-c/` were read and visually verified.
- Reliability is not fully green: one unchanged full suite read localStorage `null` rather than `work` immediately after restart; another HTTP MCP run failed second/disconnect-workspace `browser_tabs` startup with `WebDriver: unknown error`. Later unchanged runs passed. Causes remain unestablished; no arbitrary sleeps, automated retries, security relaxations or false fix claims were added.
- Missing-a11y-bus warnings remain expected in private service-free D-Bus; credential-socket warnings remain unexplained. No native chooser/portal end-to-end claim, new packages, live site, personal profile, paid client, outward publication, commit or parent guidance change.

Remaining gates: independent review, folder/common-browser HTML/history interchange, native chooser/display/accessibility matrix and restart/MCP-startup reliability investigation, in addition to the wider roadmap. See [behavior/evidence](../../BOOKMARK_TRANSFER.md).
