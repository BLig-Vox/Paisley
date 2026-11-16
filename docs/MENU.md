# Application menu and secondary sheets

The hamburger button opens a simple right sidebar. Selecting Settings, Profiles,
Site protection, Bookmark this page, Bookmarks, or History opens a second sheet immediately to its left, keeping the
main menu visible on the far right. This follows the user's choice of a second
panel beside the menu, rather than replacing the menu with a Back page.

The main menu is about 300 px wide; secondary sections are about 420 px. Downloads
uses its existing compact sheet beside the menu. Settings, profile management,
and library content are now embedded sections, rather than floating windows.
Developer tools retain their native inspector dock. Site protection and Bookmark this page now open explicit controls in their own secondary sheet. Ctrl+D opens the bookmark editor before saving.

Visual treatment: consistent icons and row spacing, restrained section labels,
quiet separators, a selected-section highlight, flatter library entries, smaller
URL text, and aligned settings switches. Details and import/export/reset controls
sit under Advanced; the save button remains visible at the bottom. Existing
profile and library confirmations are preserved. Empty messages take no space.

Close the secondary section independently with its close button or Escape. Escape
then closes the main menu. Selecting another section replaces the secondary
content. Closing Settings without saving discards the draft. Saving applies the
existing validation and persistence logic, then closes only the secondary section.
The settings editor is retained only for its mounted page; library operations use
weak panel handles and check the panel is still mounted before completing.

Manual Bot Screen review at the normal 1280×800 browser window: Settings switches
and unchanged Save, Profiles layout, Bookmarks, History rows/search/no-results,
Downloads beside the menu, reopening sections, and Escape. Native GTK inspection
used Bot Screen rather than Playwright. Locked build and whitespace check passed;
no automated tests were run. Import/export and profile creation/deletion were not
performed during this visual pass.

Final interaction checks: the bookmark Import file chooser opened with the
browser as its parent and was cancelled without importing. Escape closed the
secondary pane while leaving the menu visible, then closed the main menu.
Settings was reopened for the final visible state. These checks do not exercise
an actual import/export, profile creation, or deletion.

Also inspected the two-pane Settings layout maximized at 1440×900, then restored
the normal window size. No clipping was observed in panel controls at either size.
