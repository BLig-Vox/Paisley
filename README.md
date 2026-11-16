# Paisley

<p align="center"><img src="assets/branding/paisley-logo-stand.png" alt="Paisley logo" width="180"></p>

Formerly Browny. The executable is `paisley`; the Cargo package is `vox-paisley`
on crates.io. Existing profiles continue to use the `browny` storage
directories below, so this rename preserves settings, sessions and site data.
The internal `browny:home` address remains an alias for `paisley:home`.
MCP and diagnostic environment variables now use the `PAISLEY_` prefix.


A Linux-first web browser written in **Rust**, with **GTK4** for the native
interface and **WebKitGTK** for rendering and JavaScript. Default window:
**1280 × 800**.

## Install

```sh
cargo install vox-paisley --version 0.1.1 --locked
paisley --install-desktop
```

The desktop integration command installs the launcher and bundled logo for your
user, without sudo, and points the launcher at the installed executable. Run it
again if you move or reinstall the binary in another directory. Cargo alone
installs the executable; it does not register desktop entries. For a source
checkout, use `cargo install --path . --locked` first.

The executable is `paisley`. Building requires the Linux development libraries and
Pango 1.55+ SDK described below. WebKitGTK 6, GTK 4.10+, GStreamer codecs and
system fonts must also be installed on the machine running the browser; see
[Runtime dependencies](docs/PACKAGING.md). The Cargo install does not install
those system packages.

Prebuilt Linux assets are available from
[GitHub Releases](https://github.com/BLig-Vox/Paisley/releases). This first release
is a prerelease; platform requirements and checksums accompany the binary.

## Current features

- Address-bar **site information** with WebKit TLS details, per-site starter
  protection and confirmed domain-scoped cookie/storage clearing.
  [Site information and limits](docs/SITE_INFORMATION.md).

- Native **Notifications** bell, unread badge, recent activity and quiet banners;
  download events, optional Linux desktop alerts, and explicitly allowed website notifications in normal profiles.
  [Permission flow and limits](docs/NOTIFICATIONS.md).

- Independent, reorderable tabs with their own navigation histories, titles,
  loading indicators, errors, and close buttons.
- Address/search bar, Back/Forward, Reload/Stop, Home, DuckDuckGo search and home.
- **Persistent normal profiles**, profile-specific site data/cookies/settings,
  profile locks, and a native catalog to discover/open profiles, rename closed-profile
  display names without changing IDs/paths, and delete closed profiles after explicit
  confirmation. A root maintenance lock coordinates startup with rename/deletion.
- **Searchable supported settings**, confirmed reset of the draft, and bounded typed
  preferences import/export. Import/reset never apply until Save; default export
  omits site URLs, exceptions, paths and secrets. See [profile/settings management](docs/PROFILE_SETTINGS.md).
- **Tab restoration** with order/selection preserved; restored background tabs
  load on activation. Temporary blob/data/about documents restore to native **Not
  restored** placeholders without saving their payloads or replaying them. Atomic
  bounded snapshots are written by a background worker. See [session behavior and limits](docs/SESSION_RESTORE.md).
- **Private windows** with ephemeral website storage and no persisted browser
  profile settings/restore/history records. Private bookmarks are temporary.
- **Per-profile bookmarks and searchable history** with native managers. Ctrl+D
  opens the bookmark editor; Ctrl+H opens history. History recording has a Settings
  opt-out; clearing history requires confirmation; bookmark editing and removal use an inline sheet. Native
  [bookmark JSON import/export](docs/BOOKMARK_TRANSFER.md) uses a bounded additive,
  confirmed merge; private imports stay temporary and explicit exports survive close.
- Native **Web Inspector**, including the engine's console, DOM/CSS inspection,
  JavaScript debugger and other available panels. Right-click Inspect Element,
  F12, the toolbar, or the menu opens the real Inspector for the active tab.
  Native dock/detach/close lifecycle is tested; closing a tab or its browser closes
  detached tools. [Verification and remaining panel-workflow gaps](docs/INSPECTOR.md).
- **Downloads** with Save dialog, progress, cancellation, retry while the source
  tab exists, completion/failure UI and explicit Reveal in folder. Bounded terminal
  metadata persists in normal profiles; private records stay in memory. Clearing
  records requires confirmation and never deletes saved files. Files never execute
  or auto-open.
- **Memory in the window title and visible status bar**, including Linux-visible
  owned engine subprocesses. PSS is preferred; RSS is an approximate fallback.
  Coverage is explicitly partial, not a measurement of every profile on the host.
- A small **bundled native request-blocking starter set** for twelve ad/tracker
  domains, on by default, with per-origin exceptions and a global settings toggle.
  This is not a full EasyList/uBlock/Brave implementation or an updating list service.
- WebKit tracking prevention enabled. An optional Strict setting disables WebRTC
  and declines third-party cookies; it can break calls/logins and does not
  establish fingerprinting resistance or anonymity.
- Explicit sensitive-permission denial with an explanation while verified
  requesting-frame attribution and safe per-origin grants remain unavailable.
- Scoped DuckDuckGo promotion cleanup that preserves search/results/menu links.

## Roadmap and interactive automation

[ROADMAP.md](ROADMAP.md) describes the larger product goal and records current
progress. **The intended MCP feature is interactive:** real clicks, arbitrary
text input, keys, scrolling, navigation, screenshots and inspection, with visible
activity and user Stop—not a read-only visual substitute.

An opt-in **interactive MCP origin workspace is implemented**, with native trusted
clicks, Unicode/emoji input, keys, wheel scrolling, rendered PNGs, inspection,
trusted action/cursor display and Stop. It accepts one canonical HTTPS origin or
an owned HTTP/HTTPS loopback fixture, using a dedicated ephemeral runtime—not
personal tabs or unrestricted destinations. General HTTPS requires a native
workspace grant plus single-use native approval for every navigation/input action.
Approval shows bounded target/destination details, not the requested typed value.
See [docs/MCP.md](docs/MCP.md) for setup, disclosure and limits.

After explicit approval of the local backend and then HTTPS-origin implementation,
the driver/browser remain inside mandatory private network/PID/mount/user
namespaces, with inherited stdio, a fixed-origin HTTP relay or exact-authority
pinned HTTPS tunnel, and native origin filtering. **No host TCP debugger is
exposed; WebKit TLS validation stays enabled.** This requires Wayland, Python 3,
bubblewrap, `dbus-run-session`, compatible `WebKitWebDriver` and public CA trust
for HTTPS; no packages are auto-installed. Owned HTTPS fixtures additionally use
an explicit public fixture CA, never host trust changes or a TLS bypass.

Verification used only owned HTTP/HTTPS fixtures and native consent widgets, not
live sites or personal profiles. Two-client interoperability, broad website/frame/
dialog/focus/preemption coverage, aggregate resource budgets, maintained blocking
lists, permission management, library/settings expansion, release hardening and
representative performance budgets remain roadmap work. Reproducible owned-fixture
release baselines are available in [docs/PERFORMANCE.md](docs/PERFORMANCE.md);
unsigned local-development packaging is documented in [docs/PACKAGING.md](docs/PACKAGING.md).
These checks do not establish production readiness.

**New requested MCP direction (not implemented):** headless page/browser surface, with
an always-visible trusted control status that identifies the selected Paisley profile
or Incognito and provides Stop. The user wants an explicit choice to use a selected
Paisley profile's existing site sign-ins. Current MCP still uses a separate disposable
fixture-only profile and cannot access personal sessions. No Google/live account has
been accessed. See the [updated automation requirements](ROADMAP.md#visible-agent-control-and-mcp).

## Requirements

- Linux with a graphical desktop session.
- Rust **1.92+**, Cargo, a C toolchain and `pkg-config`.
- GTK **4.10+**, WebKitGTK **2.40+**, **6.0 API** (GTK4-compatible).
- The locked Rust bindings also require Pango **1.55+** development files to build.
  Stock Ubuntu 24.04 provides Pango 1.52, so its development packages alone
  cannot build this lockfile; use a compatible newer SDK.
- Rust crates: `gtk4` 0.11, `webkit6` 0.6, `url`, `serde`, `serde_json`.

The 6.0 API version is not the WebKit engine release number. Keep the system
engine patched; the minimum build version is not a security-update guarantee.
GTK3 packages such as `webkit2gtk-4.1` are not substitutes.

Arch setup example (install only with the machine owner's consent):

```sh
sudo pacman -S --needed base-devel rust pkgconf gtk4 webkitgtk-6.0
```

Ubuntu/Debian with sufficiently recent packages:

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libwebkitgtk-6.0-dev
```

## Run

```sh
cargo run --locked
cargo run --locked -- --profile work
cargo run --locked -- --private
```

Profile IDs are 1–64 ASCII letters, digits, underscores or hyphens. Reopening an
already-running normal profile activates its window rather than sharing its data
with another runtime. A lock prevents two independent writers opening one profile.
Normal startup restores saved tabs if available; otherwise it opens DuckDuckGo.
Private startup, new tabs and Home also open DuckDuckGo, so they contact the network.

Release build:

```sh
cargo build --release --locked
./target/release/paisley --profile work
```

If working inside Distrobox with development tools installed on the host, prefix
development commands with `distrobox-host-exec`, for example:

```sh
distrobox-host-exec cargo run --locked -- --profile work
```

Keep `Cargo.lock` in version control. `target/` is ignored.

## Controls

The menu contains Settings, Profiles/private window, site blocking, Downloads,
Bookmarks, History, and Web Inspector. Site-blocking exceptions are stored by exact HTTP(S) origin;
toggle and reload the site to apply the change to its existing content.

| Shortcut | Action |
| --- | --- |
| Ctrl+L | Focus/select address bar |
| Ctrl+T / Ctrl+W | New tab / close tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+PageDown / Ctrl+PageUp | Next / previous tab |
| Ctrl+R / F5 | Reload |
| Alt+Left / Alt+Right | Back / Forward |
| Escape | Stop loading and focus page |
| F12 / Ctrl+Shift+I / Ctrl+Shift+J | Toggle Web Inspector; select Console inside it |
| Ctrl+J | Downloads |
| Ctrl+D | Bookmark current page |
| Ctrl+H | Search profile history |

Right-click Inspect Element uses WebKit's native context menu. Inspector panels
and availability depend on the engine version; Chrome DevTools Protocol
compatibility and native Rust debugging are not implied.

## Stored data and private mode

Normal profile locations (respecting the corresponding XDG overrides):

- `~/.config/browny/profiles/<id>/settings.json`
- `~/.local/share/browny/profiles/<id>/session.json`, `library.json`, `downloads.json`, cookies and WebKit site data
- `~/.cache/browny/profiles/<id>/` for engine/filter caches

Directories are restricted to the owner, records are versioned/bounded, and
invalid or future-version state is reported instead of silently reset. Settings
include startup restoration, homepage, history recording, memory display, blocking and Strict mode.
Profile data is **not encrypted at rest**. Separate profiles are not an OS-level
security boundary against malicious software running as the same user.

Restore records contain safe HTTP(S) URLs or a fixed non-restorable-tab marker,
not page contents, transient payloads/titles, form drafts, passwords, POST bodies
or a complete navigation-history database. Blob/data/about temporary documents
keep their tab position but display a native explanation after restart; enter a
web address or use Home to continue. The marker is not a navigable URL. URLs
themselves can be sensitive. Disable restoration in Settings to clear the next
snapshot. Website logins may expire according to site/cookie rules despite tab
restoration. See [session format, privacy and downgrade limits](docs/SESSION_RESTORE.md).

History and bookmarks contain credential-free HTTP(S) URLs and plain-text titles,
not page bodies. Normal history recording is on by default; disabling it stops new
records but does **not** erase existing history. Use History → Clear history to
remove recorded URLs/titles; cookies, site data, bookmarks and restored tabs are
separate. History keeps at most 1,000 unique canonical URLs, newest first, and
bookmarks at most 256. Old history may be removed earlier to keep the record under
its size budget; bookmarks are never silently evicted. Titles are capped at 256
characters and URLs at 2,048 bytes. Managers show up to 200 search matches at once.
Private history is never recorded; private bookmarks live only in memory.
Paisley bookmark JSON import/export is supported; folders, common browser HTML
interchange, history import/export, sync and a complete visit-by-visit history
database are not implemented. Session, library and download-record files are
independently atomic, not one cross-file transaction.

Private mode does not persist normal profile records. Compiled, non-site filter
rules use a private temporary cache removed on ordinary cleanup. Explicitly saved
downloads remain on disk. Private browsing cannot prevent OS swap/crash traces,
network/server records, or identification by websites.

## Downloads and security boundaries

Advanced direct transfers are available through **New download…** in Downloads:
pause/resume, up to four validated range connections, two concurrent jobs,
speed limits, retries, durable checkpoints and optional publisher SHA-256.
See [managed downloads](docs/MANAGED_DOWNLOADS.md) for recovery behavior and
browser-session integration.

Downloads open as a full-width sheet in the browser content area (toolbar or
Ctrl+J), with aligned File, Size, Status, Date and Actions columns. There is no
floating window, card or dimmed backdrop. Return to the page with Escape, the
close button, or the Downloads toolbar button. Closing the sheet keeps transfers
running. File rows show bundled file-type icons, progress and readable local dates.
Icons are inferred from the filename; they do not inspect or validate file
contents. Reveal, cancel and retry remain explicit actions.

Downloads ask for a fresh local destination and keep WebKit's overwrite policy
disabled. Choose a new filename if a file already exists. Suggested names are
sanitized; existing destinations and symlinks are rejected. Filesystem races
against a hostile same-user process are not a security guarantee. Partial files
may remain after cancellation. Closing the browser cancels outstanding downloads.
Normal profiles save at most 200 terminal download records (completed/failed/
cancelled), subject to earlier eviction to fit a 900,000-byte budget. They contain
credential-free HTTP(S) source URLs when available, selected absolute paths,
received byte counts and terminal timestamps—not request bodies, credentials or
raw engine error messages. These URLs and paths can still be sensitive and are
not encrypted. If `downloads.json` is corrupt, oversized or from an unsupported
version, browsing remains available: the original record is preserved and download
metadata switches to visibly marked memory-only mode until explicit repair and
restart. This does not silently reset or migrate the file. Clear download records
removes metadata only, not downloaded files;
active transfers continue and create a record when they terminate. Private records
remain in memory, while explicitly saved private files survive closure.

Saved records are displayed after restart without resuming, retrying, opening or
probing their files. Reveal in folder is an explicit native desktop request, never
a shell command or file opener. Missing files, directories and final symlinks are
rejected with an explanation before launching; successful reveal depends on the
desktop's file-manager/portal support. Automated tests do not launch the external
file manager. Known identity Content-Length mismatches are treated as incomplete
failures even if the engine emits Finished; encoded/unknown lengths remain
engine-dependent, not independently verified integrity. Owned isolated fixtures
cover native disk-full failure and destination permission denial/retry; see the
[verified subset and limits](docs/DOWNLOAD_FAILURES.md). Pause/resume, restart
recovery of in-progress transfers and a full failure matrix remain outstanding.

WebKit's TLS, origin and process-security defaults are retained. Paisley explicitly
**denies permission requests** and explains the missing verified requesting-origin
association; it does not substitute the top-level URL for an unknown requesting
frame. Camera, microphone, location and related workflows may be unavailable.
Notifications have explicit native origin opt-in and fresh-tab permission
initialization; see [notification behavior](docs/NOTIFICATIONS.md). General
request-origin prompts and grants remain unavailable. See the evidence and
engine gaps in [docs/CAPABILITIES.md](docs/CAPABILITIES.md).
Do not mistake the current application for a production-hardened daily browser
or LibreWolf/Brave-equivalent protection.

Address-bar input accepts HTTP/HTTPS and safe host/search forms; it rejects
executable/local-file schemes and embedded credentials. Page-generated
`about:blank`/`about:srcdoc`, `blob:` and `data:` documents are separately allowed
under WebKit's normal origin/sandbox enforcement. Unsupported external protocols
are not launched.

## Development checks

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked
cargo test --locked --bin paisley browser_smoke -- --ignored --test-threads=1
python3 -I tests/test_mcp.py
python3 -I tests/mcp_smoke.py /absolute/path/to/target/debug/paisley
python3 -I tests/mcp_https_smoke.py /absolute/path/to/target/debug/paisley
```

**Current reliability status:** a private isolated core established SIGSEGV in a
WebKit NetworkCache file-monitor callback during the old stress test. Paired fixture
runs reproduced it when temporary profile/cache roots were deleted immediately after
closing a Browser and observed no NetworkProcess exit when roots were retained until
namespace shutdown. Native test fixtures now defer deletion; the updated stress and
fresh stress and full GTK runs passed again (7.73s and 24.66s) without reproducing
the crash or an internal-load error. This is a test lifetime correction, not a WebKit
fix or proof of all production durability. An older
localStorage `null` observation, profile lifecycle behavior and broader restart/display
matrix remain open. An earlier instrumented HTTPS run was revoked by the
native keyboard handler before navigation. Both actual HTTP and HTTPS native-input
smokes subsequently passed once each on the placeholder build; these passes do not
explain the earlier failures. Preemption remains enabled. See [restart investigation](docs/RESTART_RELIABILITY.md),
[Inspector evidence](docs/INSPECTOR.md) and
[bounded automation diagnostics](docs/MCP.md#bounded-revocation-diagnostics).

Headless [metadata crash coverage](docs/RECOVERY.md) now verifies whole valid session
records, unchanged settings and profile-lock release after actual owned writer-process
SIGKILL. This does not establish WebKit site-storage or power-loss durability.

The browser smoke requires a graphical session and real GTK/WebKit processes.
The MCP smoke additionally requires its isolated Wayland backend dependencies and
launches the actual executable against disposable local fixtures; see its coverage
and limitations in [docs/MCP.md](docs/MCP.md). The normal browser smoke's
local fixtures cover tabs/history/errors/address drafts, native Inspector
opening for the active tab, memory display/settings, saved attachment bytes,
cancellation/failure while a download destination is pending, and rejection of
late destination acceptance. Server counters verify request blocking/removal,
related views, cross-origin exceptions and history back/forward. Profile restart
checks isolated persistent cookies, local storage, tab ordering and lazy restore.
Library fixtures cover bookmark actions, native manager search/filtering, restart
persistence, disabled/private history, temporary private bookmarks, committed pages
with unfinished resources, SPA/fragment visits, failed-navigation exclusion, native
Cancel/Remove confirmations and persisted deletion. Background writes retry on
failure; an invalid session snapshot does not prevent a library update.
Download-record coverage verifies actual attachment bytes, successful/failed/
close-cancelled metadata across restart, pre-response failures, private no-records,
vanished-parent write failure, incomplete identity responses, missing-file reveal
rejection and native clear Cancel/Confirm paths without deleting files. Retention
updates the saved-record UI; damaged/future download records do not block browsing
and remain untouched on close. The isolated runner also exercises real destination
permission denial, successful native Retry after restoring permissions, and native
WebKit disk-full failure on a private 1-MiB tmpfs, checking visible errors and
persisted failure metadata. See [download failure coverage](docs/DOWNLOAD_FAILURES.md)
for reproduction and limits. Permission coverage checks
actual geolocation denial and the native explanation. Native automation consent
fixtures drive Grant/Deny and per-action Cancel/Allow/Stop, check target disclosure,
expiry/generation rejection and actual origin-filter compilation without public
navigation. The owned HTTPS smoke checks native input, TLS certificate rejection,
foreign-origin denial, unchanged-geometry label changes, PNG and process cleanup.
The programmatic smoke test ignores unrelated native desktop keypresses so they
cannot contaminate address drafts; it does not claim to test physical shortcuts.

`PAISLEY_SCREENSHOT=/absolute/path.png` captures the test browser window;
`PAISLEY_LIBRARY_SCREENSHOT=/absolute/path.png` captures the filtered history manager;
`PAISLEY_DOWNLOADS_SCREENSHOT=/absolute/path.png` captures restored download records.
`PAISLEY_GRANT_SCREENSHOT` and `PAISLEY_CONSENT_SCREENSHOT` capture native automation
consent windows; use new absolute PNG paths.
Optional `PAISLEY_LIVE_DUCKDUCKGO=1` and `PAISLEY_LIVE_DUCKAI=1` add public-site
checks; they require network access and do not submit chat messages.

## Source map

- `src/main.rs` — CLI, profile identity and application activation/errors.
- `src/mcp.rs`, `assets/mcp.py` — embedded isolated stdio adapter and fixed-origin relay.
- `src/automation_runtime.rs` — ephemeral native UI, origin filter, authenticated consent, expiry/Stop.
- `tests/test_mcp.py`, `tests/mcp_smoke.py`, `tests/mcp_https_smoke.py` — protocol/boundary tests and owned HTTP/HTTPS native-input fixtures.
- `src/browser.rs` — tabs, navigation, UI/actions, engine integration.
- `src/profile.rs` — settings, locks and bounded atomic profile persistence.
- `src/session_store.rs` — bounded background session/library/download-record writer and final flush.
- `tests/persistence_crash.rs`, `docs/RECOVERY.md` — owned process-kill metadata recovery and explicit durability limits.
- `src/library.rs`, `src/library_ui.rs` — bounded profile bookmarks/history and native managers.
- `src/profile_catalog.rs` — bounded profile discovery and closed-profile name/deletion maintenance.
- `src/settings_ui.rs`, `src/settings_transfer.rs` — searchable settings, confirmed draft reset and typed preferences transfer.
- `src/bookmark_transfer.rs`, `src/transfer_io.rs` — bounded bookmark interchange and shared explicit local-file handling.
- `scripts/smoke_isolated.py` — native GTK regression runner with disposable profiles, private network and bounded disk-full fixture.
- `src/downloads.rs` — download UI, destination validation and explicit native reveal.
- `src/download_history.rs` — bounded terminal metadata, without restart execution/resumption.
- `src/memory.rs` — Linux-visible process-tree PSS/RSS sampling.
- `src/blocking.rs`, `assets/blocking.json` — native starter request filters.
- `src/navigation.rs` — address/search normalization and document policy.
- `src/site_styles.rs`, `assets/duckduckgo.css` — scoped cosmetic promotion cleanup.
- `src/browser_tests.rs`, `tests/fixtures/` — real-engine integration fixtures.
- `src/browser_benchmarks.rs`, `docs/PERFORMANCE.md` — opt-in test-only release measurements and their limits.
- `scripts/package.py`, `tests/test_package.py`, `packaging/`, `docs/PACKAGING.md` — reproducible unsigned local-development archive tooling, tests and launcher.
- `assets/welcome.html` — original unused local welcome asset.

## License

Paisley is made available under the MIT License. See [LICENSE](LICENSE).
