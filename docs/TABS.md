# Tabs

- Middle-click a tab's title or icon to close that tab, including background tabs.
- The close button and Ctrl+W retain their existing behavior. Closing the last
  tab opens the home page.
- Site favicons appear at 16 px beside titles. A same-size spinner occupies that
  slot while loading; a globe is used when no site icon is available.
- Icons are fetched by WebKit's enabled favicon database using the existing
  profile network session. Private windows retain their ephemeral session.
- Tab callbacks hold weak browser references and close by stable tab ID so
  reordering does not change which tab is closed.

API reference: [WebKitGTK 6 migration guide](https://github.com/WebKit/WebKit/blob/main/Source/WebKit/gtk/migrating-to-webkitgtk-6.0.md#favicon-database).

Manual Bot Screen checks: YouTube and DuckDuckGo favicons appeared beside tab
titles; the spinner appeared during navigation. Middle-click closed an active
tab by its title, a background tab without changing the active tab, and a tab
by its favicon. The original YouTube tab was preserved. Locked build and
whitespace check passed. No automated tests were run.
