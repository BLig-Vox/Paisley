# Session restoration and transient documents

Date: 2026-11-07. This describes the implemented session-metadata subset, not complete recovery or WebKit site-storage durability.

## Behavior

Normal profiles save bounded session metadata containing credential-free HTTP(S) addresses, tab order and active selection. The selected safe tab loads on restore; safe background tabs remain lazy until selected.

Temporary `blob:`, `data:`, `about:blank` and `about:srcdoc` documents instead save one fixed metadata marker, `browny:non-restorable`. Their payloads, original addresses/fragments and titles are not copied into the session record. Normalization happens before background snapshot queuing and again during persisted-session validation. Unsupported file, JavaScript, credential-bearing and unknown pseudo-address entries remain rejected.

The marker restores to a **native GTK label**, titled **Not restored**, explaining that the temporary document was not saved. The WebView is hidden and no temporary document or replacement website is loaded. The address bar is empty; Reload is disabled and its action also does nothing. Entering a safe web address or choosing Home explicitly replaces the placeholder. Tab order and active selection remain intact across subsequent closes/reopens.

The marker is not a navigable URL, search query, origin permission or automation grant. Address-bar submission of it is rejected. No HTML/JavaScript placeholder is rendered. These changes do not alter the separate policy allowing page-generated transient documents under WebKit's own origin/sandbox enforcement, or enable address-bar navigation to them.

Private profiles never write session records, including transient entries or the marker, and do not restore them. Explicitly saved files and OS/engine traces are outside this metadata guarantee.

## Format and privacy limits

The existing version-1 session structure remains `tabs: Vec<String>` plus `active`. The exact marker is an additional supported metadata entry; a marker with a suffix is not accepted. Older Browny versions whose validator accepts only HTTP(S) can drop marker entries and change selection. This is not a backwards-compatible downgrade/migration guarantee.

Safe web URLs can themselves contain sensitive paths/query parameters and remain stored. This is not URL-secret detection, encrypted storage, whole-profile backup, POST/form replay, full navigation-history restoration or erasure of records previously created by other versions. Disabling restoration clears the next session snapshot. Corrupt/future records remain subject to the existing preservation/refusal rules.

## Regression evidence

- The model regression initially dropped transient tabs and shifted selection. It now verifies two safe URLs surrounding five marker entries, original active selection, rejection of unsafe entries and a serialized record below 1 KiB despite a data payload exceeding 1 MiB. The payload and original transient addresses are absent.
- The native regression initially displayed **New tab** instead of **Not restored**. It now verifies native non-markup explanatory text, a hidden/unloaded WebView, empty address, disabled/no-op Reload, lazy background views, empty visit history, restart preservation, marker address-bar rejection and explicit safe navigation replacing the placeholder.
- The same native fixture loads an actual owned data document, then verifies that queued and persisted snapshots contain the fixed marker rather than its payload. Private no-write coverage includes a data address and marker.
- Fresh verification: **66 Rust headless tests**, **27 Python tests**, strict all-target Clippy, formatting, and debug/release builds passed. The complete isolated GTK regression passed in **24.03 seconds**. It also emitted a WebKit internal-load error, credential-socket warnings, debugger-trap diagnostics and expected absent-accessibility-bus warnings. A test pass does not resolve those diagnostics or earlier intermittent failures.
- The native `placeholder.png` capture was read and visually checked at 1280 × 800: explanation fits, address is empty, Reload is disabled, and neither original payload nor marker is displayed. Local ignored evidence: `target/session-placeholder-verification-a/placeholder.png`.

Reproduce with the Cargo-reported test executable, not the production binary:

```sh
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings
cargo fmt --check
cargo test --offline --locked --bin browny --no-run
python3 -I scripts/smoke_isolated.py \
  /absolute/path/to/target/debug/deps/browny-TEST_HASH \
  --screenshots /absolute/path/to/NEW-evidence-directory
```

Prefix development commands with `distrobox-host-exec` when tools are installed on the host. The graphical runner requires its existing Wayland session and mandatory namespace/private-D-Bus isolation; the output directory must not already exist. It captures other native fixtures as well as the placeholder and does not contact public sites.

## Still open

Full migration/recovery, site-data durability, pinned tabs/windows, broad display/accessibility/scale matrices and independent review are not established. The [restart investigation](RESTART_RELIABILITY.md), [Inspector evidence](INSPECTOR.md), [metadata interruption coverage](RECOVERY.md) and [roadmap](../ROADMAP.md) retain their separate limits. Both HTTP and HTTPS actual-binary MCP smokes subsequently passed on this build, including native trusted input, PNG, scope denial and Stop/cleanup. They ran independently, once each, with no retry; local logs are `target/placeholder-mcp_smoke.py-svshbnbu.txt` and `target/placeholder-mcp_https_smoke.py-5fn9i230.txt`. These owned-fixture passes do not establish the source of the earlier native-key preemption or erase uninstrumented failures.
