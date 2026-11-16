# Native Inspector verification

Date: 2026-11-07. This records a verified lifecycle subset, not completion of developer-tools or release gates.

## Supported integration

F12, Ctrl+Shift+I, Ctrl+Shift+J, the toolbar and menu toggle the active tab's real WebKit Web Inspector. Right-click Inspect Element uses the engine's native context menu. Developer extras are enabled for each tab. Each tab has its own Inspector; opening one does not deliberately open another tab's tools.

Paisley hosts attached inspectors in a resizable pane on the right of the page.
This applies to keyboard/toolbar opening and the native Inspect Element context
menu. New tabs use the same layout; no per-tab docking choice is needed. The
divider initially gives the tools about 40% of the tab width. Closing the tools
returns that space to the page. The native detach control still opens a separate
window; reattaching returns the tools to the right. Paisley controls the attached
layout, so WebKit's native left/bottom dock buttons also use the right-side host.

The public WebKitGTK 6.0 API provides show/close, attach/detach, the frontend widget, inspected URI and lifecycle signals. The installed API/bindings do not expose a dedicated console-panel selection method. Ctrl+Shift+J therefore toggles Inspector; select Console inside the native tools. No private frontend JavaScript, fake console or Chrome DevTools Protocol bridge is substituted.

## Owner-close regression and fix

A real isolated native regression opened the tools, detached them into a separate window, retained the inspected WebView/frontend, and closed the browser window. It **failed** waiting for the detached Inspector to unmap and emit `closed` (22.59 seconds for that focused run).

The browser window shutdown handler stopped tab loading but did not close Inspectors, unlike the existing individual-tab close handler. It now explicitly closes each tab's Inspector before stopping its WebView. The regression then **passed in 2.62 seconds**, including three dock/detach/toggle cycles and tab ownership checks. This is a failing-then-fixed browser teardown defect, not a claimed fix for the separate NetworkProcess/storage failures.

The fixture checks actual mapping, positive widget allocation, roots changing between the owning browser and a detached native window, close signals, continued presence of the other tab, and private no-profile-write behavior. It retains native handles across closure so teardown cannot pass merely because the test released the last WebView reference.

## Reproduction

```sh
cargo test --offline --locked --no-default-features --features legacy-webkit --bin paisley-engine --no-run
python3 -I scripts/smoke_isolated.py \
  /absolute/path/to/target/debug/deps/paisley-TEST_HASH --inspector-lifecycle
python3 -I scripts/smoke_isolated.py \
  /absolute/path/to/target/debug/deps/paisley-TEST_HASH --inspector-preface
python3 -I scripts/smoke_isolated.py \
  /absolute/path/to/target/debug/deps/paisley-TEST_HASH --inspector-open-close
```

Use Cargo's reported test-executable path, not the production browser binary. Prefix commands with `distrobox-host-exec` when the installed development tools are on the host. `--inspector-lifecycle` runs only the Inspector fixture; `--inspector-preface` first runs the settings and non-restorable-placeholder fixtures that precede it in the ordinary full smoke; `--inspector-open-close` runs those same prechecks then three show/close cycles without attach/detach transitions. The ordinary full smoke includes additional tests. A focused pass must not be reported as a full-suite pass.

Mandatory bubblewrap user/network/PID/mount isolation, private service-free D-Bus, disposable profiles, fatal GLib criticals and the existing timeout remain in force. No package installation, public navigation, remote Inspector listener, permission grant or weaker sandbox is introduced.

## Fixture corrections and limits

- The old `is_visible()` check was too weak: GTK visibility alone does not establish that the Inspector is mapped into a window. The updated fixture retains the first frontend wrapper and waits for mapping/allocation. The installed glib binding's `from_glib_none` sinks floating references, so dropping a newly created unparented frontend during polling is not a neutral observation.
- Early docking attempts timed out on `can_attach`, even though the native tools were already attached with a 1280-by-300 frontend. The corrected fixture asserts actual attach/detach transitions and window roots instead of treating that advisory property as the transition itself.
- Close-signal totals can include additional native closure events. Each explicit close checks advancement from its immediate pre-action count and verifies unmapping/unparenting, rather than assuming a global event total equals the loop number.
- Initial screenshot diagnostics captured the browser before useful Inspector pixels were established. They are **not** accepted evidence of rendered panel contents; the premature capture hook was removed.
- Inspector runs emitted `NeedDebuggerBreak` trap diagnostics. Those messages are not evidence that a user breakpoint/stepping workflow passed. Missing-accessibility-bus warnings remain expected in this namespace; credential-socket warnings remain unexplained.

## Verification and remaining work

After the shutdown fix, 63 Rust headless tests, 27 Python tests, strict Clippy, formatting, debug and release builds passed. A full isolated GTK test passed in **34.71 seconds**, including Inspector lifecycle, process-restart triples and download failure fixtures. Its complete local log is preserved under ignored `target/inspector-full-verification-zvp1ett9.txt`.

**Later Inspector-reopen evidence — 2026-11-07:** a fresh focused lifecycle mode passed in **2.80s**, while the following complete isolated GTK run failed in **21.35s** waiting for the frontend to map on the second open/close cycle. The pre-open snapshot was unattached with no frontend; the process emitted `NeedDebuggerBreak`, but that message does not establish cause. After adding test-only mapping-state diagnostics, a complete isolated GTK run passed in **22.87s**, including all three Inspector cycles, the separate-process profile triples and post-exit profile-catalog deletion. It logged frontend transitions from unmapped/unrooted/0×0 to mapped/allocated.

A new `--inspector-preface` diagnostic mode runs the normal settings and non-restorable-placeholder fixtures before the Inspector lifecycle fixture. One run failed after **25.57s** while reopening the first tab's Inspector after closing the second tab: the frontend was attached, visible, mapped and rooted, but remained **0×0**. A later preface run passed in **9.01s** and showed that a 0×0 mapped state can be transient before allocation reaches 1280×300. The newest preface run failed after **21.40s** on the second open: the Inspector reported attached, but the visible frontend stayed unmapped at 0×0 and had no GTK parent (`rooted=false`; only the `WebKitWebViewBase` itself appeared in the ancestor dump). This differs from the earlier rooted-but-zero-sized failure.

To test whether the fixture's attach/detach transitions were required, `--inspector-open-close` repeated the prefaced setup but did only three `show()`/`close()` cycles. It failed after **22.26s** on the second open with the same unrooted/unmapped 0×0 frontend, even though the first open mapped normally; thus explicit attach/detach calls are not necessary to trigger the observed failure. This still does not identify whether WebKitGTK, GTK, compositor state or test context is responsible. The app uses WebKit's default Inspector window behavior and does not override its signals. Installed WebKitGTK is **2.54.2**. WebKit bug [288189](https://bugs.webkit.org/show_bug.cgi?id=288189), “Inspector never starts attached and can’t be attached,” is a related lead reported as fixed in a nightly build, but the released version containing that fix is not established here and the report is not confirmed to describe this failure. See the [WebInspector API documentation](https://webkitgtk.org/reference/webkit2gtk/stable/class.WebInspector.html) for the default GTK window/attachment behavior.

No production behavior, assertion, timeout or retry policy was weakened; each failure remains recorded, and intervening passes are not fixes or proof of stability.

At the earlier Inspector-integration snapshot, a complete GTK run failed in 20.93 seconds waiting for the Inspector to map on reopening after the first lifecycle cycle. After adding pre-action state diagnostics, the focused lifecycle mode passed in 5.59 seconds, with no product change; each pre-open snapshot in that run reported unattached/no frontend. The final fixture also explicitly detaches the second tab's tools before testing tab close.

No already-installed headless compositor or Xvfb was found by the host executable check (Weston, Cage, Gamescope, Sway, Labwc, Wayfire, Xvfb). Tests therefore still share the existing Wayland compositor socket. Focus/input interference is a possible limitation, not an established explanation for any failure; no packages were installed.

At this integration snapshot the actual HTTPS MCP fixture passed trusted Unicode/click/wheel input, PNG, certificate validation, origin denial and Stop/cleanup. The HTTP MCP fixture **failed** at initial `browser_navigate` with `Workspace stopped or control channel lost`; HTTPS and release build were then run separately, not hidden behind the failed command chain. Later [bounded-stop diagnostic verification](MCP.md#bounded-revocation-diagnostics) recorded an HTTP pass, a complete GTK pass with a native error, and a distinct HTTPS failure explicitly revoked by the native keyboard handler. Those later observations do not establish causes for earlier failures. No automatic retry or fallback was added. The relationship to the earlier native reliability failures is unestablished.

Still open: actual DOM/CSS edits, Console evaluation/errors, breakpoints/stepping, network/storage/performance panels, direct console selection if a supported API becomes available, frontend-render readiness and full screenshots, long-duration/native resource-leak accounting, desktop/accessibility/display matrices and independent review. The [restart investigation](RESTART_RELIABILITY.md) and [full roadmap](../ROADMAP.md) remain separate open gates.
