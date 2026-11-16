# Paisley notifications

Implement the user's requested usable in-app notifications in native GTK. A bell with unread count opens the existing menu's secondary sheet. Show brief dismissible banners without taking focus. Keep at most 50 events, truncate untrusted plain text, support clear/dismiss, and retain no event history on disk. Download completion/failure and page-process failures produce events. Website notifications use WebKit's real Notification objects and forward click/close lifecycle.

## Approaches and decision
- In-app-only events are simple but do not serve websites.
- Granting a generic native permission request using its WebView URI misattributes embedded origins and is rejected.
- Chosen implementation: explicit native opt-in for the selected exact HTTPS (or loopback HTTP) origin, initialized into a fresh per-tab WebContext. An Allow and reopen action opens a fresh tab; existing tabs keep their state. Related views inherit their context. Existing generic permission requests remain denied with helpful notification guidance. Existing context delivery is blocked if any previously granted origin has been revoked. No page-injected permission shim, patched engine or independent push service.

Normal profiles persist a bounded list of canonical origins in existing atomic Settings storage. WebKit suppresses website notifications in private profiles; expose only in-app alerts there. Profile exports omit grants. Revocation stops delivery immediately; pages may retain stale permission state until reopened. WebKit Notification exposes no authenticated origin, so events say Website notification and link to the originating tab, without claiming a verified sending domain. Notifications require Paisley to remain running; background Web Push/service-worker registration is outside this implementation.

Avoid ownership cycles: callbacks hold weak owners; timers are cancelled/replaced; at most 50 retained notifications and bounded rate of incoming events. Plain text never executes markup or opens downloaded files. Keep the existing sheet and Lucide visual language.
