# Profiles panel

Profiles opens in the secondary sheet beside the main menu. The panel emphasizes
saved display names, a small initial avatar, and the Current badge. The current
profile sorts first, followed by display name. Search filters names and IDs.

Create profile reveals an inline form. The ID uses the existing 1–64 character
ASCII validation; an already listed ID is rejected with a prompt to open it from
the list. Enter submits. Cancel clears the draft and creation message. Creating
and opening a new profile retains the existing isolated-window launch behavior.
Private window is a separate action; private mode cannot manage saved profiles.

Each profile's options expand inline. Rename changes the display name, and delete
retains its explicit confirmation and backend locking. Both controls are disabled
for the current profile. Other running profiles are protected by the existing
catalog lock checks. The Open action activates another profile's window.

Manual Bot Screen checks: normal list, Current marker, expandable options with
current-profile controls disabled, search/no results, and duplicate ID validation.
No profiles were created, renamed, or deleted during this review. Locked build
and whitespace check passed; no automated tests were run.

Final installed-build check: Enter submitted an invalid ID and displayed a clear
validation message. Cancel then removed both the form and error and returned to
the clean list. No profile storage operation was performed.
