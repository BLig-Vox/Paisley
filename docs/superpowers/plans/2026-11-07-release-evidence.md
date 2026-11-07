# Release Evidence Implementation Plan

> Continue the existing native execution workflow. No installation, publication, license choice or relaxation of automation permissions is authorized by this file.

**Goal:** Add reproducible local release packaging and measured release-build browser baselines, rather than another unmeasured completion claim.

**Architecture:** A standard-library Python packager creates an unsigned, local-development-only archive from a supplied built binary; it never installs or executes inputs. The existing single GTK test gains an opt-in release benchmark branch that operates real Browser windows against its owned HTTP fixture, temporary normal profiles and real native snapshots. No new privileged production mode or personal-profile access.

**Spec:** `../../../ROADMAP.md`, especially performance and hardening/release requirements.

## Constraints and interpretation

- Keep Rust/GTK/WebKit dependencies and TLS/origin/process defaults; builds are locked/offline.
- No packages, paid model calls, live-site actions, host installs, commits or publication.
- Parent CLAUDE.md synchronization remains permission-blocked; do not retry indirectly.
- No license is specified in Cargo.toml. Do not invent one or claim redistributable/production release readiness.
- Keep GTK operations on the existing single test thread. Memory/proc sampling belongs on background threads.
- Benchmarks must label in-process fresh/restored profile/window readiness separately from standalone process startup or cold disk-cache measurements. No cache flushes or unsupported cold-start claims.
- Measure actual 1/10/30 loaded local tabs, tab activation, native capture, 60-second idle CPU and fixture filter on/off behavior. Report missing/partial process accounting honestly and do not silently gate correctness on noisy performance samples.
- Default benchmark traffic is owned-loopback only; default homepage assertions must not initiate an unrequested public request.

## Deliverables

### A. Local packaging (delegated, isolated files)

Own files: `scripts/package.py`, `tests/test_package.py`, `packaging/site.blig.Browny.desktop`, `docs/PACKAGING.md`.

- [x] Write/run failing packaging behavior tests before implementation.
- [x] Implement fixed prefix layout, reproducible tar/gzip metadata, SHA-256 manifest, regular/no-symlink sources and exclusive no-overwrite output.
- [x] Desktop launcher matches the supported CLI; no unsupported URL/MIME association.
- [x] Independently run tests and desktop validation, build a local release archive and verify its contents/checksums without unsafe extraction.
- [x] Document unsigned/development-only scope, runtime libraries, missing license and release/update/security prerequisites.

### B. Release benchmark harness (leader)

Own files: `src/browser_benchmarks.rs`, benchmark branch in `src/browser_tests.rs`, `docs/PERFORMANCE.md` and evidence under ignored `target/`.

- [x] Observe failing percentile/statistics tests and the missing real benchmark branch before implementation.
- [x] Implement bounded finite percentile summaries with literal unit expectations and explicit empty/invalid rejection.
- [x] Add the single-threaded owned-fixture benchmark branch with exclusive JSON output and temporary normal profiles.
- [x] Measure fresh/restored in-process readiness, tab activation at 1/10/30 tabs, background partial PSS/RSS, native snapshot/PNG latency, 60-second partial owned-process idle CPU, and enabled/disabled fixture request-filter navigation. One-tab activation is explicitly a no-op baseline.
- [x] Run release measurements, read the native snapshot, record machine/GTK/WebKit/build/workload conditions and measured values without parity/production claims.

### C. Integration and remaining gates

- [x] Fresh tests, graphical smoke, strict lint, build, formatting and documentation checks after integration.
- [x] Update roadmap/ledger with actual evidence and unresolved gates, not a blanket completion checkbox.
- [x] License/distribution policy, actual production-client model sessions, missing requesting-frame permission API, personal-tab pairing and unavailable physical/display/accessibility conditions remain named user/environment/engine gates. Do not fabricate approvals or passing coverage.

## Execution and review ledger

- Packaging delegation returned four scoped files. Leader independently read them, ran the initial five tests and validated the source desktop entry. Further failing regressions reproduced a prefix-dependent launcher missing its absolute path, unsafe/noncanonical prefixes, and a too-large USTAR timestamp leaving a partial archive. All seven packaging tests now pass. The script uses bounded no-follow source descriptors, validates timestamp/prefix before writing, and exposes a completed private staging archive through a no-overwrite hard link.
- Actual optimized executable packaging: `target/package-evidence-JOt4t0/first.tar.gz` and `second.tar.gz` were byte-identical, four regular payloads with verified hashes. SHA-256: `974f22aba1a18a23be49daedd5e7dc39ee486367e1f58efaef94c54313de7809`. Both the source and generated absolute-prefix desktop launchers passed installed host `desktop-file-validate`. No extraction, installation or publication.
- Statistics and CPU-parser tests failed on missing implementations first; a real opt-in GTK run failed on the missing benchmark implementation. Nearest-rank/invalid-input and parenthesized-process-name/CPU accounting tests then passed.
- Initial real release run passed (68.76s) but captured its window before visible first paint; that image was rejected as visual evidence. The next run rendered valid images but timed out on readiness (89.12s). Its empty output and retained owned-profile directory concretely confirmed the cleanup problem. Source review independently identified cleanup/publication and misleading one-tab selection labeling.
- Two new headless regressions failed before fixes: failed evidence published an empty report, and the no-op label was absent. A private staging/profile cleanup guard, benchmark-window cleanup, exclusive complete-report publication, explicit `no_op_baseline` and always-different multi-tab selection made all five benchmark tests pass. The inspected old failed directory and zero-byte output were removed only after its owning process had exited. Current successful runs leave no benchmark profile roots.
- Final baseline `target/release-baseline-20261107-c.json` passed (81.09s) with fatal GLib criticals; `-c.png` and `-c-page.png` were read and show the real fixture. The readiness timeout is **not diagnosed or claimed fixed**; near-timeout state diagnostics were added. One credential-socket broken-pipe warning remains unexplained. Measurements/limitations are in [performance evidence](../../PERFORMANCE.md).
- Fresh verification after integration: 43 Rust headless tests; 27 Python tests; Cargo formatting plus explicit included-file rustfmt; strict all-target Clippy; debug/release builds; both actual isolated HTTP/HTTPS MCP smokes passed. The complete normal graphical regression branch then passed in 13.51s with fatal GLib criticals inside a separate mandatory user/network/PID/mount/IPC namespace, disposable HOME/runtime, read-only executable/toolchain, private service-free D-Bus, no public network and only Wayland shared. Its default-home requests therefore could not reach DuckDuckGo. Cairo/software rendering and intentionally absent accessibility services limit this evidence; expected nonfatal accessibility-bus and credential-socket warnings occurred. An earlier attempted outer wrapper failed (exit 134) because it reused read-only host runtime/proc paths and service activation; the corrected empty-root/private-runtime/service-free namespace resolved that harness configuration, without disabling WebKit's own sandbox or fatal-critical checking.
- Remaining release gates: no license/redistribution decision, notices/provenance, signing/update policy, independent production-client model sessions, safe requesting-frame permission API, personal-tab pairing, representative/standalone/incremental performance budgets, comprehensive frame/race/recovery/security/accessibility/display matrices. Parent CLAUDE.md edit remains denied and untouched.

References: [performance evidence](../../PERFORMANCE.md), [packaging](../../PACKAGING.md), [roadmap](../../../ROADMAP.md).
