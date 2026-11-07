# Profile metadata interruption coverage

Date: 2026-11-07. This verifies a bounded **process-crash metadata subset**, not full profile/site-storage or power-loss recovery.

## Actual child-process interruption

`tests/persistence_crash.rs` launches its own fixed Cargo integration-test executable against new owned mode-0700 temporary roots. It uses the production `Profile::open_at`, `save_session`, `load_session`, profile lock and atomic record writer. No GTK desktop, browser launch, network, personal profile, production fault hook or engine setting is involved.

Each saved session has 200 distinct safe fixture URLs and a nonzero active index. URLs are only strings in metadata; they are never fetched.

- **Three staged-write cases:** the parent observes a `.browny-CHILD_PID-…` staging file while the child continuously writes one fixed replacement snapshot, then sends actual SIGKILL and waits/reaps the child. Reopening must acquire the released profile lock and read either the complete seed or complete replacement session, with the matching active index and all URLs. Empty, malformed or mixed snapshots fail. Existing settings bytes must remain unchanged.
- **One acknowledged-commit case:** the child completes a production session save and publishes a separate owned marker, then blocks on its parent-owned stdin. The parent observes the marker, sends SIGKILL and reaps it. Reopening must recover the exact acknowledged replacement session and unchanged settings bytes.
- A child guard kills/reaps a still-running writer during assertion unwinding. A timeout or early child exit fails the test; it is never accepted as crash coverage. Successful cases clean only their new owned fixture roots. Failed-run metadata may remain for inspection.

Staging observation and signal delivery are not atomic: the staged-write child may reach rename before SIGKILL arrives. The test therefore accepts either whole snapshot and does not claim interruption at every filesystem instruction. The committed case separately requires preservation of an acknowledged commit. Neither case simulates power loss, cache loss, kernel failure or storage hardware behavior.

## Regression strength

The test passed existing production handling on its first execution (0.39s); no production recovery fix was needed. To check that the assertion was not vacuous, a temporary verification mutation removed the old record before serialization/replacement. The test then failed with **`interrupted write produced neither whole valid snapshot`** (0.07s), not merely a child-start error. That mutation was immediately removed. With the original atomic writer restored, the complete suite passed; the final recovery test completed in 0.26s. Debug and release binaries were rebuilt after restoring the writer.

This is coverage plus a verified mutation check, not a fabricated missing-feature RED→GREEN claim.

## Reproduce

```sh
cargo test --offline --locked --test persistence_crash -- --nocapture
cargo test --offline --locked
```

Prefix commands with `distrobox-host-exec` when the development toolchain is installed on the host. No packages or mounts are installed/created on the host beyond the test's owned temporary directories.

Final headless verification after this addition: **65 Rust tests** (48 library, 16 binary, 1 integration), one graphical test intentionally ignored; **27 Python tests**; strict all-target Clippy; formatting including the separately included benchmark file; debug/release builds.

## Remaining recovery gates

- WebKit cookies/localStorage durability and the [observed NetworkProcess/storage failures](RESTART_RELIABILITY.md) are separate from application JSON records.
- Abrupt production CLI/browser shutdown, background-writer interruption at each stage, cross-file consistency and migrations need broader coverage. Independently atomic records are not one transaction.
- No automatic repair, migration, reset or deletion of corrupt/future profile records is added. Existing preservation/refusal behavior remains.
- No stale-temp sweep, failed-profile recovery wizard, interrupted-download recovery, filesystem/power-loss matrix or independent review is claimed.

The [roadmap](../ROADMAP.md) must retain these remaining gates rather than treating the process-kill subset as complete recovery.
