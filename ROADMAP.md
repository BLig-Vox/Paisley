# Browny: product goal and roadmap

Date: 2026-11-07

Status: implementation in progress. Interactive MCP now supports an explicitly selected HTTPS origin with native grants, or owned loopback fixtures, inside an isolated ephemeral workspace. Verification remains owned-fixture-only; the overall roadmap and unrestricted website automation are not complete.

## North star

Build a fast, Linux-first Rust browser combining **LibreWolf-inspired privacy
and configuration**, **Brave-inspired built-in ad/tracker blocking**, complete
web-development tools, persistent profiles and sessions, and **visible,
permission-controlled, provider-neutral agent automation through MCP**.

A daily browser first; a developer and agent workspace without giving up user
control. Normal browsing should remember the user's work. Private browsing should
not quietly become persistent. Automation should be observable, interruptible,
and isolated from personal browsing unless explicitly authorized.

“LibreWolf + Brave” describes the ambition, not verified feature or security
parity. This roadmap does not claim Browny is already production-hardened.

## Original baseline

Before roadmap execution, the prototype had Rust/GTK4/WebKitGTK, independent tabs
and histories, DuckDuckGo home/search, navigation controls, per-tab errors, and a
scoped cosmetic stylesheet hiding DuckDuckGo promotions. It used ephemeral site
storage and canceled downloads. The implementation below supersedes that baseline.

## Implementation progress — 2026-11-07

**No complete milestone is claimed.** Several browser foundations are implemented
and regression-tested, but the remaining exit criteria still apply.

| Area | Implemented evidence | Remaining before milestone completion |
| --- | --- | --- |
| Feasibility/baseline | Native Inspector, persistent site storage, compiled filters and visible-process memory exercised on GTK 4.24.1 / WebKitGTK 2.54.2. | Full capability/threat matrix, release measurements, reliable interactive capture/input and process-ownership validation. |
| Developer tools/memory | Real mapped Inspector for the active tab; developer extras enabled. Three native attach/detach/toggle cycles verify owning/detached roots and close signals. Detached tab/window teardown regression found and fixed missing window-close cleanup; retained-view owner-close now tested. [Evidence and API limits](docs/INSPECTOR.md). Off-thread PSS/RSS sampling appears in title/status, labeled approximate and partial; display toggle tested. | Exercise DOM editing, console/debugger/panel workflows and longer lifecycle/resource-leak stress; aggregate multiple profile runtimes and validate namespace coverage. The supported API has no direct Console selection method; shortcut toggles Inspector. |
| Persistent foundation | Profile CLI/switching and bounded discovery catalog; closed-profile display-name rename and native confirmed deletion, stable IDs/paths, root maintenance plus per-profile locks. Private no-write, supported searchable settings with confirmed reset and bounded typed draft import/export, atomic background snapshots and lazy restore. Native non-restorable placeholders retain transient-tab position/selection without saving payloads or replaying documents ([evidence](docs/SESSION_RESTORE.md)). Actual owned writer-process SIGKILL covers whole metadata snapshots and lock release ([limits](docs/RECOVERY.md)). Bounded bookmarks/searchable history and native managers; strict additive bookmark JSON import/export with native confirmation, no history loss and temporary private imports ([details](docs/BOOKMARK_TRANSFER.md)). Fixtures cover restart storage/tabs/library, profile isolation, private/history opt-out, catalog rename/delete Cancel/Confirm, settings draft/Save/reset/search, nested-root deletion refusal and nonblocking metadata. [Details](docs/PROFILE_SETTINGS.md). | Migrations/recovery beyond the verified metadata SIGKILL subset, large-tree maintenance responsiveness, full native chooser/portal and private-window UI matrices, expanded settings categories, library folders/history transfer and cross-browser bookmark HTML interchange. A prior same-process localStorage `null` observation remains recorded. Subsequent investigation reproduced the WebKit NetworkProcess SIGSEGV when native test fixtures deleted watched profile/cache roots during teardown; all cycles now retain their fixture roots until isolated namespace exit. The updated stress/full suite no longer observed that crash, but production-profile close/reopen and real closed-profile deletion remain to be verified. Corrupt/future records are preserved/rejected, not repaired. Independent review of the new management/transfer UI remains open. |
| Downloads | Destination chooser, sanitized names, no-overwrite policy, progress/cancel/retry and terminal states. Bounded per-profile terminal metadata with private no-records and confirmed metadata-only clearing; explicit native reveal with missing-file/symlink guards. Real fixtures cover matching bytes, persisted completed/failed/close-cancelled outcomes, pre-response failure, vanished-parent write failure, identity-length truncation, retention/UI consistency and late destination rejection. Unreadable/future download metadata is preserved with visible memory-only degradation, not whole-profile lockout. Isolated fixtures additionally verify actual EACCES at destination preparation, native ENOSPC on a bounded tmpfs, visible failure/no Reveal, persisted Failed outcomes and successful native Retry after restoring directory permissions. [Failure matrix](docs/DOWNLOAD_FAILURES.md). | Successful external file-manager/portal matrix, broader permission/filesystem combinations, crash recovery of active transfers and capability-gated resume. Partial files may remain; same-user filesystem races are not a security boundary. |
| Privacy/blocking | ITP enabled, optional Strict WebRTC/third-party-cookie controls, twelve-domain native starter rules and exact-origin exceptions. Server counters verify blocking, related views, cross-origin navigation and history back/forward. Permission signals explicitly denied with a reason; real geolocation fixture receives PERMISSION_DENIED. [Capability matrix](docs/CAPABILITIES.md) records evidence and gaps. | Safe requesting-origin attribution blocks per-origin permission prompts/grants/revocation; never trust the top-level URL as the requesting frame. Broader privacy behavior, maintained lists/updates/rollback, diagnostics and full security review remain. No Brave/LibreWolf parity. |
| Interactive visual MCP | Opt-in stdio and ephemeral fixture workspace inside mandatory namespaces; fixed HTTP relay or pinned exact-authority HTTPS tunnel, native exact-origin filter, trusted cursor/actions and Stop. Owned HTTP/HTTPS fixtures verify native Unicode/click/key/wheel/PNG, scope/staleness/foreign-request denial, wrong-CA rejection and cleanup. | The user now requests a Playwright-like headless page surface that can use an explicitly selected existing Browny profile/session (including site sign-ins) or Incognito, with a trusted visible indicator of the selected target and Stop. This is not implemented. Existing MCP remains fixture-only. Headless rendering/input, safe attachment to the profile-owning process, per-session grant/revoke, live-account policy and independent client interoperability need design and fixture validation; no live Google session has been accessed. See [MCP setup/boundaries](docs/MCP.md). |
| Hardening/release | Headless suites, isolated HTTP/HTTPS MCP fixtures, strict Clippy and debug/release builds pass. Test-only owned-fixture release measurements cover fresh/restored in-process readiness, 1/10/30 tabs, partial PSS/CPU, native snapshots/PNG and filter request counters. Reproducible unsigned local archives and prefix-correct desktop launchers are tested and payload hashes verified. See [performance evidence](docs/PERFORMANCE.md) and [local packaging](docs/PACKAGING.md). | Representative/standalone cold-warm benchmarks, incremental CPU and agreed budgets, intermittent readiness investigation, third-party notices/dependency-license review, signing/update policy and redistributable packaging; accessibility, broader Wayland/X11/scale matrix, recovery tests and release security review. MIT was selected for the project source on 2026-11-08. |

HTTPS-slice verification before the release-evidence additions (host tools through `distrobox-host-exec`):

```sh
cargo fmt --check
cargo test --offline --locked
cargo test --offline --locked --bin browny browser_smoke -- --ignored --test-threads=1
cargo clippy --offline --locked --all-targets -- -D warnings
cargo build --offline --locked
python3 -I tests/test_mcp.py
python3 -I tests/mcp_smoke.py /absolute/path/to/target/debug/browny
python3 -I tests/mcp_https_smoke.py /absolute/path/to/target/debug/browny
```

Results: **38 Rust headless tests and 20 Python MCP tests passed** (28 library,
4 download helpers, 2 background-writer and 4 native automation regressions).
Formatting, strict all-target Clippy and debug build passed. The separately invoked
GTK smoke, including native consent buttons and real filter compilation, passed
with `G_DEBUG=fatal-criticals` (12.75s). The ordinary Rust run intentionally ignores
that graphical test. HTTP and HTTPS real-binary MCP smokes both passed; no public
site or personal profile was used. HTTPS certificate rejection occurs before the
owned server's HTTP handler; same-geometry label mutation invalidates input.

Native workspace-grant, per-action consent and actual HTTPS submitted-Unicode/
trusted-event PNGs were captured and visually checked. Earlier filtered-history
and download-manager captures were also checked. Two nonfatal WebKit credential-
socket broken-pipe warnings occurred in the final graphical run; their cause is
not established. The earlier unfinished GLib task critical was reproduced under
the debugger and resolved by cancellation/completion-aware filter lifetime and
GTK test teardown. No engine sandbox default was weakened. These fixtures do not
prove physical desktop preemption, complete race safety, two-client interoperability,
a full security audit or measured performance budgets.

### Native transient-session placeholders — 2026-11-07

Blob/data/about temporary documents now save a fixed metadata-only marker, not
payloads, original transient addresses/fragments or titles. Normalization occurs
before background snapshot queuing and during persisted validation. Restoring the
marker preserves tab order/selection and displays a native **Not restored** label,
without loading the original document or another website. Address is empty; Reload
is disabled and its action is a no-op. Explicit web navigation or Home replaces the
placeholder. Private sessions still write nothing; the marker grants no navigation
or origin authority. Unsafe file/JavaScript/credential-bearing entries remain rejected.

The model and native regressions failed before implementation, then passed. They
cover oversized transient-payload omission, lazy safe backgrounds, repeated restore,
empty history, no-op Reload, marker address-bar rejection, explicit replacement and
an actual data document becoming a payload-free queued/persisted snapshot. The native
1280 × 800 screenshot was read and visually checked. Fresh **66 Rust headless tests**,
**27 Python tests**, Clippy, formatting and debug/release builds passed; the full
isolated GTK test passed in **24.03s**, but still emitted a WebKit internal-load error
and credential-socket warnings. No storage/engine reliability fix is claimed. Actual
HTTP and HTTPS MCP smokes subsequently passed independently on this build, once each
without retry, with native input, PNG, denial and Stop/cleanup. Earlier failures remain
recorded and unresolved; these passes do not establish their cause.
[Format, downgrade limits, reproduction and remaining work](docs/SESSION_RESTORE.md).

### WebKit cache-callback SIGSEGV and fixture teardown — 2026-11-07

A private isolated eight-cycle run captured an owned NetworkProcess **SIGSEGV**. The
approved local core resolves the callback stack to `WTF::String::isolatedCopy()` →
`NetworkCache::Storage::versionPath()` → `NetworkCache::dumpFileChanged()` → GLib
file-monitor dispatch, with invalid access at `0x63`. Paired owned-fixture runs
reproduced the crash when test code removed profile/cache roots immediately after
closing a Browser and observed no NetworkProcess exit when retaining those roots
through process exit. Native integration fixtures now defer deletion; the mandatory
isolated runner's disposable namespace removes them at exit. A red/green teardown
regression, eight-cycle stress and full GTK suite pass without observed crash or
internal-load errors after the change. A fresh final pair passed stress (7.73s) and
the full GTK suite (24.66s); final checks include **71 Rust headless tests**, 27 Python
tests, Clippy, formatting and debug/release builds. Credential-socket warnings remain.
This establishes a strong test-fixture lifetime cause, not a WebKit library fix or a
production daily-use guarantee. Earlier
localStorage loss remains a separate unproven observation; profile close/reopen,
profile deletion and broader engine/display/release validation remain open. Exact
core provenance, stack, paired evidence and limits: [restart investigation](docs/RESTART_RELIABILITY.md).

### Metadata crash coverage and bounded automation diagnostics — 2026-11-07

A new headless integration fixture uses actual SIGKILL against owned profile-writer
processes: three observed-staging interruptions and one acknowledged-commit
interruption. Reopening must acquire the released lock, recover a whole valid
200-tab snapshot and preserve settings bytes. A temporary non-atomic-writer mutation
made the test fail on the recovered snapshot; it was removed and both binaries
rebuilt. Existing production atomic persistence passed; no recovery fix or power-loss
simulation is claimed. [Exact cases and limits](docs/RECOVERY.md).

Native automation now emits one fixed, bounded first-Stop cause code on stderr,
without tokens, origins, key identities, page contents or typed values. The new
cause-code regression failed on its stub then passed after implementation. An
instrumented HTTP native-input fixture passed. The next HTTPS run failed before
navigation with **`native-key-preemption`**, identifying the revocation handler for
that run—not the key's source or the earlier uninstrumented failures. Keyboard
preemption remains enabled; no restart/retry hides revocation. The full GTK test
passed in 25.31s with native consent Stop/Deny cause codes, but also printed a WebKit
internal-load error. [Diagnostic semantics and evidence](docs/MCP.md#bounded-revocation-diagnostics).

Fresh headless checks: **65 Rust tests and 27 Python tests passed**, strict Clippy,
formatting and debug/release builds. Native storage/Inspector reliability, complete
recovery and the broader compatibility/review/release gates remain open.

### Native Inspector lifecycle and shutdown fix — 2026-11-07

A retained-WebView regression reproduced a detached Inspector remaining open after
its owning browser closed. Window shutdown now closes every tab's Inspector,
matching individual-tab cleanup. The regression failed before the fix (22.59s)
and passed afterward (2.62s). Native tests also assert three dock/detach/toggle
cycles using actual mapping/allocation/window roots, active-tab ownership, native
close signals and private no-profile-write. The old visibility-only check was
insufficient; fixture lifetime/readiness assumptions were corrected, not presented
as production fixes.

A full isolated GTK smoke passed in 34.71s after the browser fix; 63 Rust and 27
Python headless tests, Clippy, formatting and debug/release builds passed. The latest
Inspector investigation reproduced the intermittent boundary: focused lifecycle passed
in 2.80s, but a complete GTK run failed in 21.35s waiting for mapping on the second
open/close cycle. Test-only mapping diagnostics were added; a later complete GTK run
passed in 22.87s and included profile isolation and post-exit catalog deletion. The new
`--inspector-preface` mode then reproduced a different timeout after 25.57s: the reopened
frontend was mapped/rooted but stayed at 0×0. A subsequent preface run passed in 9.01s,
with a temporary 0×0 mapping that later allocated normally. The newest preface run failed
in 21.40s on the second open: the frontend remained unmapped/unrooted at 0×0 despite
`is_attached=true`. The minimal `--inspector-open-close` mode then failed in 22.26s on
its second `show()` without any attach/detach calls; the frontend again had no parent.
This rules out those explicit transitions as necessary to trigger the observed failure,
but does not distinguish WebKitGTK, GTK, compositor or fixture causes. Installed engine
is WebKitGTK 2.54.2. WebKit bug [288189](https://bugs.webkit.org/show_bug.cgi?id=288189)
is a related-but-unconfirmed Inspector attachment lead; its fixed nightly has no
verified released-version mapping here. These passes do not erase any failure; no
production behavior or assertion was weakened. Actual HTTPS native-input MCP
passed. Actual HTTP MCP **failed** initial navigation with
`Workspace stopped or control channel lost`; no retry hid it. Inspector DOM edits,
console evaluation, breakpoints/stepping, rendered panel evidence and broader
lifecycle/resource-leak coverage remain unverified. [Reproduction and failures](docs/INSPECTOR.md).
These results do not resolve the separate storage/NetworkProcess failures below.

### Restart reliability investigation — 2026-11-07

The following two entries are historical diagnostic snapshots; the later cache-callback
section above supersedes their open core-analysis status and narrows the repeatable
crash to fixture teardown.

At the first passive-sampling snapshot, the tool establishes **SIGSEGV (11)**
for the owned NetworkProcess in an eight-cycle run (9.07s; assertions passed despite
the native crash). A matching system core record confirms the exact owned host/PID
namespace identity. Core-memory export and local analysis were subsequently approved
and completed; the result is recorded in [restart evidence](docs/RESTART_RELIABILITY.md). Four diagnostics
regressions include a real owned SIGKILL child and mandatory fixture-scope checks.
At that earlier snapshot **70 Rust headless tests**, **27 Python tests**, Clippy and
formatting passed; ordinary GTK passed in **23.90s**, still with a native internal-load
error. Failed GDB tracing experiments were removed rather than shipped as a working
mode. [Updated core and controlled teardown evidence](docs/RESTART_RELIABILITY.md#passive-terminal-status-capture--2026-11-07).

Post-placeholder stress exposed a separate fixture defect: every cycle reused the
same directory, which reappeared after cleanup; cycle 6 failed the fresh-root
assertion (4.24s). Each cycle now uses a distinct owned root, without changing any
storage/isolation assertions. A subsequent eight-cycle run completed those assertions
in 8.12s **but still explicitly logged a NetworkProcess crash/relaunch in cycle 7**.
That corrects fixture path reuse, not engine reliability. No retry/sleep or production
storage fix was introduced. [Logs and interpretation](docs/RESTART_RELIABILITY.md#distinct-stress-roots-and-retained-engine-crash--2026-11-07).

At that earlier snapshot, bounded same-process restart stress reproduced localStorage
reading `null` instead of the seeded value and native fixture-load timeouts after a
WebKit internal-load error. The later controlled fixture-teardown comparison above
explains the repeatable cache-callback crash but does not yet prove the storage-value
observation shares that cause.
Native weak references were released; one eight-cycle run had zero retained GTK/
application windows and stable pre-cycle visible process counts. A growing UI-object
leak was not demonstrated. A later engine-logged stress run failed in 24.73s and
explicitly reported `NetworkProcessProxy::didClose (Network Process crash)`, then
replacement startup and a main-frame load failure. Sampled filesystem/descriptor
capacity and cgroup events did not demonstrate resource exhaustion. Later core analysis
identified SIGSEGV in a NetworkCache file-monitor callback; the controlled root-retention
runs implicate test teardown. This did not establish a relation to localStorage loss.

A new isolated **separate-test-process** mode passed four seed/check/isolation
triples (4.04s), verifying a localStorage value/cookie across actual process exit and
that another owned profile cannot read them. It is also included in the ordinary
GTK test; it supplements rather than removes existing assertions. This is the
shared Browser implementation, not proof of the production CLI or crash recovery.

A complete GTK smoke failed after 24.71s, waiting for an allowed tracker after a
site exception, with the same WebKit internal-load message. At that investigation
snapshot, a complete isolated run passed in **30.35s**, including four separate-process triples and actual
download permission/retry/disk-full cases, without a production fix. Fresh checks
also passed 63 Rust headless tests, 27 Python tests, Clippy and formatting. These
do not resolve the intermittent failures or turn the earlier failed runs green.
No automatic retry, arbitrary sleep, security relaxation, host package or engine
change was introduced. [Reproduction, outcomes and open work](docs/RESTART_RELIABILITY.md).

### Previous bookmark transfer integration — 2026-11-07

Strict, bounded version-1 bookmark JSON import/export is implemented, with native
Cancel-default import confirmation, additive current-state merge, preserved history
and existing bookmarks, source-order additions, no page opening, and explicit
sensitive-export disclosure. Both private and normal native fixtures cover changes
while consent is pending, capacity refusal without mutation, literal title display,
normal persistence and private no-profile-write. Manager/owner close regressions
reproduced retained dialogs/late export and gained owner-bound cancellation guards.
Shared no-follow/nonblocking bounded file I/O is reused by settings and bookmarks.

Fresh checks: **63 Rust headless tests and 27 Python tests passed**, strict Clippy,
formatting and debug/release builds. The complete isolated GTK regression passed
in **18.13s** with fatal criticals, including real disk-full and permission-retry
cases; new native bookmark/disclosure captures were visually read. Actual HTTP and
HTTPS MCP smokes passed on the subsequent run, with real native input retained.

Two independent reliability failures remain open: one immediate-restart site-data
fixture read localStorage as `null` instead of `work`, and one HTTP MCP second-workspace
startup returned `WebDriver: unknown error`. Later unchanged runs passed; neither
cause is established or claimed fixed. Native chooser/portal selection, folder/HTML
interchange, independent review and wider release criteria remain open. See
[behavior, reproduction and failures](docs/BOOKMARK_TRANSFER.md) and the
[execution ledger](docs/superpowers/plans/2026-11-07-bookmark-transfer.md).

### Previous profile/settings integration — 2026-11-07

**56 Rust headless tests and 27 Python tests passed**, with strict all-target
Clippy, debug/release builds and formatting. The complete GTK smoke passed in a
private no-public-network namespace (30.78s, fatal GLib criticals enabled), including
real reset and delete Cancel/Confirm controls, visible settings/catalog widgets,
draft import/export handlers, explicit Save and close-without-Save. Native settings
and renamed-profile PNGs were read and visually checked. The Save footer gained
a failing-then-passing allocation regression after an initial screenshot showed it
below the window. Actual isolated HTTP and HTTPS MCP smokes passed again.

Catalog regressions also reproduced and fixed nested-root cross-profile deletion,
FIFO metadata blocking, missing reopened display names, and startup/catalog
operations ignoring maintenance. The root maintenance lock is outside the deletable
profile directory and coordinates current-version writers; it is not a hostile
same-user or mixed-version boundary. Name metadata is bounded/versioned and preserved
on corruption/future versions. Import/reset stay draft-only, and exports omit site
identifiers, paths and secrets rather than claiming full backup.

See [profile/settings behavior, test commands and limits](docs/PROFILE_SETTINGS.md)
and the [execution ledger](docs/superpowers/plans/2026-11-07-profile-settings-management.md).
Native chooser/portal end-to-end selection, expanded settings, large-tree/crash
recovery and independent review remain open; these tests do not complete every
persistent-foundation or release exit criterion. Earlier performance measurements
below predate this integration and are not a freshly remeasured current build.

### Supplemental release evidence — 2026-11-07

Fresh checks after packaging/benchmark integration: **43 Rust headless tests and
27 Python tests passed** (20 MCP, 7 packaging). Formatting, explicit benchmark-file
rustfmt, strict all-target Clippy, and debug/release builds passed. Actual isolated
HTTP and HTTPS MCP smokes were rerun and passed. The opt-in real GTK release
benchmark passed in 81.09 seconds with `G_DEBUG=fatal-criticals`; its window and
WebKit page PNGs were visually read. The complete ordinary graphical regression
branch also passed (13.51s) inside a separate mandatory user/network/PID/mount
namespace with disposable HOME/runtime, read-only executable/toolchain, private
service-free D-Bus and only the Wayland socket shared. Its DuckDuckGo-default
assertions could not contact the public network. Rendering was Cairo/software;
accessibility services were intentionally absent, so this is not that matrix.

The baseline measured fresh/restored **in-process** readiness p95 of 225.86/172.56 ms,
10/30-tab dispatch p95 of 0.78/1.16 ms, native capture/PNG p95 of 11.52/23.21 ms,
and partial PSS of 278.0/753.5/1,418.3 MiB at 1/10/30 loaded tiny local tabs. One-tab
selection is explicitly a no-op baseline, not tab-switch performance. Partial owned
CPU including harness/fixture overhead was **1.75% of one core over 60.018 seconds**;
this does not validate the under-1% incremental-monitoring goal. It is not standalone
startup, cold cache, compositor-presented latency or a production workload/budget.
Filter server counts were 10/10/0 with disabled/nonmatching-starter/synthetic-matching
filters. [PERFORMANCE.md](docs/PERFORMANCE.md) records raw-evidence locations,
conditions, methodological limits, a prior readiness timeout, and the unestablished
credential-socket warning. A later green run does not erase the timeout.

Two local release archives were byte-identical, with four regular members and
verified SHA-256 payloads. The generated `/opt/browny/bin/browny` desktop launcher
passed host validation; nothing was installed or published. Missing license,
dependency notices/provenance, signing and update policy still block a redistributable
release. See [PACKAGING.md](docs/PACKAGING.md) and the
[release-evidence ledger](docs/superpowers/plans/2026-11-07-release-evidence.md).

### Interactive automation boundary and approval

The user explicitly selected **“Approve local backend”**, authorizing implementation
and bounded local-fixture tests after disclosure of the proposed service's risk.
A source review still raised concerns about a host-visible unauthenticated general
WebDriver endpoint. That design was **not** implemented. The implemented alternative
is materially narrower, not a relabeled loopback listener:

- Inherited stdio MCP transport; WebDriver/browser confined to private user,
  network, PID, IPC and mount namespaces. No host TCP debugger listener.
- Read-only trusted executable/toolchain, disposable HOME/cache, private D-Bus,
  no personal profiles or host session bus; only the required Wayland display
  socket crosses for the visible GUI.
- A bounded-body HTTP relay forwards only the explicitly selected fixture origin.
  The subsequently approved HTTPS extension adds exact-authority CONNECT to pinned
  validated peers, not an unrestricted tunnel. Other destinations fail closed.
- Fixed browser capabilities/binary; no arbitrary JavaScript, shell, raw cookie,
  general filesystem, upload/download or desktop-control tools.
- Token-authenticated private control for display/status/Stop and requesting a
  native authorization decision, never supplying one. Native input is delivered
  by WebKit, not DOM `click()`/value assignment.

The real fixture and process checks pass. Native key actions preserve supplementary
Unicode where the installed engine's element-send-keys path dropped emoji. Observe
returns a rendered PNG and single-use context/layout-checked input authorization.
The user then explicitly selected **“Approve HTTPS-origin support”**, authorizing
design/code changes and owned HTTPS-fixture tests, not live-site actions or personal
profiles. The extension keeps engine TLS validation, fixed binary and namespaces;
adds public-peer pinning, scoped public CA trust, a native origin filter, native
workspace Grant/Deny/expiry, and single-use input/navigation consent. Targets and
bounded page/layout state are rechecked after approval; same-geometry changes,
idle control and pending-consent EOF gained regressions. This is not a guarantee
that arbitrary clicks are safe or that third-party-dependent sites work.

Verification proves an **interactive owned HTTP/HTTPS fixture slice plus native
consent policy**, not complete arbitrary-site compatibility, an atomic race-free
action boundary or personal-tab authorization. No less-isolated fallback is allowed.
See [docs/MCP.md](docs/MCP.md) for exact quotas, client setup, disclosure and gaps.

### Known capability limits

- Hyperlink-auditing and DNS-prefetch setters on the installed engine are
  deprecated/ineffective; they are not exposed as functioning privacy protections.
- Memory covers only the Linux-visible owned process tree. Namespace/reparenting
  coverage and multi-profile aggregation remain unproven.
- Profile persistence is not encrypted storage or a boundary against hostile
  processes running as the same user. Private mode is not anonymity/no traces.
- The native starter rules are separate from DuckDuckGo cosmetic cleanup. No list
  update service, exact blocked-request counter or WebExtension compatibility exists.

The [execution plan](docs/superpowers/plans/2026-11-07-daily-browser.md) records
implementation and verification details. Older `docs/superpowers/` browser
documents describe the original prototype, not the current roadmap specification.

## Foundation and architectural choices

### Recommended approach

Keep **Rust + GTK4 + WebKitGTK**. Use WebKit's Inspector, network sessions,
website-data APIs, download APIs, snapshots, and native content filters where
appropriate. Rust owns browser policy, profiles, settings, persistence, UX, and
the agent permission boundary. WebKit still owns rendering and JavaScript.

Before substantial feature work, validate the capabilities that could change
this choice: privacy controls, reliable visual capture, real input delivery,
profile isolation, blocking coverage, and debugging APIs. A failed mandatory
capability requires an explicit product/engine decision, not a silent downgrade.

### Alternatives considered

- **Keep WebKitGTK (recommended):** builds on the working native Linux app and
  avoids owning a browser-engine fork. Engine-specific privacy and automation
  limitations must remain visible.
- **Change to a Chromium embedding stack:** worth reconsidering if mandatory
  debugging or automation requirements cannot be met. Requires revalidation of
  the integration, packaging, updates, resource use, and security model; it does
  not automatically inherit Brave's protections.
- **Maintain a Gecko/Chromium fork:** closer access to engine-specific defenses,
  but a substantially larger project with continuous upstream security-merging
  obligations. Not the proposed scope of this Rust application.

### Proposed defaults, not previously approved implementation decisions

- Standard profiles persist cookies/site data and restore tabs by default.
- Private mode uses ephemeral website storage and never writes browser-managed
  history or restore records. This is not a no-traces/anonymity guarantee.
- One application process/runtime per standard profile, with its own persistent
  WebKit session and explicit data/cache directories. Switching activates or
  opens that profile's window; never retarget live WebViews to another profile.
- Fixture/untrusted-site automation defaults to a disposable dedicated profile.
  New requested mode: before each session, the user explicitly selects one Browny
  profile with its existing WebKit sign-ins, or Incognito. Show that choice and the
  connected client in trusted UI before control begins and throughout the session.
  The page surface should be headless (not shown), while a native trusted status/
  Stop surface remains visible. Incognito uses ephemeral site storage. Never clone
  cookies, import/export raw tokens, or silently switch targets. This mode is not
  implemented; the currently approved backend remains isolated fixture-only. External
  browser profiles and live account testing require separate scoped authorization.
- MCP is off by default and local-first. Once the user enables an automation
  workspace, agents can click, type arbitrary input, press keys, scroll, navigate,
  and inspect pages. Observation-only access is an optional permission level,
  not the intended product or a substitute for interactive automation. No
  required cloud/model subscription and no browser-owned LLM loop.
- Privacy protections and built-in blocking are on by default once implemented
  and verified. No telemetry or unsolicited search suggestions by default.
- Performance is measured throughout development, not postponed to the end.

Separate profiles prevent accidental storage mixing; they are not an OS security
boundary against malware or another process running as the same user. Additional
sandboxing is a separate, tested hardening measure.

## Product requirements

### Developer tools: real Inspect Element and debugging

- Integrate WebKit Web Inspector, rather than building a fake console.
- Right-click Inspect Element; F12 / Ctrl+Shift+I toggle tools;
  Ctrl+Shift+J opens the console when the supported engine API permits it.
- Support docked and detached tools, clearly bound to the inspected tab.
- Target DOM/CSS inspection and editing, console evaluation/errors, network
  requests, JavaScript breakpoints and stepping, storage inspection, and
  performance diagnostics through the available Inspector panels.
- Document panel/API gaps on the supported WebKit version. Do not equate this
  with Chrome DevTools Protocol compatibility or native Rust crash debugging.
- Remote inspection and privileged agent evaluation stay separately gated.

### Downloads

- Save files through a downloads UI with destination choice, progress, received
  bytes, completion/failure state, cancel, retry, and reveal-in-folder.
- Handle attachments and unsupported renderable MIME types through downloads
  instead of the current unconditional cancellation.
- Validate server filenames and paths, avoid traversal/symlink overwrite, ask
  before replacing an existing file, and keep incomplete files distinguishable.
- Never execute or auto-open downloaded content. Opening a completed file is a
  separate user action. Do not claim malware scanning unless actually integrated.
- Persist download metadata in normal profiles, not private download history.
  Explicitly saved files survive private mode; say so in the UI.
- Pause/resume and restart-resume are capability-gated follow-ups. If the engine
  and server cannot resume safely, show retry rather than a misleading control.

### Profiles, sessions, and settings

- Create, rename, identify, switch, and delete profiles with clear confirmation
  for destructive deletion and a visible current-profile indicator.
- Isolate cookies, website storage, permissions, history, bookmarks, settings,
  restore records, download metadata, and blocking exceptions by profile.
- Use XDG configuration/data/cache locations, restricted directory permissions,
  stable internal profile IDs, profile locks, and versioned storage migrations.
- Restore normal-profile windows, tab order, active tab, pinned tabs when added,
  and safe URLs after clean restart; provide bounded crash recovery.
- Persist session metadata atomically, with debounced writes and recovery from
  interrupted/corrupt state. Never restore POST submissions automatically.
- Do not save passwords, form drafts, page contents, or sensitive URL credentials
  in restore records. URLs may themselves be sensitive; explain and offer clear/
  disable controls. Blob/data documents need a safe non-restorable placeholder.
- Website login survival depends on cookie expiration and website policies;
  reopening tabs cannot guarantee every website keeps the user signed in.
- Start with lazy restoration: load the selected tab; defer background network
  requests until activation, with explicit user controls for other behavior.
- Settings categories: startup/session, profiles, privacy/security, blocking,
  website permissions/data, downloads, appearance/accessibility, search,
  performance, developer tools, and automation.
- Each setting has a typed value, default, validation, scope, and restart-needed
  status. Offer searchable advanced settings and reset/export/import, excluding
  secrets by default. Unknown or unsupported options must not look effective.
- Add history/bookmarks management as daily-browser essentials. Password-manager
  integration and encrypted sync are later separately designed work, not implied
  by persistent profiles. Persistent data is not automatically encrypted at rest.

### Privacy, security, and blocking

- Preserve TLS validation, origin enforcement, and WebKit process/sandbox
  protections. No global certificate bypass or disabled sandbox as a feature fix.
- Establish an evidence-backed capability matrix for HTTPS-only navigation,
  tracking prevention, third-party storage/cookie handling, WebRTC exposure,
  referrers, permission controls, GPC/DNT signals, fingerprinting surfaces,
  DNS/proxy behavior, mixed content, and certificate handling.
- Classify every protection as implemented and tested, partially supported,
  unavailable, or requiring engine work. Sending GPC/DNT is a signal, not proof
  that a site stops tracking. DNS/proxy claims must state OS/engine dependencies.
- Provide per-origin permission prompts and revocation for camera, microphone,
  geolocation, notifications, clipboard, and other supported sensitive APIs.
  Unsupported privileged requests fail closed with an explanation.
- Standard and Strict privacy presets expose compatibility tradeoffs and
  per-site exceptions. Neither a preset nor a collection of spoofing scripts
  establishes LibreWolf-style anti-fingerprinting or anonymity.
- Block ad/tracker **requests**, not only visible elements. Use native WebKit
  content filters plus cosmetic rules where appropriate, subject to measured
  engine coverage. No extension-compatibility claim.
- Evaluate maintained, license-compatible list sources such as EasyList and
  EasyPrivacy. They are not directly interchangeable with WebKit JSON filters
  or the full uBlock Origin rule language: validate translation/coverage first.
- Show a protection panel, per-site toggle, exceptions, rule/list versions, and
  a breakage-report path. Counts must describe an observable metric; do not
  invent exact blocked-request counts from CSS or unsupported engine events.
- Treat list updates as untrusted data: HTTPS, provenance/integrity appropriate
  to the source, size/complexity limits, validation, atomic activation, rollback,
  and a last-known-good cache. Lists must never install executable code. Expose
  update destinations/frequency and permit disabling background updates.
- Publish the threat model, protection gaps, supported engine versions, security
  update process, and vulnerability reporting policy before a daily-use release.
  The old build floor is not a promise that an unpatched engine is safe.

### RAM title and performance

Example title: `Page title — Browny · Work · RAM ≈ 842 MiB`.

- Show application-attributable memory, including UI, web, network, and other
  owned subprocesses—not just the Rust shell and never every WebKit process on
  the desktop. Aggregate open profile runtimes once, without double counting.
- Prefer Linux proportional set size (PSS) when available; label summed RSS as
  approximate because shared pages can be counted more than once. Offer details
  showing the metric, included processes, timestamp, and unavailable readings.
- Validate ownership discovery across sandbox/PID namespace boundaries and
  process churn. Use a launcher/application cgroup when available; do not weaken
  sandboxing just to obtain metrics. Show partial/unavailable coverage honestly.
- Sample inexpensive metrics around every two seconds; collect expensive PSS
  less frequently, initially ten seconds. Do work off the GTK UI thread and keep
  the last valid reading while updating. Make the title metric configurable.
- Baseline release builds on a recorded reference machine, engine version,
  viewport, and local fixture corpus. Record cold/warm startup separately,
  p50/p95 tab-switch latency, page rendering/load timing, idle CPU, aggregate
  memory at 1/10/30 tabs, capture latency, and blocking overhead.
- Initial engineering targets, to validate in milestone 0: p95 browser-chrome
  tab activation under 100 ms for already-loaded local tabs; incremental idle
  monitoring/automation CPU below 1% of one core averaged over 60 seconds when
  no capture stream is active; investigate median startup or aggregate-memory
  regressions over 10% against the recorded same-environment baseline.
- Separate chrome response from page paint/network latency. Establish absolute
  startup/memory budgets after the baseline; do not promise arbitrary RAM caps
  or to outperform every browser on every page.
- Compile filters and persist state off the main thread. Bound screenshots and
  event queues, use on-demand capture, and add reversible inactive-tab unloading
  only with protections for media, downloads, forms, and explicitly pinned work.

### Visible agent control and MCP

Browny exposes a **browser-control MCP server**, usable by compatible clients
regardless of model provider. MCP interoperability does not guarantee that every
client supports image results, streaming, or every advertised tool.

**New user requirement — 2026-11-07:** The page/browser window should be headless so
web content does not cover the user's desktop, while a separate trusted native status
surface remains visible and names the selected Browny profile or Incognito, connected
client, current authorization and Stop control. The user wants an explicit session
choice that can use an existing Browny profile's own signed-in site state. This is not
implemented: today's MCP is a separate fixture-only ephemeral runtime. Before adding
it, validate the supported WebKit/Wayland headless rendering and real input path, then
architect attachment through the profile-owning Browny process (rather than sharing
profile files, copying databases/cookies or launching a competing profile writer).
Require a fresh native per-session grant, visible target confirmation, immediate
revocation and clear Incognito semantics. Persistent profiles retain their usual
browser-managed state; Incognito remains ephemeral. Do not expose cookie/token export
or accept external Chromium profile paths. Fixture-only tests remain the default; this
requirement alone does not authorize live Google sign-in/action tests or profile data
inspection in this development session.

- Initial transport: a local stdio adapter to a permission-restricted browser
  IPC endpoint. Authenticate/authorize the adapter and require browser-side
  pairing; knowing a tab ID or connection handle is not permission.
- No network listener by default. Optional Streamable HTTP requires a separate
  security design, current MCP authorization, origin/host validation, scoped
  credentials, and a loopback default. No unauthenticated remote-debugging port.
- Capabilities: list approved tabs; navigate/back/forward/reload; screenshot;
  inspect DOM/accessibility and element geometry; click, type, keypress, scroll;
  read bounded console/network diagnostics where supported; wait for explicit
  conditions. Advanced page-JavaScript evaluation is a separate elevated grant.
- No general host shell, arbitrary filesystem access, raw profile-cookie/token
  export, or desktop-wide screenshot/control tools in the browser MCP surface.
- The browser—not a cooperative agent—enforces profile/tab/origin grants,
  expiry, parameter validation, limits, and cancellation. Redirects, cross-origin
  frames, popups, downloads, and navigations trigger scope revalidation.
  Private-network/localhost targets require a distinct grant, including indirect
  requests and redirects; if full enforcement cannot be demonstrated, restrict
  automation to an OS-isolated network workspace rather than claim isolation.
- Permission levels: observe; interact; privileged debugging. “Observe” still
  exposes page data and needs consent. Default to a dedicated agent profile;
  export to the connected client may send information to that client's cloud
  provider, and the approval UI must explain that before access is granted.
- Require confirmation for uploads, clipboard sharing, permission changes,
  downloads, and other high-impact actions. Payments, messages, account changes,
  and deletion should pause for human review. Semantic detection is imperfect:
  for untrusted sites, use per-action confirmation, a restricted test workspace,
  or both; do not promise arbitrary clicks are automatically safe.
- Webpage text, screenshots, console messages, and tool results are untrusted
  observations, never authority to expand grants or approve a pending action.
  Prompt injection cannot be solved by a warning in an agent's system prompt.
- Display the connected client, selected profile or Incognito state, active tab,
  granted capabilities, operation summary and Stop in a trusted native control
  surface that page scripts cannot dismiss or impersonate. In visible-page mode,
  show a cursor/click marker in browser chrome/overlays; in headless-page mode,
  expose the bounded action/status stream there without putting the web page on the
  desktop. DOM-only actions need a distinct indicator. Do not claim a cursor is
  visible on a page surface the user has asked not to display.
- Pause/Stop and a global browser shortcut revoke control immediately, reject
  queued commands, and invalidate capabilities; human input preempts automation.
  Stopping cannot undo an already-sent request or completed external action.
- Screenshots use actual rendered WebKit content. Include tab/origin, capture
  time, viewport/scroll, device scale, zoom, and navigation generation. Reject
  stale element handles/coordinates after navigation or layout changes; hit-test
  before input, and verify the observed result after interaction.
- Do not substitute `element.click()` for native input without declaring the
  difference. Validate real trusted-input delivery and WebKit automation APIs;
  return unsupported for gestures that cannot be delivered correctly.
- Test cross-origin frames, overlays, canvas, scrolling, zoom, HiDPI, animation,
  navigation races, dialogs, file choosers, and screenshots while compositing.
  The current GTK test screenshot path has shown incomplete live-page captures;
  it is not yet evidence of a reliable visual-control implementation.
- Keep bounded local action records, redacting secrets; no raw screenshots,
  page bodies, auth headers, or typed values stored by default. Redaction is
  best-effort, so do not capture/share sensitive tabs without explicit consent.
- Publish schemas, errors, capability negotiation, a supported-protocol version,
  and client configuration examples. Verify with at least two independent MCP
  clients, including image-result handling and disconnect/reconnect behavior.

## Milestones and exit criteria

These are ordered product increments, not authorization to execute every phase.
Each milestone gets its own scoped implementation plan before code changes.

| Milestone | Deliverable | Exit criteria |
| --- | --- | --- |
| **0. Feasibility and baseline** | Capability matrix, threat model, release-build measurements; bounded checks of Inspector, persistent sessions, filters, screenshots, native input, and subprocess accounting. | Every mandatory capability has evidence or a named blocker/decision; reference workload and measurements are reproducible. |
| **1. Debuggable, observable browser** | Inspector integration, console entry point, memory title/details, bounded local diagnostics. | Inspect the correct tab; exercise DOM edits, console and JS breakpoints; dock/detach/close without leaks; RAM includes identifiable owned engine processes and reports gaps. |
| **2. Persistent browser foundation** | Profile runtime/locks, profile switcher, typed settings, atomic session restore, private mode; history/bookmark foundation. | Restart restores tab order/selection and fixture site data; two profiles remain isolated; locking, migration, crash recovery, and private non-restoration tests pass. |
| **3. Usable downloads** | Download UI, safe file destinations, cancellation/retry, profile metadata and errors. | Attachment bytes match the fixture; malicious names, collisions, cancellation, disk-full/write failure, profile closure and private-mode metadata are handled without silent execution or overwrite. |
| **4. Privacy and blocking** | Permission manager, supported privacy presets, native request blocking, cosmetics, site controls, list updates. | Fixture server confirms blocked requests never arrive; allowed content works; exceptions are scoped; invalid/oversized updates roll back; permission revocation and advertised privacy behaviors are tested. |
| **5. Headless interactive MCP** | Opt-in adapter, screenshots/inspection plus real clicks, arbitrary text, keys, scrolling and navigation; headless page surface with a trusted visible profile/Incognito target and Stop. User may explicitly pair a Browny profile's own signed-in session. | An MCP client completes a local fixture task; verified supported headless input/capture works. Explicit profile/Incognito choice is correct and visible, grants revoke, cross-profile/session access is denied, and Stop cancels queued actions. Existing profile sign-in state is used in place, never copied/exported. |
| **6. Automation hardening and interoperability** | Granular grants, elevated debugging, approval workflows, human preemption, robust frames/dialogs, and multi-client coverage. | Two independent clients interoperate; scope escapes, injected page instructions, layout races, disconnects, and post-revocation commands fail safely; human control remains authoritative. |
| **7. Hardening and daily-use release** | Packaging/update policy, recovery/migration coverage, performance tuning, accessibility, security review and documented limitations. | Full test matrix and security review are completed with material findings resolved; patch delivery is documented; measured performance stays within agreed budgets and no unverified parity claims ship. |

Performance and security verification apply to every milestone; milestone 7 is
not permission to defer either. Milestones 2 and 4 are prerequisites for exposing
personal persistent tabs to automation. Agent experiments before then must use
separate disposable profiles and local fixtures.

## Release discipline and non-goals

- Keep formatting, headless tests, strict Clippy, builds, and real GTK/WebKit
  graphical integration tests. Extend local fixtures for each new capability.
- Add storage/restart tests, download-failure tests, permission/isolation tests,
  filter fixtures, MCP contract/security tests, and reproducible benchmarks.
- Test Wayland and X11, multiple scale factors, keyboard-only use, reduced motion,
  and screen-reader accessibility on declared supported environments.
- Record WebKit/GTK versions in failures; public-site smoke checks are optional
  supplements, not substitutes for deterministic tests.
- No Chrome/Firefox extension-store compatibility, engine rewrite, built-in VPN,
  Tor/anonymity guarantee, cloud account/sync, or hosted autonomous agent service
  in this initial roadmap. Revisit separately if requested.
- No installation, implementation, engine migration, commit, push, or publication
  is authorized by this planning document alone.

## Definition of success

Browny can retain and restore separate everyday profiles; inspect and debug real
web applications; download files safely; explain and demonstrate its privacy/
blocking controls; report honest aggregate memory use; and let compatible agents
observe and interact with explicitly approved pages while the user sees and can
stop the activity. It remains responsive under recorded workloads, and its
security claims never exceed its tests and engine capabilities.

## Primary references

Capability references checked when drafting; recheck bindings and version-specific
behavior before implementation. These sources are not proof of completed work.

- [WebKitGTK Web Inspector](https://webkitgtk.org/reference/webkitgtk/stable/class.WebInspector.html)
- [WebKitGTK NetworkSession](https://webkitgtk.org/reference/webkitgtk/stable/class.NetworkSession.html)
- [WebKitGTK content-filter store](https://webkitgtk.org/reference/webkitgtk/stable/class.UserContentFilterStore.html)
- [WebKitGTK WebView snapshots](https://webkitgtk.org/reference/webkitgtk/stable/method.WebView.get_snapshot.html)
- [MCP security best practices](https://modelcontextprotocol.io/specification/latest/basic/security_best_practices)
