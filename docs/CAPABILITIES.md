# Capability evidence and remaining boundaries

Recorded 2026-11-07 against GTK 4.24.1 / WebKitGTK 2.54.2, GTK4-compatible 6.0 API,
with the project's `webkit6` 0.6 bindings. This is an implementation matrix, not a
security audit, a threat-model substitute or LibreWolf/Brave parity claim.

| Requirement | Current status and evidence | Gap / next validation |
| --- | --- | --- |
| TLS validation, origin enforcement, engine sandbox | Engine defaults retained; no certificate bypass or sandbox-disable option. Nested WebKit sandbox runs inside automation namespaces. Owned HTTPS automation fixture verifies end-to-end engine TLS, rejection of an unrelated CA before an HTTP request, foreign-origin denial and scoped public fixture trust without host trust changes. | Broader normal-browser certificate/mixed-content/origin/process-security matrix; upstream patch/update policy. |
| HTTPS-only browsing | Not implemented. Address heuristics prefer HTTPS for public hosts, but explicit HTTP remains allowed. | Upgrade/fallback policy, exceptions and deterministic certificate/redirect tests. |
| Tracking prevention | Native ITP enabled; graphical getter assertion passes. | Setter state is not behavioral proof of cross-site tracking resistance; storage/network fixtures required. |
| Third-party cookies | Optional Strict uses native NoThirdParty policy. Profile site-data persistence/isolation is tested. | Strict cross-site cookie/storage behavior and compatibility matrix not yet demonstrated. |
| WebRTC | Optional Strict disables WebRTC per view using engine settings. | Exposure/network behavior and site exception tests; not an anonymity guarantee. |
| Native request blocking | Twelve-domain starter compiles before enabled navigation. Custom native fixture filters stop requests at the server; related views, exceptions, navigation and BFCache regressions pass. | Full maintained lists/translation, update validation/rollback, coverage/overhead, diagnostics and breakage reporting. No invented exact counters. |
| DuckDuckGo promotions | Top-frame HTTPS-domain-restricted stylesheet, dynamic/scoping graphical fixtures. | Cosmetic cleanup only; not general ad/tracker request blocking. |
| Permissions | Explicit deny for every permission-request signal, with a native explanation. Real geolocation fixture receives PERMISSION_DENIED and displays the reason. | Safe requesting-origin attribution, per-origin prompts/grants/revocation and camera/microphone/notification/clipboard fixtures are not implemented. See below. |
| Hyperlink auditing / DNS prefetch | Installed engine exposes deprecated/ineffective setters; no functioning protection advertised. | Engine capability decision, not pretend toggles. |
| Referrers / GPC / DNT | No separately verified Paisley controls. | Capability/behavior research and fixture verification; signals alone are not tracking prevention. |
| Fingerprinting resistance | No verified anti-fingerprinting system. | Research engine coverage and compatibility; do not substitute spoofing-script claims for security. |
| DNS / proxy | Normal browsing uses engine/OS defaults. MCP uses private networking, a fixed HTTP fixture relay or exact HTTPS CONNECT to once-resolved validated public peers; private/mixed results fail closed. Unit resolver tests use supplied answers, not public DNS. | No normal-browser DNS/proxy privacy or system-wide VPN claim. No live public-origin compatibility validation; one origin deliberately excludes third-party dependencies. |
| Profiles / sessions | Normal cookies/local storage and tab order/selection survive clean restart; separate profile isolated; private records never written; locks/corrupt/future-version/symlink tests pass. | Version migrations/recovery, crash/interruption matrix, rename/delete/registry and encrypted storage. Same-user filesystem races are not an OS boundary. |
| Bookmarks / history | Bounded per-profile canonical credential-free HTTP(S) records; native searchable managers; Ctrl+D/Ctrl+H actions. Model tests cover limits, deduplication, title sanitization/late updates, search, persistence, isolation and corrupt/future records. Graphical fixture covers search, restart, opt-out/private behavior, committed unfinished documents, SPA/fragment navigation, failed-navigation exclusion, Cancel/Remove confirmation paths and persisted deletion. Worker regression proves session-save failure does not prevent library save; graphical fixture verifies retry without another edit. | At most 1,000 history URLs / 256 bookmarks, with earlier history eviction to fit storage. Not a full history database, folder/import/export/sync system. Session and library are independently atomic, not one cross-file transaction. |
| Downloads | Real attachment bytes; pending-destination cancellation/failure and late-callback rejection; bounded normal-profile terminal metadata with private no-records; native clear Cancel/Confirm and persisted deletion without file deletion; pre-response failure, vanished-parent write failure, identity-length truncation and missing-file reveal rejection. Saved UI follows retention; corrupt/future download records remain unchanged while browsing continues with visibly memory-only download records. | Successful external file-manager/portal integration is not exercised by automated tests. Disk-full/permission-denied matrix, active-transfer crash recovery and resume remain. Unknown/encoded lengths depend on engine behavior. |
| Memory ownership | Background PSS/RSS includes Linux-visible owned process descendants; labels partial coverage. | PID namespaces/reparenting/cgroups/churn and multiple profile runtime aggregation remain unproven. |
| Developer tools | Native active-tab Inspector opens; extras enabled; F12/menu/toolbars wired. | DOM edits, console/debugger/docking/leak stress matrix; no CDP/native Rust debugging claim. |
| Interactive MCP | Real trusted Unicode/emoji, keys/form submit, selector/coordinate clicks, wheel, PNG, scope/staleness and cleanup pass against owned HTTP/HTTPS fixtures. Canonical single HTTPS origin, pinned public peers, mandatory isolation, native origin filter, ten-minute workspace grant and native single-use navigation/input consent implemented. Actual GTK buttons verify Grant/Deny/Cancel/Allow/Stop and target disclosure; regressions cover changed targets/labels, idle control, EOF and bounded forwarding/queueing. | Wayland/single-origin scope, no live-site/personal-profile tests. Consent callbacks and backend guards are separately exercised, not a full public-origin end-to-end human/race matrix. Two independent clients, complete frames/dialogs/focus/layout/long-input/physical-preemption coverage and aggregate resource budgets remain. Sampled checks/input are not atomic or a semantic-safety guarantee. See [MCP details](MCP.md). |
| Release performance | Monitoring/persistence work kept off GTK, lazy restored tabs, bounded state queues. | No measured release startup/latency/CPU/memory/capture/blocking baseline or accepted budgets yet. |

## Permission-origin blocker: no unsafe top-frame shortcut

The native `WebView::permission-request` signal supplies a WebView and a generic
PermissionRequest. The generic request API exposes allow/deny, not an authenticated
requesting-frame origin. The inspected geolocation, notification, clipboard,
device-info and user-media request bindings do not expose a requesting-origin
accessor. Some other specialized APIs, such as XR on newer feature levels, do
expose origin information; this does not establish an attribution mechanism for
the missing request types.

A WebView's current URI identifies its top-level page, **not necessarily the frame
requesting access**. A page-provided script/message or an unrelated permissions
query is not trusted evidence tying a later request to a principal. Paisley must
not save or auto-apply a top-level-origin grant to a potentially different frame.

Until that association is established, normal browsing explicitly denies all
permission-request signals and shows why. Automation also denies them. There is
no misleading Always Allow prompt, persistent grant or revocation promise. Camera,
microphone, geolocation and other sensitive workflows may therefore not work.

The current engine choice remains WebKitGTK. Completing general per-origin grants
requires verified public engine support, a reviewed upstream/binding change, or
an explicit product/engine decision. Do not change engines, install a patched host
library, use private object-layout tricks, weaken frame/origin checks, or silently
grant an unknown principal to make a demo pass. The remaining work is named rather
than marked complete.

References:

- [WebView permission-request](https://webkitgtk.org/reference/webkitgtk/stable/signal.WebView.permission-request.html)
- [PermissionRequest interface](https://webkitgtk.org/reference/webkitgtk/stable/iface.PermissionRequest.html)
- [UserMediaPermissionRequest](https://webkitgtk.org/reference/webkitgtk/stable/class.UserMediaPermissionRequest.html)
- [Interactive MCP boundaries and tests](MCP.md)
- [Full product roadmap](../ROADMAP.md)
