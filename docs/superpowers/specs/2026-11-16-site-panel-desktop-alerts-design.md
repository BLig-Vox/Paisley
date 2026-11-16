# Desktop alerts and site information

After finishing in-app alerts, add background delivery through the session's
org.freedesktop.Notifications service. Use bounded asynchronous calls, daemon
capabilities, escaped body markup, click actions and explicit close handling.
Send ordinary alerts to the OS only while Paisley is inactive. Provide a normal-
profile toggle and preview button; private windows do not send desktop alerts.
Unavailable service or Do Not Disturb does not break in-app delivery. Paisley
must remain running; do not promise background Web Push.

Add a shield button at the left of the address entry. Its native popover shows
the actual committed origin, WebKit TLS certificate validation status, a concise
starter-protection card and per-site toggle, clear-site-data action, and links
to notification/privacy settings. Use Paisley's existing Lucide icons and quiet
native styling, not Firefox branding or unsupported security claims. Close the
popover on navigation/tab switch to prevent stale-site actions.

Website data clearing uses WebKit's returned domain records. Show the exact
record names before destructive confirmation; do not guess a registrable domain
or silently clear unrelated records. Warn that the site may sign out and other
open tabs can recreate data. Keep per-site cookie/storage removal separate from
browser history/bookmarks/downloads. Do not bypass certificate errors.
