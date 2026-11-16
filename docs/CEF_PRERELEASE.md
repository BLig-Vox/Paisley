# CEF prerelease: 0.2.0-alpha.1

## Backend selection

The default Cargo install and GitHub binary use WebKitGTK. This is deliberate: the optional CEF implementation has known missing functionality and is not the production default. It does not include Servo. Do not interpret the release as a completed migration.

## Building the optional CEF backend

Linux x86_64 only has been exercised. Install the normal Paisley build dependencies, GTK4 4.12+ and the protobuf compiler (`protoc`). The pinned Rust dependency is cef-rs 156.6.0, whose matching CEF is 156.0.30 (Chromium 156.0.8078.96).

Download/extract that exact minimal CEF distribution using cef-rs's `download-cef` tooling, which also writes its required `archive.json`. `CEF_PATH` must point to that distribution or its versioned parent. A build with no `CEF_PATH` lets cef-rs fetch the runtime into its build directory, but Cargo does not install it with the executable. The runtime uses roughly 1.9 GB disk space. Its library, resource packs, locale directory, ICU data, V8 snapshot and sandbox are all required; copying `libcef.so` alone is insufficient.

From a checkout, with `CEF_PATH` set to the matching distribution:

```sh
python3 scripts/build-limited.py -- cargo build --features cef
export PAISLEY_CEF_RUNTIME="$CEF_PATH"
export LD_LIBRARY_PATH="$PAISLEY_CEF_RUNTIME${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
./target/debug/paisley --profile cef-preview
```

For registry installation, the corresponding command is `cargo install vox-paisley --version 0.2.0-alpha.1 --locked --features cef`; supply `CEF_PATH` while building and the same two runtime variables when launching. Run that command through `scripts/build-limited.py --` to enforce the build cap. If using a versioned parent as `CEF_PATH`, set `PAISLEY_CEF_RUNTIME` to the actual child directory containing `libcef.so`, not the parent. Persist runtime variables in a separate user launcher if adding a CEF build to the application menu; `--install-desktop` does not configure the CEF runtime.

Run as an ordinary desktop user with Chromium's sandbox available. Follow CEF's Linux sandbox installation requirements for the exact distribution and operating system. Production builds do not accept the test-only sandbox override. The root Bot Screen tests used an explicit debug/test-support exception; that is not evidence of a validated secure non-root installation. No CEF binary archive or automatic runtime installer is provided in this prerelease.

## Verified behavior

The recorded native suite passed 60 tab creation/close cycles with checkpoints around 669–683 MiB PSS and no owned processes remaining after exit. Eight public sites used 1,642 MiB before five eligible inactive pages slept and 1,142 MiB afterwards. These are whole owned-process PSS measurements on this Bot Screen, not per-tab guarantees or a comparison against another browser. Timing was accelerated only in fixtures. Real YouTube playback advanced approximately five seconds without an error; codecs, DRM and long playback have not been exhaustively tested.

Native tests cover shared/private cookies, text/PNG/ZIP/PDF downloads, pauses and closed source tabs, clear-site-data, strict cookies/WebRTC, right-side DevTools, script dialogs, basic HTTP authentication, Unicode file upload and dropdowns. Japanese, Chinese and emoji text displayed; IME composition is not implemented. See [the evidence ledger](testing/2026-11-17/cef-progress.md) for exact builds, measurements and limits.

## Known gaps

- Website notification and sensitive permission integration is incomplete. Existing WebKit feature documentation does not imply CEF parity.
- HiDPI screen scale is not implemented correctly. Use a scale-1 display for preview testing.
- IME composition is not wired; text rendering is not proof of input-method support.
- New-window handling creates independent HTTP(S) tabs and does not preserve `window.opener`/WindowProxy semantics. Some OAuth flows may fail.
- Matching runtime distribution, a relocatable launcher and secure non-root deployment remain release blockers for making CEF the default.
- Native download fsync runs on the GTK thread and can briefly stall the interface. Interrupted/cancelled partials are removed when terminal state is handled; an abrupt whole-application/OS crash may leave a private staging directory requiring manual cleanup.
- Flatpak file portal support and fractional/HiDPI dropdown placement are not verified.

## Support crates

`paisley-engine-transport` and `paisley-cef-gtk` are published as `0.1.0-alpha.1` so Cargo can resolve the optional CEF graph. Their APIs are internal and may change. The GTK adapter retains its MPL-2.0 upstream notices; transport and root application are MIT. See THIRD_PARTY_NOTICES.md and engine/cef-gtk/UPSTREAM.md in the source repository.
