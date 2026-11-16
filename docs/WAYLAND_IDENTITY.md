# Paisley window identity on Wayland

## Root cause

The installed launcher is `site.blig.Paisley.desktop` and its icon name is `site.blig.Paisley`. GApplication IDs deliberately include a hex-encoded profile ID, for example `site.blig.Paisley.p64656661756c74`, with `.Private` for private launches. Those IDs separate D-Bus instances and external-link routing. GTK also used them as Wayland `xdg_toplevel.app_id`, but there are no matching desktop entries for those profile IDs. A dock could therefore display a separate generic item even while the launcher showed the correct pink logo.

Changing every GApplication ID to the same value would merge normal-profile routing. Generating desktop entries per profile would create separate launcher identities and expose private profile details. Neither is needed.

## Fix

Keep the profile-specific GApplication IDs, HANDLES_OPEN and private NON_UNIQUE flags. In `desktop_install::configure_application`, attach a handler to every application window's `map` signal. On a Wayland toplevel, use GDK's public `set_application_id` API to send the shared `site.blig.Paisley` desktop ID. X11 surfaces fail the checked downcast normally and are not passed to Wayland APIs. Window icon names use the same constant as user-local icon installation.

The callback runs after GtkWindow's default map handler. This timing matters: in GTK 4.14, GDK's Wayland setter silently returns before the native xdg_toplevel exists; a realize-only handler did not fix the bug. The map callback runs after creation, before the first rendered buffer, and repeats after unmap/remap or surface recreation. Application error windows get the same hook. D-Bus identities remain profile-specific, including GTK's separate D-Bus integration properties.

The packaged desktop entry already had the correct filename, Icon and `%U` forwarding argument, so its behavior is preserved. A regression checks its identity against the window/install constants. Installation still writes the bundled pink PNG and the canonical desktop file. No user profile, cookie database or desktop settings are reset.

Upstream references: [Wayland toplevel API](https://docs.gtk.org/gdk4-wayland/method.WaylandToplevel.set_application_id.html) and [GTK 4.14 Wayland implementation](https://github.com/GNOME/gtk/blob/4.14.5/gdk/wayland/gdktoplevel-wayland.c).

## Verification on 2026-11-17

Bot Screen uses X11, so tests created a separate Weston 13 Wayland compositor inside it, with a private D-Bus session, isolated HOME/XDG directories and loopback-only fixture pages. The WebKit default backend ran the actual CLI. No engine sandbox override was added.

Before the fix, all five browser windows advertised profile-specific IDs; the same test failed. After the fix:

- Two normal profiles advertised `site.blig.Paisley` and retained distinct cookies.
- Repeated launches forwarded HTTP links to each correct existing profile, returned success and created no second toplevel in the forwarding client.
- Three private instances across two profiles remained independent, including two simultaneous private instances for the same profile. Private cookies did not enter normal profiles or the other private instance.
- All five browser windows used the canonical ID before their first rendered buffer.
- A real malformed-profile error window also used the canonical ID.
- A focused native test unrealized/recreated the window surface; both distinct Wayland toplevels used the canonical ID and the GApplication ID stayed unchanged.
- The actual `--install-desktop` output resolved through Gio.DesktopAppInfo and GTK IconTheme to the installed desktop entry and byte-identical bundled pink logo.
- Normal completion and injected SIGTERM interruption left no tracked owned processes running.

Rust regressions: 49 library, 24 binary and 1 integration tests passed; two display-dependent cases remained ignored in the default command. The new Wayland case was run explicitly inside Weston. Python: 36 tests passed. The optional CEF binary passed `cargo check --features cef --bins`; the runtime identity matrix used WebKit.

All compilation used `--limit-mb 3000`: requested limit 3,000,000,000 bytes, kernel limit 2,999,996,416 bytes, swap zero, one Cargo job. Cached successful build/test cgroup peaks ranged from 442.64 to 1105.53 MiB; this is not a clean-build benchmark. Exact reports and before/after results are under [testing/2026-11-17/wayland-identity](testing/2026-11-17/wayland-identity/).

**Limits:** Native Wayland identity and desktop/icon resolution are verified. GNOME, KDE and COSMIC dock presentation was not visually tested; Weston has a different shell. This fix is included in 0.2.0-alpha.2; it is absent from 0.2.0-alpha.1.

## Repeat the native check

Install Weston, Python GTK4 introspection and the normal Paisley prerequisites. In the Bot Screen terminal, on its X11 desktop:

```sh
dbus-run-session -- python3 scripts/test-wayland-identity.py \
  --binary /absolute/path/to/the/fixed/paisley \
  --test-binary /absolute/path/to/the/wayland_identity/test-executable \
  --evidence target/wayland-identity-check
```

Build the CLI and focused test with `python3 scripts/build-limited.py --limit-mb 3000 -- cargo build --bin paisley` and `python3 scripts/build-limited.py --limit-mb 3000 -- cargo test --test wayland_identity --no-run`. The optional `--test-binary` is the executable Cargo reports for `tests/wayland_identity.rs`. Omit it to run only the real CLI matrix. `--interrupt-after-first-window` exercises SIGTERM cleanup and must exit 143 with an interrupted result and no tracked owned processes.

After installing the fixed build, run `paisley --install-desktop` to refresh its launcher and logo, then close and reopen existing browser windows so they use the new code.
