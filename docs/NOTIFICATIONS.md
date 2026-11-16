# Notifications

The bell opens Notifications in the native right sidebar. Alerts have an unread
badge, six-second dismissible banner and a window-local recent list. Clear all
or dismiss individual events. No notification contents are written to disk.
History retains at most 50 events. Website arrivals are throttled to one per
second per window; repeated identical Paisley notices are coalesced.

## Website alerts

In a normal profile, visit the website and open the bell. **Allow & open in new
tab** saves permission for that exact HTTPS origin (scheme, host and port) and
opens its URL in a fresh context. The original tab is unchanged. HTTP is accepted
only for loopback development origins. Allowed origins are capped at 128 and
saved in the profile's existing atomic settings record; preference exports omit
them. Different profiles have independent contexts and permissions.

This uses native WebKit notification permission initialization, not injected
JavaScript or a top-level URL substituted for an unknown requesting frame.
Unknown generic requests remain denied; the native permission callback does not
expose a trustworthy requesting origin. The user explicitly selects an origin in
Paisley's own UI instead. Related views inherit their originating context.

**Block** stops delivery from any older context whose grants exceed the current
list. Reopen tabs to refresh the page's permission state; it may continue to say
`granted` until reopened. Other allowed sites sharing that older context must
also reopen their tabs. Existing notifications may disappear when their website
closes or replaces them. Website titles and bodies are untrusted plain text;
WebKit's notification object exposes no authenticated sender origin, so the UI
labels these as Website notification and links back to their tab.

WebKit suppresses website notifications in private browsing. Paisley's own
notifications and downloads remain available there, with memory-only history.
There is no service-worker Web Push registration or delivery while Paisley is
closed. Website notifications and download events are delivered inside Paisley.

## Linux desktop notifications

Normal profiles can also deliver alerts through the session’s freedesktop
notification service while Paisley is in the background. The Notifications sheet
has a persistent Desktop notifications toggle and a preview button. Private
windows never send desktop alerts. A missing daemon or session bus keeps in-app
notifications usable. OS display and retention follow the desktop’s settings and
Do Not Disturb. A desktop click opens the source tab/downloads or Notifications,
when the daemon supports actions. Clearing/dismissing in-app events also withdraws
their corresponding desktop alerts. Up to four desktop calls may be in flight,
with at most 50 retained action mappings, two-second call deadlines and escaped
notification body text. Paisley must remain running for actions and delivery.

Bot Screen initially lacked a notification service. Installed `xfce4-notifyd` and
`libnotify-bin` there; the daemon reports Xfce Notify Daemon 0.9.4 / protocol 1.2.
A real Paisley preview and a background website alert appeared as desktop
notifications; the session-bus trace confirmed its native Notify request. A desktop preview
click returned to Paisley; ActionInvoked and close signals were observed. No daemon is bundled
in the Cargo package.


## Manual observations on Bot Screen

2026-11-16, Ubuntu 24.04 / WebKitGTK 2.52.6:

- Native sidebar, empty/recent states, banner, explicit allow and per-site Block.
- A loopback page received `denied` before consent and `granted` in the fresh
  allowed normal-profile tab. A real `new Notification()` produced a banner and
  recent card containing Chinese, Japanese and emoji.
- Open tab focused the source tab and the page received its notification click
  event. Block closed subsequent notifications instead of displaying them.
- A downloaded text attachment completed and produced an actionable event.
- Private WebKit did not grant or deliver website notifications; the UI explains
  this restriction rather than claiming that an in-memory allow will work.

Release build and source whitespace checks were run. No automated tests were
added or run for this change. These checks do not establish broad live-site,
Web Push, multi-user OS notification or memory-stress compatibility.

References: [WebKit permission initialization](https://webkitgtk.org/reference/webkit2gtk/2.41.3/method.WebContext.initialize_notification_permissions.html),
[upstream WebContext API](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/UIProcess/API/glib/WebKitWebContext.h.in).
