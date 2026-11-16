# Owned-fixture release baseline

Measured 2026-11-07. These are **development baselines, not validated production budgets, browser comparisons, standalone launch measurements or cold-cache results**. No public sites or personal profiles are used by this opt-in benchmark branch.

This reference run predates the profile-catalog/settings integration and root maintenance lock. The newer release build passed correctness checks but has not been remeasured here; rerun before comparing or asserting current-build performance.

## Reproduce

Run in a graphical session from `Browny/`, using installed host tools through `distrobox-host-exec` when needed. Choose new absolute output/screenshot paths; existing files are not overwritten. No packages, host settings or caches need changing.

```sh
PAISLEY_BENCHMARK_OUTPUT="$PWD/target/baseline-new.json" \
PAISLEY_BENCHMARK_SCREENSHOT="$PWD/target/baseline-new-window.png" \
PAISLEY_BENCHMARK_PAGE_SCREENSHOT="$PWD/target/baseline-new-page.png" \
G_DEBUG=fatal-criticals \
cargo test --release --offline --locked --no-default-features --features legacy-webkit --bin paisley-engine browser_smoke \
  -- --ignored --test-threads=1 --nocapture
```

The existing single GTK smoke-test thread takes a separate benchmark branch before the ordinary smoke constructs its default-home browser. It creates temporary **normal**, not personal, profiles with the owned loopback fixture configured as homepage **before** opening each browser. Debug builds are rejected. Allow roughly two minutes; the idle interval alone lasts 60 seconds. The desktop should remain undisturbed, with no simultaneous benchmark/build workload.

JSON publication occurs only after measurements, serialization and successful profile-directory cleanup. A private staging directory and window-cleanup guard remove temporary profiles on Rust unwinding; complete JSON is linked into place exclusively. This is not cleanup after SIGKILL, power loss or an aborting native library. Optional images are diagnostic captures and may survive a later failed measurement; they are not proof that the complete run passed. Use trusted local output-parent directories.

`cargo fmt` does not traverse the `include!` benchmark file; also run:

```sh
rustfmt --edition 2024 --check src/browser_benchmarks.rs
```

## Reference conditions

- Paisley 0.1.0, optimized Cargo release test executable; Rust/Cargo 1.99.1.
- Intel Core 7 150U; Linux `7.2.14-arch1-1`; reported MemTotal 16,048,112 KiB.
- GTK 4.24.1; WebKitGTK 2.54.2; Wayland desktop, `GskVulkanRenderer`, scale 1.
- Window 1280 × 800; initial captured WebView 1280 × 698. The 30-tab notebook/final viewport was 1280 × 694.
- Tiny HTTP loopback document: title/heading “One” and a “Next” link. This is not a representative modern web application.
- Five fresh/restored profile pairs within one already-initialized GTK process. OS/engine caches are not flushed; fresh profile does not mean cold disk cache.
- 64 tab-selection/main-context-dispatch samples per tab count; one tab is explicitly a **no-op baseline**. Ten/30-tab samples always select a different tab. These do not wait for compositor-presented page pixels.
- Ten native WebKit visible-region snapshots, followed by GDK PNG encoding; callback waits use the smoke helper's approximately 10 ms polling, which limits the precision of snapshot/load timings.
- Native bundled-filter navigation samples run in fixed order: disabled, twelve-domain nonmatching starter filter, then one synthetic matching `/tracker.js` rule. Ten unique document URLs per mode; the tracker script response disables caching. Compilation is excluded from navigation timing.
- Idle CPU samples on background threads use `/proc` user+system ticks, matched by PID and process start time. Only processes present at both endpoints count; newly created/exited/reparented/invisible processes may be missing. It includes the test runner, fixture listener, GTK polling and browser monitoring, not just incremental monitoring CPU. CPU percent is normalized to one core with `CLK_TCK=100`.
- Memory is background Linux-visible owned-process PSS (RSS fallback if unavailable), explicitly **partial**. Sequential browser creation and engine caching/teardown can affect it; it is not an isolated per-profile or all-host-runtime measurement.
- Raw samples and nearest-rank p50/p95 are retained in JSON, without trimming outliers. With five/ten samples, p95 is the maximum: do not infer statistical stability.

## Measured results

Reference evidence: local ignored `target/release-baseline-20261107-c.json`. The release test passed in 81.09 seconds with fatal GLib criticals enabled. Window and real WebKit page PNGs were separately read and visually checked: both show the actual “One” document/link, not a blank frame.

| Workload | Samples | p50 | p95 |
| --- | ---: | ---: | ---: |
| Fresh profile/window/fixture ready, in process | 5 | 173.15 ms | 225.86 ms |
| Restored profile/window/fixture ready, in process | 5 | 147.00 ms | 172.56 ms |
| Native WebKit snapshot callback | 10 | 10.67 ms | 11.52 ms |
| GDK PNG encoding | 10 | 15.91 ms | 23.21 ms |
| One-tab no-op/main-context dispatch | 64 | 0.0030 ms | 0.0031 ms |
| Different-tab dispatch, 10 loaded tabs | 64 | 0.27 ms | 0.78 ms |
| Different-tab dispatch, 30 loaded tabs | 64 | 0.60 ms | 1.16 ms |
| Navigation, blocking disabled | 10 | 24.09 ms | 26.37 ms |
| Navigation, nonmatching starter filter | 10 | 24.58 ms | 29.73 ms |
| Navigation, synthetic matching filter | 10 | 24.42 ms | 25.99 ms |

| Loaded local tabs | Partial owned-process PSS | Linux-visible owned processes |
| --- | ---: | ---: |
| 1 | 284,692 KiB (278.0 MiB) | 14 |
| 10 | 771,571 KiB (753.5 MiB) | 68 |
| 30 | 1,452,352 KiB (1,418.3 MiB) | 188 |

The 60.018-second idle interval measured **1.75% of one core**, partial ownership and harness overhead included. Server counters were **10 / 10 / 0** tracker requests for disabled / starter-nonmatching / synthetic-matching modes. These counters prove this filter fixture, not real-world ad-block coverage. Small fixed-order timing differences do not establish that blocking improves performance or has negligible overhead.

The roadmap's under-100-ms chrome response target is not a demonstrated paint-latency budget merely because dispatch is fast. The idle result is numerically above its under-1% incremental-monitoring target, but this harness does not isolate incremental overhead; that target is **not validated**. There is no agreed startup/memory budget yet. Thirty tiny loaded tabs consuming roughly 1.4 GiB partial PSS is a measured optimization concern, not a RAM cap or a favorable comparison.

## Failures and remaining evidence

An initial complete run passed in 68.76 seconds, but its immediate window capture preceded the first visible document paint, so it is not the visual reference. A second run produced valid page/window images but **failed after 89.12 seconds**, timing out on fixture readiness; its cause was not established and the failure must not be hidden by later green runs. That failure also demonstrated an empty public report and retained profile-directory bug. Cleanup/publication and explicit one-tab labeling gained failing-then-passing headless regressions. Readiness now logs tab/URI/title/loading/allocation/filter state near timeout. The final run passed; intermittent readiness reliability still needs investigation if it recurs.

A nonfatal WebKit credential-socket broken-pipe warning occurred in the final run. Its cause is not established. No GLib critical or engine-security-default relaxation was required.

Still open: independent-process cold/warm launch, representative workload corpus and repetitions/randomization; compositor-presented tab/page latency; incremental monitoring/automation CPU versus an uninstrumented control; aggregate resource budgets, teardown stability and process-namespace accounting; automation latency/capture separately from this normal-browser fixture; tuning against agreed budgets; X11/HiDPI/accessibility/desktop and production-client matrices. Do not mark the performance or release roadmap milestone complete from this baseline.
