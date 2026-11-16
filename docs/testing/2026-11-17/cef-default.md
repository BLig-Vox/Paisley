# CEF default / WebKit deprecation verification

Unreleased source change after 28f8032, 2026-11-17.

## Automated checks

- Default CEF: `cargo check --locked --bins` passed.
- Explicit deprecated fallback: `cargo check --locked --no-default-features
  --features legacy-webkit --bins` passed.
- Default CEF unit/integration selection: `cargo test --locked --lib --bin paisley
  --bin paisley-engine --test wayland_identity`: **67 passed**, one display test
  ignored. This does not rerun the separate Wayland native matrix.
- Python discovery `test_*.py`, with PAISLEY_TEST_LAUNCHER set to the built
  launcher: **47 passed**. Covers nine executable-launcher cases, feature/binary
  contracts, package manifest/reproducibility, and existing supervisor checks.
- The launcher additionally passed three direct rustc unit tests under the cap.
- `readelf -d` launcher dependencies: only libgcc_s, libc and the ELF loader;
  no GTK or libcef dependency before runtime discovery.
- Independent whole-change review completed; source-launch instructions and
  legacy browser-test targets were corrected in response. No important issues
  remained in the final review.

All compiler/check/test builds ran through scripts/build-limited.py with
`--limit-mb 3000`: hard requested maximum 3,000,000,000 bytes, kernel page-rounded
maximum 2,999,996,416, build swap zero and Cargo jobs one. Cached build peak was
856.12 MiB for the native engine; no build reported OOM events. This is not a
clean-build benchmark or a runtime memory ceiling. See [exact reports](cef-default-limits.json).

## Bot Screen native check

Used temporary binaries in target/cef-default-native, leaving the installed
browser and existing user profiles unchanged. Launcher was built with ordinary
CEF defaults; native engine was a debug cef-test-support build. Bot Screen runs
as root, so only that temporary engine used the previously approved
PAISLEY_CEF_TEST_NO_SANDBOX=1 exception. Production sandbox policy is unchanged.

With LD_LIBRARY_PATH and CEF_PATH unset at launch, the explicit complete runtime
was discovered, libcef loaded, and Example Domain visibly rendered. A second
launcher invocation for the same cef_default profile exited zero and added
example.org to the existing window. Desktop installation into a temporary
XDG_DATA_HOME produced Exec pointing to **paisley**, not paisley-engine, and
exactly the bundled logo bytes. The fixture's --backend-info reported CEF
156.0.30 and the expected canonical runtime directory.

This check does not certify secure non-root sandbox setup, CEF feature parity,
Wayland dock behavior, private cookie isolation or long-session memory behavior.
Existing profile/application identity, private storage logic, residency policy
and engine child-process entrypoint were not modified. Earlier evidence remains
in cef-progress.md and the Wayland identity report.

Release preparation for alpha.3 reran the extracted crate against the published
support dependencies: 67 Rust tests passed, one display test ignored. The capped
package test peaked at 1,381.89 MiB with zero OOM events. Published alpha.2
continues to use its original feature defaults.
