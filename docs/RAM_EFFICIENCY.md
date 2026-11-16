# Memory-saving policy

Paisley retains WebKitGTK. The separate Servo preview did not establish reliable
RAM savings or a working GitHub social-login flow, and is not part of this change.

## Engine policy

- All native browsing contexts use WebKit's DocumentBrowser cache model instead
  of the default WebBrowser model. Related views inherit their source context.
- Full Back/Forward page snapshots are disabled on native browsing WebViews.
  History URLs and Back/Forward buttons remain; revisiting pages may reload,
  losing transient JavaScript state from the page that was left. Live tabs are
  not automatically discarded, and editing a form in a live tab is unaffected.
- Native WebKit memory-pressure reclamation checks every ten seconds with a
  512 MiB per-process policy reference: conservative cleanup at 128 MiB, strict
  cleanup at 256 MiB. The fixed-threshold kill policy remains disabled.
- The network policy is configured during GApplication startup, before sessions
  are created; each web context receives the same policy at construction.

The 512 MiB reference is not a whole-browser cap or guaranteed usage. Active
JavaScript, decoded media/images, GTK rendering, engine allocation caches, and
multiple tabs can still exceed it. More frequent collection and history reloads
can cost CPU or loading time. Cookie/storage data, downloads and notification
permissions are not cleared by this policy. No scripts disable site features.

## Manual observations on Bot Screen — 2026-11-16

Release build succeeded. Fresh comparison profiles loaded five owned loopback
pages in one tab, each allocating a 32 MiB typed array. PSS was sampled for the
visible process group. The prior build settled near 635 MiB; the revised cache
and reclamation configurations also reached roughly 635–637 MiB. This short
allocation-heavy check does not demonstrate a measurable RAM reduction and must
not be advertised as a percentage saving. Allocators can retain released pages.

Native links and history navigation worked. A custom unsaved input value
survived creating another tab and switching back. No automated tests were added
or run. Broad website, video, long-session, and CPU-impact measurements remain
unmeasured. Existing VNC DRI3/a11y and startup GTK image warnings remain.

References consulted for the installed runtime version:
- https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebKit/UIProcess/API/glib/WebKitWebContext.cpp
- https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebKit/UIProcess/API/glib/WebKitMemoryPressureSettings.cpp
- https://github.com/WebKit/WebKit/blob/webkitgtk-2.52.6/Source/WebKit/UIProcess/API/glib/WebKitSettings.cpp

This policy is newer than the published crates.io 0.1.1 version.
