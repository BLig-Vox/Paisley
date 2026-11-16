# Restart reliability investigation

Date: 2026-11-07. **The repeatable crash was traced to test-fixture storage deletion during native WebKit teardown; production-release durability is still not established.**

The original stress test created and deleted many profile cache roots within one GTK/WebKit UI process. A deleted watched-cache path led to a NetworkProcess SIGSEGV inside a GLib file-monitor callback. Deferring owned fixture-root cleanup until the isolated namespace exits eliminated observed crashes/internal-load errors in the latest stress and complete GTK runs. This is a test-harness lifetime correction, not a production WebKit fix or a blanket guarantee for deleting live profile data. Earlier localStorage loss and native-load failures are retained as historical observations; this controlled test change narrows their likely context but does not prove every earlier symptom had the same cause. No engine replacement, host package, sandbox/TLS change, retry or artificial sleep was introduced.

## Reproduce under mandatory isolation

Build the single Rust test executable, then use Cargo's reported absolute path:

```sh
cargo test --offline --locked --bin paisley --no-run
python3 -I scripts/smoke_isolated.py \
  /absolute/path/to/target/debug/deps/paisley-TEST_HASH --restart-stress
python3 -I scripts/smoke_isolated.py \
  /absolute/path/to/target/debug/deps/paisley-TEST_HASH --restart-processes
```

Prefix with `distrobox-host-exec` when tools are on the host. The existing runner still requires installed Wayland/bubblewrap/private-D-Bus dependencies; it installs nothing. Both modes keep host personal profiles and public networking unavailable. They are diagnostics, not alternate browser backends or isolation fallbacks.

- `--restart-stress` runs the existing profile/session/site-storage fixture **eight times within one GTK test process**, stopping on failure. It creates only temporary owned profiles. It logs namespace-visible process counts, application/GTK window counts and weak native view/session lifetime after close. Normal clean-restart, safe tab order, lazy loading and profile-isolation assertions are retained, not weakened.
- `--restart-processes` runs **four independent seed/check/isolation triples**. Each stage launches this same fixed compiled **Rust test executable** in a new process, with one normal owned profile/runtime. Seed writes a known localStorage value and cookie, check reads them after actual process exit/relaunch, and isolation verifies another owned profile cannot read them. Each triple gets a new disposable root. There is no caller-selected browser executable, arbitrary site or personal-profile path.
- The ordinary complete GTK smoke now also calls the separate-process triples, alongside its existing in-process assertions. This is extra coverage, not removal of the failing test.

The subprocess branch uses the production Browser/Profile implementation but not the production CLI/activation entry point. It is not a standalone release-binary startup benchmark, crash/power-loss recovery test or proof of every platform/engine configuration.

## Observed evidence

1. An initial eight-cycle run failed during cycle 7 while waiting for the restored active fixture page. WebKit printed:

   ```text
   WebKit encountered an internal error. This is a WebKit bug.
   WebLoaderStrategy.cpp(679): internallyFailedLoadTimerFired()
   ```

   The test failed after 25.88 seconds; that engine message is observed output, not an independently established attribution of the root cause.
2. An instrumented eight-cycle run passed in 8.38 seconds. Before cycles 2–8, namespace-visible process counts stayed at 17 after an initial 4; application and GTK window counts were zero. Closed native view/session weak references were gone. This does not support a growing UI-object/process leak in that run; it does not prove all native shutdown/disk work is synchronous or rule out other resource problems.
3. A bounded three-pass command stopped at its first failure: pass 1 completed eight cycles (8.25 seconds); pass 2 failed during cycle 6 with the known value reading `null` instead of `work` (7.03 seconds). A read-only SQLite diagnostic exited successfully but printed no matching fixture-row report; that is **insufficient evidence to identify a storage cause**. Failure diagnostics now also list bounded owned file metadata/header classification before checking SQLite fixture rows.
4. Another bounded diagnostic command stopped during cycle 6 on a lazy-background-page timeout after the same WebKit internal-load message (25.06 seconds). It did not complete the remaining passes.
5. The first separate-process four seed/check pairs passed in 2.89 seconds. After adding a distinct-profile isolation stage, all four seed/check/isolation triples passed in **4.04 seconds**. No sleeps or automated retries were needed. This stronger lifecycle check does not erase the in-process failures or establish their cause.
6. A complete ordinary GTK smoke failed, before reaching the new process triples, while waiting for an allowed tracker after a site exception. The same WebKit internal-load message preceded the timeout (24.71 seconds). Do not report that run as green merely because prior complete runs passed.

7. Resource-instrumented eight-cycle stress passed in 9.09 seconds, but its NetworkProcess PID changed around cycles 6–7. That successful load run does not establish uninterrupted engine lifetime.
8. The next engine-logged stress run **failed in 24.73 seconds**, waiting for the isolated-profile page. Preserved local evidence is `target/restart-engine-log-20261107-a.txt` (ignored build output). Lines 1595–1607 show the internal-load error, then `NetworkProcessProxy::didClose (Network Process crash)`, a main-frame provisional-load failure (`WebKitErrorDomain`, code 300), and replacement NetworkProcess launch. This establishes WebKit's crash report and relaunch, **not the exit signal, crash stack or underlying cause**. It does not yet prove the separate localStorage-loss observation has the same cause.

9. After formatting the diagnostic helper and restricting logging to the supported Process channel, **one complete isolated ordinary GTK smoke passed in 30.35 seconds**, including all four separate-process triples, permission-denied/native Retry and disk-full fixtures. Preserved local output: `target/restart-full-check-2zk8y3el.txt`. Expected missing-a11y-bus and two unexplained credential-socket warnings were present. No production browser code changed and no automatic retry was used. This run does not resolve the earlier failures or validate the revised stress logging, since ordinary mode does not enable it.

### Distinct stress roots and retained engine crash — 2026-11-07

A fresh post-placeholder stress run failed in cycle 6, after **4.24 seconds**, at
`test directory must be fresh`, before its page/storage assertions. Local log:
`target/restart-post-placeholder-71zijwqg.txt`. Every cycle reused the same
`paisley-restart-PID` root even though each preceding fixture removed that directory.
Its reappearance demonstrates that reusing the pathname is not a fresh-root test;
the particular recreating writer was not identified.

The fixture now includes the cycle number in its owned root. That is a test-fixture
correction only: existing same-profile close/reopen, tab order/selection, localStorage,
cookie and other-profile isolation assertions are unchanged. Cleanup is unchanged.
The next single eight-cycle run completed all assertions in **8.12 seconds**, without
retry or sleep, but **still explicitly reported a NetworkProcess crash during cycle 7**.
`target/restart-distinct-roots-5xfd12wt.txt`, lines 983–988, records the crash, an empty
connection identifier and engine-internal reconnection/replacement launch. The runner
and Rust assertions returned success; this was **not an engine-clean reliability pass**.
No Paisley retry was added. View/session weak references were gone after closes, and
pre-cycle application/GTK window counts stayed zero. Credential-socket warnings remain.

Distinct roots therefore correct the fresh-directory fixture failure but do not
eliminate the engine crash. Neither this passing assertion run nor the fixture change
explains the earlier localStorage loss, establishes a crash signal/stack, or justifies
marking restart reliability resolved. Full production process-exit coverage remains
distinct from repeated Browser instances within one test process.

After the fixture correction, all **66 Rust headless tests**, strict Clippy and
formatting passed. The complete ordinary isolated GTK test passed in **27.29 seconds**,
including the four separate-process triples, but again printed a WebKit internal-load
error and credential-socket warnings. This final assertion pass is not an engine-clean
run or evidence that the retained release blocker is resolved.

### Passive terminal-status capture — 2026-11-07

The test-only `src/restart_diagnostics.rs` module now samples Linux process status
in restart-stress mode. It requires both the explicit isolated-runner fixture marker
and PID 1 named `bwrap`; otherwise it refuses process enumeration. It reads bounded
`/proc/PID/stat` records, identifies only the fixture's `WebKitNetworkPr` processes,
tracks PID plus start time, and emits at most 32 fixed-format lifecycle events. It
uses no ptrace, signal injection, waits/reaping of engine children, engine changes
or host-process attachment. A 1ms polling interval can miss rapid reaping; absence
of a captured status is not evidence of normal exit. This is not performance evidence.

A single eight-cycle run completed assertions in **9.07 seconds**, but captured:

```text
owned network terminal: pid=17 started=1554336 raw_status=139 signal=Some(11) exit_code=None core_dumped=true
NetworkProcessProxy::didClose (Network Process crash)
```

Local evidence: `target/restart-passive-exit-ci632zil.txt`, lines 977–983. This
establishes **SIGSEGV** for this run, not a stack, faulting function, memory-safety
classification, relationship to earlier storage loss or root cause. The kernel's
core-dump status bit alone does not guarantee a retrievable core.

A metadata-only journal query, restricted to the fixture's `/home/agent` working
directory and NetworkProcess name, found a matching system core for host PID
**1918940**, namespace PID **17**, SIGSEGV and the installed
`/usr/lib/webkitgtk-6.0/WebKitNetworkProcess`. After explicit approval, the core was
copied to mode-0600 under the private mode-0700 `target/owned-network-core-1918940-20261107/`
directory and analyzed locally with GDB; it was not uploaded, committed or published.
The executable Build ID was matched, and bounded `.symtab`/`.strtab` sections for
`libwebkitgtk-6.0.so.4` were fetched from debuginfod rather than the 1.63-GB full
DWARF file. The retained core is sensitive process memory; do not share it.

GDB reports `SEGV_MAPERR`, fault address `0x63`, at the `mov (%rsi), %rcx`
instruction inside `WTF::String::isolatedCopy()`. The stack resolves to:

```text
WTF::String::isolatedCopy() const &
WebKit::NetworkCache::Storage::versionPath() const
WebKit::NetworkCache::dumpFileChanged(WebKit::NetworkCache::Cache*)
GIO / GObject file-monitor signal dispatch
GLib main-context dispatch
```

Thus the bad access occurs while a NetworkCache file-monitor callback asks storage
for a versioned path. The faulting `String` receiver pointer is `0x63`, not a valid
mapped object address. The stack does **not** prove which owner/lifetime became
invalid, establish a WebKit upstream defect, or explain the earlier localStorage
value mismatch by itself. GDB could not open an ITP shared-memory file which had
already been deleted with the fixture root. Local bounded backtrace/register output:
`target/owned-network-backtrace-f21ehmn2.txt`; full core remains only under the
private target evidence directory.

An earlier tracing experiment was abandoned: default GDB shell startup failed in
the restricted filesystem; direct execution returned after a resource-probe child
exit; a followed-child continuation attempt timed out in cycle 1 after 90 seconds.
These are failed diagnostics, not test passes or evidence of a crash cause. The
experimental `--restart-debugger` mode and its temporary contract tests were removed;
no less-isolated fallback was added. Logs remain in ignored `target/restart-debugger-*`.

Four passive-monitor regressions first failed before implementation, then passed:
status/identity parsing, malformed/truncated input rejection, mandatory fixture
scope, and real Linux terminal status from an owned SIGKILL child before reaping.
A separate teardown-lifetime regression first failed when cleanup removed a marker,
then passed with a bounded owned-root deferral helper. This helper is used only by
native test fixtures: under `smoke_isolated.py`, a disposable namespace `/tmp` is
removed at namespace exit; a direct graphical test run logs each retained `/tmp/paisley-*`
root for manual cleanup only after the test process exits.

Paired eight-cycle comparison captured SIGSEGV when roots were deleted immediately
(`target/restart-paired-restart-stress-pl5y00ad.txt`, 8.50s), but no observed
NetworkProcess exit in the retained-root control
(`target/restart-paired-restart-retained-up9_442s.txt`, 6.91s). Earlier independent
cleanup stress runs also captured SIGSEGV. Three runs after deferred cleanup all
passed the assertions without an observed NetworkProcess exit: stress **6.71s**
(`target/engine-lifetime-fixed-stress-s0mpce2n.txt`), full GTK **22.97s**
(`target/engine-lifetime-fixed-full-nw6ro9hs.txt`), then fresh stress **7.73s** and
full GTK **24.66s** (`target/final-lifetime-stress-8gpimorf.txt`,
`target/final-lifetime-full-u1y6rdxy.txt`). The last full run contained no WebKit
internal-load/crash message. Credential-socket broken-pipe warnings remain in the
latest stress and full logs. This is strong evidence that deleting watched fixture
cache roots during native teardown triggered the test crash; the small paired sample
and retained storage do not prove or repair every WebKit cache-lifetime issue.

Final fresh verification after these changes: **71 Rust headless tests**, **27 Python
tests**, strict Clippy, Cargo formatting, explicit included-module rustfmt, debug and
release builds passed.
The crash monitor is test-only and refuses to run without the private-fixture marker
and PID-namespace `bwrap` guard. No production browser policy, persistence writer,
engine sandbox, TLS behavior or native process lifetime code was changed.

### Resource observations and logging limits

The isolated snapshots reported roughly 8.21 GB free on the ordinary fixture filesystems, millions of free inodes, and the intentionally bounded download tmpfs entirely free during restart stress. The failed run's timeout snapshot had 38 UI descriptors, 30 replacement-NetworkProcess descriptors and 43 UI threads, against a soft descriptor limit of 1,024. Earlier samples also remained well below that descriptor limit. Read-only host cgroup ancestry inspection found no sampled PID-limit events or OOM/kill events and no configured memory maximum in the sampled ancestry. These samples do not demonstrate filesystem, descriptor, PID or OOM exhaustion; they do not exclude a short-lived event between samples or establish the crash cause.

Resource diagnostics enumerate process metrics only when PID 1 identifies the private bubblewrap namespace. No host profile or host-process contents are collected. `--restart-stress` enables the installed engine's supported `WEBKIT_DEBUG=Process` channel. The preserved failure log used two additional channels, which native child processes warned were unknown; those channels were removed from the runner, not interpreted as causal evidence. Logging can change timing and does not provide a crash stack.

Failure-only diagnostics read at most 128 owned fixture paths and read-only SQLite tables for the fixed test key. They never inspect host profiles or print arbitrary site contents. Python runs with `-I`; an unavailable diagnostic cannot replace the original assertion. SQLite reads are diagnostic only, not a browser persistence implementation.

## Current verification status

- Latest fresh headless run: **71 Rust tests passed**, one graphical test ignored; 27 Python tests, strict Clippy and formatting passed.
- Separate-process storage/cookie persistence plus isolation passed all four triples.
- With fixture roots retained until isolated-namespace exit, the latest eight-cycle stress and full graphical suite both passed without a NetworkProcess crash or WebKit internal-load error. The immediate-cleanup control reproduced SIGSEGV; see the paired evidence above. A later full GTK run after the post-exit profile-deletion assertion also passed in **22.87s**; it had test-only Inspector mapping diagnostics enabled.
- Before that pass, a complete GTK run failed after **21.35s** waiting for the native Inspector frontend to map on its second lifecycle reopen. The focused Inspector mode passed in **2.80s**; the later instrumented full pass does not identify the failure cause or erase the failed run. See [Inspector evidence](INSPECTOR.md).
- The older localStorage `null` observation and prior native-load/Inspector reopen failures remain in the history. Current tests no longer reproduce them with this teardown change; this does not prove a universal site-storage durability fix, verify every past failure's cause, or cover the production CLI/desktop/display matrix.
- The earlier HTTP MCP second-workspace `WebDriver: unknown error` failure also remains unresolved; subsequent HTTP/HTTPS smokes passed, but this investigation has not established whether those failures share a cause.
- Missing-a11y-bus warnings remain expected in private service-free D-Bus; credential-socket warnings remain unexplained.

## Work ledger and remaining gates

- [x] Save reproducible bounded isolated diagnostic modes.
- [x] Reproduce the teardown-triggered NetworkCache crash and verify its SIGSEGV core stack.
- [x] Check native weak references and process/window counts rather than assuming a leak.
- [x] Capture owned process exit status and correlate the crash with immediate fixture-root deletion.
- [x] Add real test-process exit/relaunch persistence and cross-profile isolation coverage.
- [x] Retain original assertions, defer cleanup only in test fixtures, and record cleanup/control runs separately.
- [ ] Determine whether the earlier localStorage `null` observation has any independent production cause.
- [ ] Validate ordinary production profile close/reopen and closed-profile deletion without deleting storage under a live native session.
- [ ] Broader engine/display/desktop/crash matrix and independent review.

A supported engine change or host-package replacement is **not authorized** by these fixture tests; no such change was made. Missing independent review, license/release decisions and the other [roadmap](../ROADMAP.md) gates remain open. This investigation does not finish the roadmap.
