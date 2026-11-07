# Explicit HTTPS automation grants

Date: 2026-11-07

## Intent and authorization

The user explicitly selected **Approve HTTPS-origin support** after disclosure that the current policy permits only local fixtures. This authorizes design/code changes and owned HTTPS-fixture validation, not live-site actions, personal-profile access, package installation, external publication or TLS bypasses. The browser must remain genuinely interactive: native clicks, arbitrary bounded Unicode input, keys, wheel scrolling, screenshots and inspection.

## Chosen architecture

Keep Rust/GTK4/WebKitGTK, the embedded provider-neutral standard-library Python MCP adapter, mandatory private namespaces, private D-Bus, fixed executable and ephemeral profile. Extend the fixed-origin relay to HTTPS using an exact-authority CONNECT tunnel to a pinned approved peer. WebKit performs end-to-end TLS certificate/hostname validation. No generic proxy, unrestricted tunnel, public debugger or general shell/JavaScript tool is added.

A grant selects exactly one canonical HTTPS origin. The host broker resolves it once and rejects non-public, loopback, private, link-local, multicast, reserved and mixed public/private results. It connects only to those validated pinned addresses and never re-resolves while forwarding. The only loopback HTTPS exception is an explicitly configured owned fixture with its bounded public CA certificate; it cannot authorize another host. Existing exact HTTP loopback fixtures remain supported.

The namespace receives a read-only public trust bundle, never private keys or host identity/profile/session-bus files. Fixture CA trust is scoped to that disposable fixture namespace; it does not change host trust or set acceptInsecureCerts. General origins use the host's public trust anchors without disabling WebKit TLS validation.

## Independent enforcement layers

- Outer broker: fixed origin/authority, pinned peer, at most 16 resolver answers, eight concurrent connections per side, CONNECT headers limited to 16 KiB/five seconds, and each tunnel limited to 32 MiB aggregate bytes/120 seconds lifetime/15 seconds idle; no redirects/DNS changes broaden peers. These are per-connection limits, not aggregate CPU/memory/session budgets.
- Native browser: exact origin navigation policy plus preinstalled deny-all/allow-exact-origin request filter, including requests that could otherwise reuse/coalesce a TLS connection. Failure to compile the filter prevents navigation.
- Native grant UI: before general HTTPS access, show the exact origin, ephemeral scope, data-sharing disclosure and side-effect warning. Native Grant/Reject only; page/MCP content cannot approve itself. Grant expires after ten minutes.
- Native per-action consent for non-fixture navigation/input, rather than pretending to reliably classify payments/messages/deletion from page semantics. Approval shows bounded plain-text destination/target details labeled untrusted, never the requested typed value. It is single-use and bound to current navigation generation; tab/generation, bounded DOM/layout and target identity/meaningful state are rechecked after the decision. Same-geometry label/destination changes must invalidate input. Checks/input are not atomic or a semantic-safety guarantee.
- Existing Stop/disconnect/native preemption, permission/file/download/popup denial, fixed binary, no personal profiles, and default TLS/origin/process sandbox remain.

Read-only status/action-display IPC must not grant access. Authenticated `authorize` requests may ask for native approval, not supply a decision. Typed values and page text are not logged or echoed in approval prompts. Observation itself requires the workspace grant because screenshots/text may go to the client's model provider.

## Alternatives considered

1. **Unrestricted host-visible WebDriver or arbitrary network access:** rejected; contradicts the existing isolation boundary and consent scope.
2. **TLS-intercepting proxy with generated origin certificates:** rejected; adds a new TLS termination/trust boundary and changes the meaning of engine validation.
3. **Pinned exact-authority CONNECT plus native request filtering and explicit native consent:** selected as the smallest extension preserving end-to-end engine TLS and private driver ownership. It costs strict one-origin compatibility, no silent third-party dependencies, connection quotas, and native approvals for general input.

## Verification and limits

Use only disposable owned HTTPS servers and generated fixture certificates. Verify trusted Unicode input/form submission, native clicks/wheel, real PNG, wrong-origin/foreign-subresource denial, certificate rejection, exact CONNECT parsing, non-public-address rejection, grant/consent/revocation and process cleanup. No live website is contacted during implementation validation. Two protocol versions are not two independent production clients; broader frame/focus/layout/human-preemption/display/interoperability matrices remain separate exit criteria. Do not claim complete roadmap completion from this slice.
