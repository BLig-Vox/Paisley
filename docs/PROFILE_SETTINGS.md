# Profile and settings management

## Profiles

Open Profiles from the browser menu. Entering an unused validated ID creates a profile through the normal launcher; an existing profile opens/activates its separate runtime. The catalog discovers existing normal profiles under the XDG config/data/cache roots, displaying at most 256 IDs and scanning at most 3,072 directory entries. Private windows do not enumerate or modify this catalog; explicitly opening a separate normal profile is a separate action.

- IDs remain 1–64 ASCII letters, digits, underscores or hyphens. They identify directories and runtime locks, not display names.
- **Rename** changes a closed/unlocked profile's display name only. Names are 1–128 UTF-8 bytes, without control characters or surrounding whitespace. The optional version-1 `profile.json` in its config directory contains the plain-text name; data paths, sessions, IDs and cookies do not move. Reopened browser titles/status include the name plus the stable ID. Bad name metadata is preserved and shown as an error in the catalog; normal browsing falls back to an ID with an unavailable-name indication rather than overwriting metadata.
- **Delete…** shows a native Cancel-default confirmation. It removes settings, bookmarks/history/session/download records, cookies/site data and cache inside that profile's configured directories. Downloaded files outside them survive. Deletion is refused for locked/running profiles, invalid or symlinked paths, invalid/future name metadata, and nested roots that could remove another profile's storage root.
- A root-local `.browny-maintenance.lock` outside the deletable profile folder coordinates current-version startup and catalog mutations. The existing per-profile lock still protects a running profile. Busy operations fail with a retry message, not a concurrent writer. Mixed-version writers and hostile same-user filesystem changes are not a security boundary.
- Multi-root deletion is **not a transaction**. An I/O error may leave a partially deleted profile, and the error says so. Large trees can make synchronous maintenance slow; no crash recovery, undo or untrusted-filesystem guarantee is claimed. Config/cache-only orphan records without a data directory need separate recovery, not silent destructive repair.
- Name records are bounded to the existing 1-MiB versioned-record limit, with a 128-byte validated name. Corrupt, oversized, nonregular and future records are rejected/preserved. Record opens are no-follow/nonblocking on Linux so a FIFO cannot hang discovery.

## Settings draft

Settings presents only implemented controls: homepage, session restoration, history recording, partial memory display, bundled starter blocking and Strict WebRTC/third-party-cookie behavior. Search filters the supported controls. Scope/application timing and unsupported permission/proxy/fingerprinting controls are explained; unsupported fields do not appear to work.

Editing, importing or resetting changes a **draft**. Save validates/persists it in normal profiles and applies it in memory; private profiles keep it in memory. Closing without Save discards edits. The Save/error footer stays visible while the form scrolls. New-request settings do not undo requests already sent or delete existing history/site data.

**Reset draft…** requires native confirmation because defaults enable history recording/session restoration/memory/starter blocking, turn Strict off, reset homepage to DuckDuckGo and clear blocking exceptions. It does not erase history, bookmarks, cookies or downloaded files. Even after confirmation, Save is required to apply the reset.

## Import/export

The native file chooser accepts local files only. Imports are at most **64 KiB**, regular/no-final-symlink/nonblocking reads; directories, FIFOs, malformed JSON, duplicate/unknown/future/null/wrong-type fields and unsafe URLs are refused without changing the draft or source file. Parent directories must be trusted; this is not a hostile same-user filesystem boundary. The file chooser itself and desktop portals vary by environment.

A supported patch looks like:

```json
{
  "version": 1,
  "settings": {
    "restore_session": true,
    "remember_history": false,
    "show_memory": true,
    "block_trackers": true,
    "strict_privacy": false
  }
}
```

Omitted fields retain the draft's current values. Explicit import may also contain `homepage` (credential-free HTTP(S), at most 2,048 bytes) and `blocking_exceptions` (at most 128 unique canonical HTTP(S) origins of at most 2,048 bytes each). Origins contain no slash/path, query, fragment, credentials or redundant default port, for example `https://example.test`. Imported site exceptions are visible in the draft's expandable list before Save.

**Default export deliberately includes only the five supported switches above.** It omits homepage, site exceptions, local paths, browser records, credentials and unused agent/download-directory fields; it is not a complete backup or exact transfer of all settings. No import enables MCP, launches an agent or grants site permissions. Even in a private window, an explicitly requested export writes a file that survives closing that window; the UI discloses this.

Export uses a private mode-0600 staging file, syncs it, links the complete data exclusively into a new destination and removes staging. Existing files/symlinks are never replaced. Cancellation does not save anything; chooser/I/O failures are shown. Crash residue and network/removable filesystem semantics are not a recovery guarantee.

## Verification and limits

On 2026-11-07, **56 Rust headless tests and 27 Python tests passed**, with strict all-target Clippy, debug/release builds and formatting. Actual isolated HTTP/HTTPS MCP fixtures passed again without changing automation authority.

The complete single-threaded GTK smoke ran through `scripts/smoke_isolated.py`, with user/network/PID/mount/IPC isolation, disposable HOME/runtime, service-free private D-Bus, read-only executable/toolchain and no public network. It exercised:

- actual settings/search widgets and native reset Cancel/Confirm, draft-only import and export through the same file/draft functions used by chooser callbacks;
- invalid imports leaving the draft unchanged, reset not saving, explicit Save, close-without-Save and the always-visible Save footer;
- actual catalog rename, stable IDs and native deletion Cancel/Confirm, using owned temporary profiles only.

Final graphical run: **30.78s**, `G_DEBUG=fatal-criticals`; native settings/catalog images under ignored `target/profile-settings-verification-b/` were read and visually checked. Expected accessibility-bus warnings occur because accessibility services are intentionally absent; this is not accessibility validation. Earlier nonfatal credential-socket warnings remain unexplained. The native file chooser's portal/file-selection interaction was not automated end-to-end; shared file/draft handlers and real confirmation controls were exercised. Profile-launch buttons were not clicked, so tests did not spawn personal profile runtimes.

Reproduce with the Cargo-reported unit-test executable path (hashes can differ):

```sh
cargo test --offline --locked --no-default-features --features legacy-webkit --bin paisley-engine --no-run
python3 -I scripts/smoke_isolated.py \
  /absolute/path/to/target/debug/deps/paisley-TEST_HASH \
  --screenshots /absolute/path/to/new-empty-output-directory
```

The screenshot directory must **not exist**; the runner creates it exclusively. Only the display socket and, when requested, this new output directory cross the test namespace. The runner is for the compiled Rust **test executable**, not `target/debug/paisley`, and is not an MCP backend or an isolation fallback. It requires the already-installed tools documented for MCP; it installs nothing. Prefix host commands with `distrobox-host-exec` where applicable.

Still open: expanded settings categories/search providers/site-data controls, advanced migration/recovery, native chooser/portal matrix, large-tree responsiveness and interrupted deletion, private-window UI coverage beyond model guarantees, independent review, and broader daily-use/release criteria. This feature does not complete the whole roadmap.
