# Interactive MCP origin workspace

Paisley has an opt-in **interactive**, provider-neutral stdio MCP adapter. It can
click, type Unicode text, press keys, scroll, navigate, inspect and capture the
rendered page. It is not a read-only substitute and does not require a browser-owned
LLM or any particular model provider.

**Current scope: one explicitly selected HTTPS origin or owned loopback fixture,
one ephemeral window, on Wayland.** General HTTPS needs native workspace consent
and native single-use approval for every navigation/input operation. This is not
unrestricted browsing, personal-tab access, production hardening or complete
roadmap delivery. Implementation was validated on owned fixtures only, not live sites.

## Requirements and startup

Build Paisley normally. These tools must already be installed on the host:

- Python 3 (standard library only; the adapter runs with `-I`).
- `bubblewrap` with working user/network/PID/mount namespaces.
- `dbus-run-session` and GTK4-compatible `WebKitWebDriver`.
- A graphical Wayland session with an accessible `wayland-N` display socket.
- An installed public PEM CA bundle for general HTTPS.
- `openssl` additionally for the owned HTTPS smoke test's disposable certificates.

Do not install packages automatically. There is no less-isolated fallback when a
requirement is unavailable. X11 and other platforms are not supported by this
automation launcher; normal browsing is unchanged.

For an origin you explicitly intend to control and share with the connected MCP
client, configure the executable and a **canonical origin**, for example:

```json
{
  "mcpServers": {
    "paisley": {
      "command": "/absolute/path/to/Browny/target/debug/paisley",
      "args": ["--mcp", "--allow-origin", "https://app.example.org"]
    }
  }
}
```

The example is configuration syntax, not a tested or preapproved live destination.
Origins have no path, credentials, query or fragment; use lowercase ASCII/punycode
hostnames and omit the default HTTPS port. A nondefault port is permitted. The host
broker resolves general HTTPS once and rejects non-public/private/loopback/link-local/
multicast/reserved or mixed answers. Connections use only the pinned numeric peers,
not a fresh DNS lookup. At most 16 resolver answers are accepted. A redirect does
not add another origin to the grant, even on the same IP.

For an owned local HTTP fixture, start its server separately and use:

```sh
/absolute/path/to/paisley --mcp --allow-origin http://127.0.0.1:8000
```

For an owned HTTPS loopback fixture, explicitly provide its **public CA certificate**:

```sh
/absolute/path/to/paisley --mcp --allow-origin https://127.0.0.1:8443 --fixture-ca /absolute/path/to/public-ca.pem
```

Fixture origins require the exact host `127.0.0.1` and an explicit nondefault,
nonzero port. `--fixture-ca` is rejected for public origins or plain HTTP. Its file
must be a bounded regular, non-final-symlink PEM file containing only public
certificates, not a private key. Public certificates are copied into the disposable
namespace; host trust is never changed. The fixture server keeps its key outside
the namespace. **WebKit still validates TLS certificates/hostnames;
`acceptInsecureCerts` remains false.**

Only explicitly configured loopback fixtures auto-grant control for testing.
General HTTPS never auto-grants. The first browser tool opens a clearly labeled
workspace and native Grant/Deny dialog. Until Grant, tools return a native-grant
error; approve in the real window, then retry. The dialog explains the ephemeral
profile, real-world side effects and that page text/screenshots may reach the MCP
client **and its model provider**. Local transport is not local-only model processing.
The grant expires after ten minutes. No normal Profile, personal cookies, saved
tabs, settings or DuckDuckGo startup are opened.

## Tools and interaction flow

1. Initialize MCP and send `notifications/initialized`.
2. `browser_tabs` starts the workspace and, after any required native grant,
   returns its single tab handle.
3. `browser_navigate` requests a URL in the selected origin. General HTTPS shows
   the bounded destination in a native single-use approval dialog.
4. `browser_observe` returns bounded inspection text, origin, tab, navigation
   generation, viewport, scroll position, scale, zoom, capture time, trusted cursor
   metadata and an actual PNG. Set `screenshot: false` for text only.
5. Use the **single-use `observation`** for one input. General HTTPS requires a
   separate native Allow once/Cancel decision. Plain-text target details are labeled
   untrusted; they cannot grant authority. The requested typed value is not included.
6. The adapter rechecks origin/tab/generation, bounded DOM/layout state and target
   identity/details after approval. Navigation or changed text/destinations can
   invalidate input even when geometry is unchanged. Observe again after failure.
7. `browser_stop` permanently revokes the workspace. Start a new explicitly selected
   process for another session. Stop cannot undo a request already sent to a site.

| Tool | Operation |
| --- | --- |
| `browser_tabs` | List the single approved ephemeral tab. |
| `browser_observe` | Rendered screenshot and bounded DOM/geometry inspection. |
| `browser_navigate` | Same-grant navigation; no external protocols. |
| `browser_click` | Native WebKit click on the approved CSS-selected element handle. |
| `browser_click_at` | Native viewport-coordinate click with bounds/hit-test checks. |
| `browser_type` | Native key input into an element, optionally clearing first. |
| `browser_key` | Press/release Enter, Tab, Escape, Backspace, arrows, PageUp/PageDown, Home or End. |
| `browser_scroll` | Native wheel input, not a DOM `scrollBy` substitute. |
| `browser_wait` | Wait for an element, at most five seconds. |
| `browser_stop` | Close the browser/driver and reject subsequent commands. |

Text is bounded to 32,768 characters, with an independent 128 KiB request-frame
limit. Invalid Unicode, non-text controls and reserved WebDriver private-use key
codepoints are rejected; use `browser_key` for keys. Native key actions preserve
supplementary Unicode/emoji. There is no DOM value-setting or `element.click()`
fallback. General HTTPS rechecks native generation/grant and active element before
each character; owned fixtures use batches of up to 128 characters. A changed focus
stops the remaining text, but input already delivered cannot be retracted.

The trusted native chrome shows the origin, operation and Stop. A cursor/click
marker is outside the page DOM. Off-screen selector targets have an operation
indicator while WebKit scrolls them into view; an old marker is not their exact
click position. Native keyboard input or a click in the page preempts the runtime.
Native consent windows remain keyboard-operable. Physical desktop preemption has
not been independently exercised by these fixtures.

## Isolation, limits and security model

The user separately approved local backend work and the explicit HTTPS-origin
implementation/owned-fixture validation extension. Neither approval authorizes
personal-profile access, arbitrary live-site actions or a public debugger.

- Inherited stdio carries MCP. Worker/driver/browser use mandatory private user,
  network, PID, IPC and mount namespaces. Raw WebDriver is never host-visible.
- The fixed trusted executable and `/usr` are read-only. HOME/cache/temp state are
  disposable; personal profiles, host session bus and private keys are not mounted.
- A private service-free D-Bus avoids desktop-service autoactivation. The required
  Wayland socket is deliberately shared, not isolation from the compositor or
  hostile same-user software. Software rendering and no accessibility bus are
  automation restrictions, not normal-browser settings.
- HTTP fixtures use the fixed-origin bounded-body relay. HTTPS uses **only an exact
  authority CONNECT** to pinned peers, with WebKit performing end-to-end TLS. No
  TLS interception, arbitrary proxy destination or engine TLS bypass is added.
- Before nonblank navigation, the native runtime requires a compiled deny-all/
  allow-exact-origin request filter, plus navigation policy and workspace grant.
  This is a second layer for requests that might reuse/coalesce TLS connections.
  Filter failure leaves control/navigation denied, not a less-restricted fallback.
- Proxy/relay concurrency is capped at eight connections per side. CONNECT headers
  are capped at 16 KiB/five seconds. Each tunnel is capped at 32 MiB aggregate bytes,
  120 seconds lifetime and 15 seconds idle. Caps can interrupt large/long-lived pages;
  they are not an aggregate CPU/memory/session-budget system. HTTP fixture bodies
  remain bounded to 1 MiB requests and 16 MiB responses.
- Staleness inspection fails closed beyond 524,288 DOM string characters or 5,000
  body descendants. Observation text is capped at 12,000 characters/100 candidate
  elements. Native approval details are capped at 2,048 printable ASCII characters
  (Unicode is escaped); complex targets/URLs may be rejected, never silently
  truncated into an indistinguishable approval.
- Token-authenticated private IPC permits status/action display, Stop and asking
  for native authorization—not supplying a grant decision or executing commands.
  Native per-action decisions have a 120-second deadline and bind to generation.
  Ordinary idle reads are not disconnects; the ten-minute native grant still expires.
- The stdin reader bounds pending frames to 16 and watches EOF even while native
  consent is pending. EOF/overflow interrupts the owner channel; Stop/EOF reap the
  owned workspace and requests after revocation do not reopen it.
- Permission requests, downloads, file choosers and extra windows remain denied.
  No host-shell, arbitrary-file, raw-cookie/token-export, clipboard, arbitrary-JS
  or desktop-wide capture/control tool exists.

**Limitations:** strict one-origin networking breaks third-party dependencies and
cross-origin login flows. Inspections/target details are untrusted page data;
checks and native input are not atomic and do not prove a click's semantic safety.
Rapid script/focus/layout changes, event-handler changes, frames/dialogs, long
input and physical interruption still need a broader race matrix. Per-action
consent is not a reliable payment/message/deletion classifier. No live public-site
compatibility or two-independent-production-client interoperability is claimed.
Personal-tab pairing, privileged debugging, upload/download approvals, aggregate
resource budgets, accessibility and other display backends remain separate work.

## Bounded revocation diagnostics

The native runtime writes **one fixed cause code to stderr on its first Stop**:
`Automation stopped: CODE`. Codes distinguish Stop-button/protocol Stop, native
keyboard/pointer preemption, owner-window close, denied/expired workspace grant,
action-consent Stop, unavailable/non-ephemeral automation session, and control-owner
disconnection. Reentrant close callbacks do not add another Stop line.

These lines contain no origin, token, key identity, selector, page content, typed
value or request payload. Paisley does not create a persistent diagnostic log;
the caller may retain stderr under their own policy. Engine/driver stderr remains
outside this fixed-code guarantee and should not be shared without inspection.

This does **not** auto-restart or renew a revoked workspace. A native input code
would establish that the native input handler revoked that run, not why the event
arrived. A control-disconnection code does not by itself distinguish EOF, timeout,
invalid framing or a native-process failure; further investigation is required.
An abrupt process exit may leave no Stop line. Shared Wayland focus/input and the
known native reliability failures remain limitations.

On 2026-11-07, the instrumented HTTP fixture passed trusted native input and cleanup.
The next HTTPS fixture failed before its first navigation and stderr recorded
`Automation stopped: native-key-preemption` (`target/stop-cause-https-fchrh2vl.txt`).
That establishes invocation of the native keyboard revocation handler for **that
run**, not the key's source, an engine defect, or the cause of earlier uninstrumented
HTTP failures. Keyboard preemption was not disabled and no automatic retry was
added. In the same verification command the full GTK regression passed in 25.31s,
including native action-consent Stop and workspace Deny cause codes, but also
printed a WebKit internal-load error; a passing assertion set is not warning-free
engine/reliability evidence (`target/stop-cause-gtk-pgdb1fio.txt`).

## Verification

From `Browny/`, prefix host commands with `distrobox-host-exec` when needed:

```sh
python3 -I tests/test_mcp.py
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings
cargo build --offline --locked
cargo test --offline --locked --bin paisley browser_smoke -- --ignored --test-threads=1
python3 -I tests/mcp_smoke.py /absolute/path/to/target/debug/paisley
python3 -I tests/mcp_https_smoke.py /absolute/path/to/target/debug/paisley
```

The 20 headless Python tests cover protocol/schemas/input bounds, exact HTTP/HTTPS
scope, CONNECT authority, public/mixed peer validation, fixture trust restrictions,
CA-file rejection, native-grant gating, post-approval target changes, bounded
tunnel forwarding, EOF during consent and queue overflow. Resolver tests substitute
only DNS answers and never query public DNS.

The single GTK-thread smoke drives the actual native Grant/Deny and per-action
Cancel/Allow/Stop buttons, checks displayed target details, generation/expiry
rejection and actual filter compilation without navigating to a public origin.
A Unix-socket Rust regression proves idle timeout does not revoke and EOF does.
Native consent callbacks and backend post-approval guards are separately exercised;
this is not a complete public-origin end-to-end human-interaction/race matrix.

The real HTTP smoke verifies trusted Unicode/emoji/key/form input, selector and
coordinate clicks, wheel, PNG, scope/staleness/foreign-subresource rejection,
distinct network namespaces and Stop/EOF descendant cleanup. The HTTPS smoke uses
owned TLS servers and disposable certificates to verify trusted Unicode/click/wheel
input, PNG, foreign-origin denial, unchanged-geometry label-mutation rejection,
wrong-CA rejection before any HTTP fixture request and Stop cleanup. Neither uses
personal profiles or live websites. Two protocol versions are **not** two clients.

An optional second smoke argument saves the PNG to a **new** path without overwrite:

```sh
python3 -I tests/mcp_https_smoke.py /absolute/path/to/paisley /new/empty/directory/https.png
```

The GTK smoke optionally captures native windows with `PAISLEY_GRANT_SCREENSHOT` and
`PAISLEY_CONSENT_SCREENSHOT` set to new absolute PNG paths. The recorded grant,
per-action prompt and rendered HTTPS Unicode/trusted-event captures were visually
checked. Test evidence is not a full security audit or production-hardening claim.
