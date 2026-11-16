# Memory counter

Paisley samples its Linux-visible owned process tree on a background thread about
once per second. PSS is preferred; each process whose PSS cannot be read falls
back to RSS. It does not measure total system RAM or promise complete coverage of
sandboxed or reparented processes.

The GTK timer checks a single replaceable result slot every 250 ms. Old samples
cannot queue behind a busy UI. Status updates do not refresh navigation controls.
The display uses decimal MiB, marks samples older than three seconds as stale,
and replaces failed samples with “memory unavailable”. Hovering the status bar
shows the age of the last sample. An unchanged number can still be a fresh sample.
GTK stalls and slow /proc reads can delay display updates: this is periodic
sampling, not a hard real-time monitor.

Closing a window releases its result slot; the worker holds only a Weak reference
between passes and exits once the slot has gone. Samples are never overlapped.

## Manual check — Bot Screen, 2026-11-16

Release build succeeded. A fresh one-tab profile loaded an owned loopback fixture.
The counter showed 457.2 MiB before a 48 MiB typed-array allocation and 510.1 MiB
after it. The release button cleared the page's reference; the counter continued
sampling rather than assuming the engine must return those pages immediately.
WebKit may retain heap or cached pages after JavaScript releases a reference.

No automated tests were added or run. Stale/unavailable paths were reviewed in
source, not induced through process failures. This change is newer than crates.io
0.1.1 until the next published release.
