# Favicons in History and Bookmarks

Each profile owns a favicon service shared by History and Bookmarks. Lookups try the exact page URL, up to six known page URLs from the same origin, the origin root in WebKit's favicon database, and the persistent site cache. The service does not make network requests: visiting a site remains WebKit's responsibility. Sites that provide no usable icon show a consistently sized domain initial.

The site key is the complete HTTP(S) origin (scheme, hostname and port); unrelated subdomains and different ports do not share icons. URLs containing credentials are excluded. WebKit favicon-change events update mounted rows automatically. Successful page loads retry cache lookup, and failed lookups are otherwise throttled for 60 seconds. A change arriving during a lookup schedules another lookup instead of being lost. Reopening a sheet also retries expired misses.

## Persistence and bounds

Normal profiles store icons under `<profile cache>/site-icons-v1`. Private windows have an independent in-memory service and never read or write this directory. The PNG filenames are SHA-256 hashes of the origin, rather than full URLs or page titles. This is a cache, not encrypted browsing metadata; clearing the profile's cache directory removes it.

The service retains at most 256 site icons and 256 cached files. Incoming textures must be at most 512×512 pixels and are reduced proportionally to at most 64×64. PNGs are limited to 64 KiB; cached input is checked for size, PNG header and dimensions before decoding. Cache files use atomic replacement and mode 0600 in a 0700 directory. Reads reject symlinks and nonregular files. Cache I/O runs in background jobs; at most eight lookups and one write run concurrently, with bounded lookup and coalesced write queues. Failures keep the existing icon or initial.

Mounted rows are weakly referenced; closing or rebuilding a sheet does not keep old row widgets alive. Source icons are presented through GTK's icon interface at 18 px, inside a consistent 28 px slot.

## Review

Locked build and whitespace check passed. Native Bot Screen checks confirmed that older Google and YouTube History entries now reuse their site icon. An isolated local site delayed its favicon by eight seconds: both an icon-bearing page and an older page with no icon updated live while History stayed open. The isolated profile was closed, its WebKit data/cache moved aside, and its local server stopped. On restart, both rows still displayed the icon using Paisley's disk cache. Cache files were confirmed mode 0600. This verifies a controlled persistent-cache scenario, not availability of an icon from every website.

No automated tests were added or run. The native GTK UI is inspected with Bot Screen rather than Playwright.
