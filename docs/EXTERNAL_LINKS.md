# External web links

`paisley [--profile ID] [--private] https://example.com [https://example.org ...]`
opens each link in a new tab. Normal windows share GApplication routing by profile;
private invocations remain independent. Existing tabs are retained. A cold launch
restores the profile normally before adding the requested links.

The desktop entry uses `Exec=paisley %U` and advertises only HTTP/HTTPS scheme
handlers. `paisley --install-desktop` embeds and installs this updated entry and
the logo. Select Paisley in desktop preferences or use `gio mime
x-scheme-handler/http site.blig.Paisley.desktop` and the equivalent HTTPS command.
Desktop installation alone does not change the user's browser preference.

The entry does not claim HTML files: external file paths, `file:`, `javascript:`,
`data:`, unknown schemes, credential-bearing URLs and control characters are
rejected. Up to 32 links of 8192 bytes each are accepted per command. Incoming
GApplication open messages are validated again and capped at 32 links.

Implementation follows Gio's primary-instance open signal and freedesktop `%U`:
- https://docs.gtk.org/gio/signal.Application.open.html
- https://specifications.freedesktop.org/desktop-entry/latest/exec-variables.html

## Manual checks on Bot Screen, 2026-11-16

- Release build completed; desktop-file-validate accepted the installed entry.
- Cold HTTP/HTTPS URL launch opened a new tab in the default profile.
- A second invocation with two URLs opened two more tabs in the same window.
- XFCE `exo-open --launch WebBrowser` opened the supplied URL in that window.
- GIO HTTP/HTTPS preferences were set to `site.blig.Paisley.desktop`.
- `gio open` dispatched an HTTP link to Paisley.

Existing VNC graphics/a11y and startup image-allocation warnings remain in runtime
logs. No automated tests were added or run. The published 0.1.1 crate predates
this feature; install the current GitHub source until the next release.
