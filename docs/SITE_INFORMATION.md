# Site information and protection

The shield beside the address opens a native site popover with the committed
HTTP(S) origin, WebKit connection details, starter-blocking status, a per-site
toggle, confirmed cookie/storage clearing and links to Notifications and Settings.
It closes on navigation and tab changes so actions cannot silently follow an old
site selection. Internal, uncommitted and failed pages do not get website controls.

Connection secure requires WebKit to supply TLS information with no certificate
errors. HTTP says Connection not encrypted. Mixed content detected by WebKit
changes the shield and connection label. The certificate expander shows bounded
subject/issuer text. This describes transport security, not site trustworthiness;
there is no certificate bypass or full tracking-protection claim.

Per-site blocking uses the existing profile settings and compiled starter filter.
The toggle is unavailable if global blocking is off or its filter is not ready.
Changing it updates the same state as the protection sidebar; reload the page to
apply to existing content. Closing the popover on a successful change avoids a
stale protection card. Private changes stay temporary.

## Clear site data

Paisley asynchronously fetches WebKit website-data records and selects exact or
parent-domain records named by the engine, with a dot boundary. It does not invent
a registrable-domain match. A confirmation lists the actual record names and warns
that related subdomains can share the records, the site may sign out, and other
open tabs can recreate data. Cancel makes no changes. Only one clear flow runs at
a time in the window, and more than 32 matching records is rejected.

The operation removes cookies, local/session storage, IndexedDB, DOM cache,
service-worker registrations and memory/disk caches from the selected records in
this profile. History, bookmarks, downloaded files, notification permissions,
HSTS and tracking-prevention records are not cleared by this action. No matching
records produces a notification instead of clearing the entire profile.

## Manual observations

On Bot Screen, a local HTTP page showed Connection not encrypted. Its cookie and
local-storage marker were saved, the confirmation listed `127.0.0.1`, and both
were empty after confirmed clearing and page reload. DuckDuckGo HTTPS showed
Connection secure with WebKit-provided subject and issuer in the expander. The
popover was inspected at normal and maximized window sizes; no automated tests
were added or run. Mixed-content handling is implemented from the engine signal
but was not exercised with a mixed-content fixture.
