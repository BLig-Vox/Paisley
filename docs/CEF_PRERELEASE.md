# CEF backend status: 0.2.0-alpha.6

## Backend selection

CEF is the default starting with 0.2.0-alpha.3. WebKit is deprecated, available
only with `--no-default-features --features legacy-webkit`; no automatic fallback
is attempted. Published alpha.2 and earlier artifacts retain their original
backend defaults. This selection change does not complete CEF feature parity.
Servo is not included.

## Building and launching CEF

Linux x86_64 only has been exercised. Install the normal Paisley build dependencies, GTK4 4.12+ and the protobuf compiler (`protoc`). The pinned Rust dependency is cef-rs 156.6.0, whose matching CEF is 156.0.30 (Chromium 156.0.8078.96).

Download/extract that exact minimal CEF distribution using cef-rs's `download-cef` tooling, which also writes its required `archive.json`. `CEF_PATH` must point to that distribution or its versioned parent. A build with no `CEF_PATH` lets cef-rs fetch the runtime into its build directory, but Cargo does not install it with the executable. The runtime uses roughly 1.9 GB disk space. Its library, resource packs, locale directory, ICU data, V8 snapshot and sandbox are all required; copying `libcef.so` alone is insufficient.

From a checkout, with `CEF_PATH` set to the matching distribution:

```sh
python3 scripts/build-limited.py --limit-mb 3000 -- cargo build --locked
# CEF_PATH must be exported before the build.
export PAISLEY_CEF_RUNTIME="$CEF_PATH"
./target/debug/paisley --profile cef-preview
```

For the historical alpha.2 registry release, the corresponding command is `cargo install vox-paisley --version 0.2.0-alpha.2 --locked --features cef`; supply `CEF_PATH` while building and set both `PAISLEY_CEF_RUNTIME` and `LD_LIBRARY_PATH` to the actual runtime directory when launching that older single binary. Run that command through `scripts/build-limited.py --limit-mb 3000 --` to enforce the build cap. Current source installs use `cargo install --path . --locked` (both binaries).
The lightweight `paisley` launcher runs without loading CEF first; it locates the
runtime and sets `LD_LIBRARY_PATH` and `PAISLEY_CEF_RUNTIME` for `paisley-engine`.
Desktop installation points at the launcher, so external URLs use the same path.

Runtime lookup order:

1. Explicit `PAISLEY_CEF_RUNTIME`, then `CEF_PATH` (invalid explicit values fail).
2. `../lib/paisley/cef` relative to the launcher directory.
3. `$XDG_DATA_HOME/paisley/cef` or `~/.local/share/paisley/cef`.
4. The exported `CEF_PATH` recorded when compiling the launcher.

Each location may be the exact runtime, its `cef_linux_x86_64` parent, or the
versioned parent. Locations must be absolute. Keep the complete distribution,
including `include/cef_version.h`, locales, resources and sandbox. The launcher
checks required files and the pinned version header; this is **not** a signature
or a library checksum verification. Obtain and verify the distribution through
the upstream download tool. No code is downloaded automatically at startup.
Use `paisley --backend-info` to inspect selection; `paisley --help` works even
when the runtime is missing.

Run as an ordinary desktop user with Chromium's sandbox available. Follow CEF's Linux sandbox installation requirements for the exact distribution and operating system. Production builds do not accept the test-only sandbox override. The root Bot Screen tests used an explicit debug/test-support exception; that is not evidence of a validated secure non-root installation. The alpha.6 GitHub binary archive includes the matching CEF runtime and both executables; system GTK libraries and fonts remain required. No runtime is downloaded at startup.

## Verified behavior

The recorded native suite passed 60 tab creation/close cycles with checkpoints around 669–683 MiB PSS and no owned processes remaining after exit. Eight public sites used 1,642 MiB before five eligible inactive pages slept and 1,142 MiB afterwards. These are whole owned-process PSS measurements on this Bot Screen, not per-tab guarantees or a comparison against another browser. Timing was accelerated only in fixtures. Real YouTube playback advanced approximately five seconds without an error; codecs, DRM and long playback have not been exhaustively tested.

Native tests cover shared/private cookies, text/PNG/ZIP/PDF downloads, pauses and closed source tabs, clear-site-data, strict cookies/WebRTC, right-side DevTools, script dialogs, basic HTTP authentication, Unicode file upload and dropdowns. Japanese, Chinese and emoji text displayed; IME composition is not implemented. See [the evidence ledger](testing/2026-11-17/cef-progress.md) for exact builds, measurements and limits.

## Known gaps

- Desktop-video permission prompts are wired with cancellation and a deny default; full capture compatibility is unverified. Camera, microphone, capture audio and website notification integration remain incomplete. Existing WebKit feature documentation does not imply CEF parity.
- HiDPI screen scale is not implemented correctly. Use a scale-1 display for preview testing.
- IME composition is not wired; text rendering is not proof of input-method support.
- New-window handling creates independent HTTP(S) tabs and does not preserve `window.opener`/WindowProxy semantics. Some OAuth flows may fail.
- Source/Cargo installations still require a separate matching runtime. Secure non-root deployment has not been validated on Bot Screen, which runs as root.
- Native download fsync runs on the GTK thread and can briefly stall the interface. Interrupted/cancelled partials are removed when terminal state is handled; an abrupt whole-application/OS crash may leave a private staging directory requiring manual cleanup.
- Flatpak file portal support and fractional/HiDPI dropdown placement are not verified.

## Support crates

`paisley-engine-transport` is published as `0.1.0-alpha.1` and `paisley-cef-gtk` as `0.1.0-alpha.4` so Cargo can resolve the CEF graph. Their APIs are internal and may change. The GTK adapter retains its MPL-2.0 upstream notices; transport and root application are MIT. See THIRD_PARTY_NOTICES.md and engine/cef-gtk/UPSTREAM.md in the source repository.
