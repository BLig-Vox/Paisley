# Period input and inspector stability: alpha.4

## Root causes and fixes

Alpha.3 native Bot Screen keystrokes `a`, period, `b` yielded `ab`.
The DOM saw key=Delete, code=Period, keyCode=46: the runner used the ASCII
character value where Chromium expects a virtual key. Period now uses OEM_PERIOD
190; named Delete remains 46. Punctuation and shifted digits have explicit
mappings, ASCII letters/digits keep their keys, and unknown/Unicode keys use VK0
with the existing separate UTF-16 text events. Values follow Chromium's
[POSIX virtual-key definitions](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/ui/events/keycodes/keyboard_codes_posix.h).

The inspector relay disconnected and destroyed the pane after only 64 queued
messages. Clicking a fixture button producing 2000 console logs reproduced the
closure; startup also sends bursts of small commands. Both FIFO queues now allow
4096 messages while retaining the existing 4 MiB payload cap per queue. Empty
messages remain bounded by the entry ceiling (about 96 KiB of Vec metadata).
Observers, IPC ownership, shutdown, authentication and production sandbox policy
are unchanged. Finite byte/message limits can still reject extreme traffic.

## Verification

- Keyboard regression was red at period 46 versus expected190, then green.
- Queue burst regression was red at message64, then green. Byte overflow,
  reclamation, FIFO and empty-message flooding remain covered.
- 22 adapter Rust unit tests and 67 root Rust tests passed (one display test
  ignored); 47 Python regressions passed. Extracted adapter package also passed
  its 22 unit tests using the registry transport dependency.
- Focused CEF native suite: period inserted in text/email/textarea; actual Delete
  removed the next character and Backspace removed the period.
- Inspector native suite: live DOM access, 2000-message burst, 15-second settle,
  explicit close, loopback-listener cleanup and original page survival passed.
- Full CEF native suite: 60 tab cycles, form retention, cookie sharing/private
  isolation, authenticated transfers, pause/resume, closed source tab,
  PNG/ZIP/PDF/TXT, atomic publication, site-data clearing and intentional engine
  crash recovery passed. Peak sampled owned-process PSS: 1130.34 MiB.
- Manual Bot Screen native keys produced `a.b` in all three fields with period
  keydown190. The right-hand Console stayed open after2000logs; typing and
  evaluating `1.2` returned1.2 while the pane remained visible.
- All three supervised native runs exited0 with no tracked owned processes and
  no supervisor-classified runtime criticals. Tests used the previously approved
  temporary debug/test-support root sandbox exception only. Secure non-root
  deployment is outside this environment's verified scope.
- Independent read-only code review found no critical/important issues. Its minor
  request for shifted-digit/Unicode mapping assertions was included.

All compilations used hard decimal3GB, zero build swap and one Cargo job.
The largest cached compile peak was1791.58MiB; no build reported OOM events.
These figures do not impose a runtime RAM ceiling or prove zero leaks.
See [machine-readable evidence](period-inspector-evidence.json).
