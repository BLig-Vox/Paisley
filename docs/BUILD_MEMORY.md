# Building within 2 GiB RAM

On Linux with a working systemd user manager and cgroup v2:

```sh
python3 scripts/build-limited.py -- cargo build --release
```

Use `--limit-gib 1` before `--` for a 1 GiB cap. The default is 2 GiB for the entire build process group, including Rust, native compilers and linker children. The kernel also counts charged filesystem cache. Build swap is disabled. Compilation uses one job, no development debug info, and no release LTO. A watchdog stops the build if system available memory falls below 2 GiB.

The supervisor refuses to build if it cannot verify the hard limit; it never falls back to an uncapped build. Results, cgroup peak memory and OOM counters are written to `target/build-limited.json`. Resource limits can cause an individual compile or link step to fail. Increase neither the cap nor job count without the user's approval.

This limits the build, not unrelated desktop applications, and cannot guarantee that the whole laptop never runs out of memory. A lower memory limit can make compilation slower or prevent it from completing.

## Observed build, 2026-11-17

The existing Paisley WebKit binary compiled successfully through the supervisor, using the shared dependency cache and Ubuntu linker wrapper. Cgroup peak memory was 1,794 MiB; elapsed time 133 seconds; exit code 0; OOM and OOM-kill counters both zero. No build swap was allowed. This is not a clean-build result or a CEF build result. See `testing/2026-11-17/build-2g-result.json`. The CEF migration must be verified separately under the same cap.

## CEF build and regression checkpoint

The CEF native fixture executable also built successfully through the same limit. The latest cached relink peaked at 564.96 MiB (5.5 seconds, exit 0); an earlier initial CEF native build peaked around 1747 MiB. The root library regression build/run peaked at 1793.8 MiB (125.1 seconds), with zero OOM kills and 54 passing tests. Neither is a clean-build benchmark. Exact supervisor reports and runtime measurements are in `testing/2026-11-17/`; runtime PSS figures are separate from build cgroup peaks.
