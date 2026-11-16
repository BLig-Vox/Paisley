# Managed Downloads Implementation Plan

**Goal:** Ship durable direct downloads in Paisley's sheet, plus safer native completion.
**Spec:** ../specs/2026-11-16-managed-downloads-design.md
**Architecture:** A UI-independent Rust transfer engine and bounded on-disk job
registry, with a GTK controller owning worker lifetimes and rendering status.

- [x] Implement private atomic state, job and manifest validation, locks and checkpoint hashing.
- [x] Implement identity HTTP, strict ranges/validators, bounded segmented workers,
  retry/backoff, cancellation, throttling, streamed verification and publication.
- [x] Add URL/destination/checksum form, queue, pause/resume, speed and recovery UI.
- [x] Stage native WebKit transfers, preserving authenticated browser behavior.
- [x] Build, review lifetime/error/partial-file paths, install and inspect in Bot Screen.

Constraints: preserve existing uncommitted work; no automated tests without a
request; never label a file checksum-verified without matching a supplied digest.

## Evidence

Source build passed. Independent source review found a completion transaction gap;
a durable receipt and shutdown result collection fixed it, and the follow-up
review accepted that correction. A direct download of the public Ripple logo
completed through the GTK form and displayed Verified after matching its supplied
SHA-256. This was manual UI inspection; no automated tests were added or run.
Segmented/resume failure scenarios are not claimed to be runtime-verified by this
single small-file transfer. Browser session transfers remain native and now stage
partials with no-overwrite publication. Managed queue, speed, recovery and partial
metadata behavior are documented in docs/MANAGED_DOWNLOADS.md.

Manual reopen caught GTK effective visibility being used as a persistence filter.
Replaced it with an explicit removed flag in persistence, scheduling and counts.
The session's one verification job was recovered from its durable receipt after
that earlier build dropped its row. Resume reconciled the existing completed
file and displayed Verified. A further close/reopen preserved that completed job.
Completed row timestamps and sizes persist; older entries can recover their size
from matching receipt metadata. No user download records other than the newly
created UI verification job were modified by this recovery check.
