# Download filesystem failure coverage

Date: 2026-11-07.

## Verified cases

The existing single-threaded native GTK/WebKit smoke now includes owned filesystem-failure fixtures when launched through `scripts/smoke_isolated.py`:

| Case | Actual failure | Application evidence |
| --- | --- | --- |
| Destination preparation | An owned mode-0500 directory refuses creation with Linux EACCES (13). | A visible error and Failed progress state; one Failed record, no completed file and no Reveal action. Failure metadata is saved outside the restricted directory. |
| Permission recovery | Restore that directory to mode 0700 and click the real Retry button. | A new native download writes exactly `test`; the original Failed record remains and a separate Completed record appears. |
| Native output disk full | A 4-MiB HTTP attachment targets a new private 1-MiB tmpfs. | WebKit emits `Error writing to file: No space left on device` after destination selection; the application shows the error and Failed state, never adds a completed path or Reveal action, and persists exactly one Failed record. The record remains Failed after profile reopening. |

These extend coverage of **existing production behavior**. They passed on first integration; no production download logic was changed and no new failing-then-fixed production regression is claimed. Assertions protect against completion/duplicate-terminal regressions, missing error presentation, lost failure metadata and a broken retry path.

## Reproduce

Use the already-installed host toolchain and Wayland dependencies; no package installation or host filesystem mounting is needed:

```sh
cargo test --offline --locked --bin browny --no-run
python3 -I scripts/smoke_isolated.py \
  /absolute/path/to/target/debug/deps/browny-TEST_HASH
```

Use Cargo's reported test-executable path, not `target/debug/browny`. Prefix commands with `distrobox-host-exec` when tools are on the host.

The runner always creates the limited tmpfs **inside its mandatory private mount/user/network/PID/IPC namespace** and sets `BROWNY_DOWNLOAD_CAPACITY_FIXTURE` there. The test checks the exact path and tmpfs mount/size before a bounded 2-MiB probe establishes real ENOSPC; that probe is removed before the native download. No host disk is filled. Public networking, personal profile roots and host session D-Bus remain unavailable. No engine security default is weakened.

An ordinary graphical smoke invocation without that fixture variable explicitly prints a skip for these I/O cases; it must not be presented as disk-full coverage. These cases are subchecks of the existing single GTK test, not additional headless tests. Non-Linux or differently sized/mounted filesystems are not silently accepted as equivalent fixtures.

## Verification at the download-coverage integration

After strengthening native error-label and ENOSPC-specific assertions, the complete isolated GTK smoke passed in **15.27 seconds**, with fatal GLib criticals enabled. Expected missing-accessibility-bus warnings remain; accessibility is not validated. The initial run passed in 15.75 seconds and also emitted the previously observed, unexplained credential-socket broken-pipe warning.

The full headless suite passed: **56 Rust tests**, one graphical test intentionally ignored, and **27 Python tests**. Strict all-target Clippy and explicit formatting of the included benchmark module passed. No production browser/MCP code changed in this coverage slice; HTTP/HTTPS MCP graphical smokes were not rerun for it.

## Still open

This is not a complete filesystem/download failure matrix. Remaining cases include permission changes during transfer, ACL/portal/removable/network filesystems, inode exhaustion, successful external Reveal integration, abrupt process/power interruption, active-transfer recovery and capability-gated HTTP resume. Partial native output may remain after failure; the fixture does not delete user downloads or promise atomic file creation/integrity. A verified subset does not complete the whole [roadmap](../ROADMAP.md).
