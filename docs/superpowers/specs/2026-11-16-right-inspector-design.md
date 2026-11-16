# Right-side Inspector

The user requests Inspect Element on the right by default. WebKitGTK's native
right-dock button applies to the current inspector; a new tab was observed to
open at the bottom again. Host the native frontend in a horizontal GTK Paned
per tab via the public attach/detach/closed signals. Weak widget references
avoid ownership cycles. Keep native detached windows. Initial tools width is
about 40%, with a movable divider; closing removes the frontend from the pane.
No private inspector JavaScript or engine preference mutation is used.
