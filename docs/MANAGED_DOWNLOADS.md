# Managed downloads

Open Downloads (Ctrl+J), choose **New download…**, paste a public direct HTTP(S)
URL, optionally supply a publisher SHA-256, select 1–4 connections and a speed
limit, then choose a new destination. Zero means unlimited; nonzero limits start
at 16 KiB/s. Two jobs run at once; further jobs wait in the queue.

## Completion and recovery

- Bytes stay in a private, destination-adjacent `.paisley-<id>.part` directory.
  The final filename is linked into place only after completion and read-back
  verification, without replacing an existing file.
- Files of at least 4 MiB can use multiple connections when exact byte ranges
  and a strong ETag are available. Every response range, validator, final URL
  and total must match. Single-connection fallback starts again from zero on
  resume when the server cannot validate ranges.
- Segments have synced SHA-256 checkpoints. Resume rechecks the server identity
  and checkpointed local bytes before continuing. Uncheckpointed tails are
  discarded. Changes or corruption cause an explicit refusal, not mixed bytes.
- A supplied checksum must match before publication. Without one, completion
  establishes transfer consistency, not publisher authenticity. SHA-256 is
  recorded locally and the assembled file is read back before publication.
- Pause preserves partials. Interrupted jobs restore paused, with no automatic
  network activity on startup. Resume explicitly requeues a job.
- A small completion receipt remains beside the file in the staging directory
  after payload cleanup. It recovers completion if a crash or registry-write
  failure occurs between publication and UI persistence.
- Remove job removes its registry entry; partial directories and completed files
  are kept. Keep the job if you want to resume it. The session holds at most
  100 job rows; reopen Paisley after removing old rows to reclaim those slots.

## Browser sessions

The direct manager does not copy browser cookies, passwords, POST bodies or
browser authentication grants. Authenticated and ordinary browser-initiated
requests continue through WebKit. Those transfers now write into a private
`.paisley-native-…part` directory and publish only after the native completion
and known identity-length checks. They retain native cancel/retry behavior;
managed pause/resume, segmentation and publisher checksum entry apply to the
**New download…** route. Managed jobs are unavailable in private windows to avoid
writing URL-bearing recovery metadata there. Explicitly saved private files and
partial payloads can remain on disk.

Managed job URLs and paths are saved unencrypted in the profile's private
`managed-downloads.json`. Signed URLs can contain secrets and may expire.
A changed redirect URL or validator can prevent resume. TLS checks remain enabled;
redirects are bounded and HTTPS-to-HTTP downgrades are rejected.

## Evidence and limits

The implementation was built and source-reviewed, including completion/crash
reconciliation, worker shutdown, bounded buffers and checkpoint ordering.
A small live direct transfer matched the supplied SHA-256, receipt reconciliation
restored that finished file, and a subsequent close/reopen preserved the job.
No automated tests were added or run in this session. UI inspection cannot prove
absence of corruption or memory leaks. Server and filesystem failures, hostile
same-user edits and storage faults cannot be eliminated by a downloader.

### Simpler Downloads interface

The Downloads sheet uses compact file rows without table headings or a date
column. Each row keeps its size, status, and primary action visible. Managed
transfers expose their date, speed limit, and removal action in an options menu.
The new-download form shows the URL first; checksum, connections, and speed
settings are under a collapsed Advanced section. Clearing history keeps files.

Manually inspected on Bot Screen at the normal desktop window size: completed
verified row, row options menu, and both states of Advanced. This is native GTK
UI, so visual inspection used Bot Screen rather than Playwright. The locked
build and whitespace check passed; no automated tests were run.

Pending and paused rows share the compact layout. Pending transfers can be
paused before a worker starts. Paused rows show Resume and hide the inactive
progress bar. Connection, preparation, retry and finishing messages use short
labels; technical transfer errors remain in tooltips. A throttled live download
and its transition to Paused were manually observed on Bot Screen.

### Received size and retained progress

Following user feedback, paused and failed transfers keep their progress bars.
Managed rows display received / total bytes using B, KiB, MiB, or GiB; unknown
sizes show received bytes without inventing a total. Received and total snapshots
are retained in the registry on pause, failure, and clean shutdown. Partial range
bytes written before a pause are synced and checkpointed for resume. Failed
transfers may display received bytes that need to be fetched again on retry;
resume still validates and truncates uncheckpointed data. Native WebKit failures
also retain their last progress display.

The preview transfer was explicitly limited to 16 KiB/s for manual pause review;
new downloads default to unlimited (0). Source builds and whitespace checks pass.
Installation, removing the preview limit, and visual review of this refinement
are pending Return to agent because Bot Screen is currently in human control.

Observed the old installed UI advancing from 335 KiB to 697 KiB while its
half-second speed sample repeatedly showed 0 B/s. The display now samples over
three seconds and shows a preparation label before its first speed sample,
reducing zero flashes between rate-limited chunks. Locked build succeeds;
Bot Screen installation and visual verification remain pending human takeover.

Installed the patch through Bot Screen after control returned to agent. Cleared
the old preview's 16 KiB/s limit. A new short live transfer displayed received /
total and an averaged rate, then paused at 128 KiB / 1.2 MiB with its progress bar
still visible. Set its limit to 0 in the row options and resumed; it completed at
1.2 MiB / 1.2 MiB. Both preview limits are now unlimited. This confirms live
pause/resume; failure rendering changes were inspected in source, not exercised
with an induced network failure. No automated tests were run.

### Right sidebar sheet

Downloads now docks to the right of the webpage in a roughly 400 px sidebar.
The page stays visible and interactive beside it. File rows stack their metadata
and actions to fit the sidebar. Ctrl+J toggles it; Escape or the close button
returns the space to the page. Navigating or creating a tab keeps the sidebar
open. This supersedes the earlier full-page sheet layout.

After the desktop crash/reconnect, reinstalled and reopened Paisley. Manually
confirmed the right sidebar layout at 1280×800, Escape restoring page width,
Ctrl+J reopening the sheet, and creating/closing a tab keeping Downloads open.
Locked build and whitespace check passed. No automated tests were run.

### File-type download checks (2026-11-16)

Downloaded five public files using the actual New download / Save to workflow on
Bot Screen with the default unlimited speed setting:

| Saved file (Bot Screen `/home/bot`) | Bytes | Source |
| --- | ---: | --- |
| sample.pdf | 13,264 | W3C dummy PDF |
| ripple-readme.txt | 8,257 | BLig-Vox/Ripple README |
| ripple-source.zip | 1,106,175 | GitHub codeload archive |
| sample-audio.mp3 | 39,868 | MDN CC0 T-Rex audio |
| sample-video.mp4 | 1,128,375 | MDN CC0 flower video |

All completed and displayed received/total sizes. Scrolled the right sidebar to
inspect document, archive, audio, and video icons. A one-off script run in the
Bot Screen terminal verified each file's size and SHA-256 against an independently
fetched copy, PDF/MP3/MP4 signatures, UTF-8 text, and ZIP member CRCs. All passed.
This checks download integrity and presentation, not audio/video playback. The
first prospective media host returned HTTP 403 during source selection; MDN
public examples were used instead.
