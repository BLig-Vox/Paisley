# Browsing sheets

The user approved matching secondary sheets beside the main menu for Site protection, Bookmark this page and Bookmarks. This builds on the existing GTK menu and profile library; no new persistence format or icon service is introduced.

## Site protection

The sheet identifies the site it controls, shows a labeled ads/trackers switch and plain-language status. The switch is disabled for internal pages, when profile-wide blocking is off, or when the filter is unavailable. Protection settings leads to the existing Settings panel. Saving uses the current profile settings and updates loaded filters; existing content needs a reload. The panel describes starter rules without claiming complete protection or fabricated blocked counts. A panel opened on a site continues to identify that site if the browser navigates elsewhere.

## Bookmark this page

Ctrl+D and the menu action open a form with a title, full selectable URL, Save and, when already saved, Remove. Enter submits the form. Existing titles are loaded from the library. Validation and profile/private storage semantics come from the existing Library implementation. Invalid/internal URLs cannot be saved. Success and failure messages stay in the sheet.

## Bookmarks

Search remains above the list. Each row shows a cached site icon or a consistent domain initial, its title and domain, with the full URL in the tooltip. The profile-local favicon service reuses same-site icons, persists a bounded cache for normal profiles and updates rows live; see FAVICONS.md. It makes no independent network requests. The ellipsis button reveals an inline editor with Save/Remove. Rows rebuild after a change so title and search results reflect the current library. Import/export retains existing file chooser and confirmation behavior.

## Review

Locked build and whitespace check passed. Installed and inspected the native GTK UI on Bot Screen at 1280×800 and maximized at 1440×900. Checked site protection off/on (restored its original state), existing bookmark presentation and cached YouTube favicon, Ctrl+D, Enter to save a new private bookmark, inline title editing, search/no-results, removal and empty state. Bookmark writes were exercised only in an isolated private preview window; the saved default-profile library was left unchanged. No automated tests were added or run. Native GTK was inspected through Bot Screen rather than Playwright, which does not control these GTK widgets.
