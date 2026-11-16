# CEF migration and bounded tab residency

The user redirected the unfinished Servo migration to CEF after standalone Servo failed to load YouTube. Preserve the GTK interface and profile separation. Use cef-rs 156.6.0 with its matching CEF runtime. CEF compatibility must be demonstrated; it is not evidence of lower memory by itself.

## Decisions

- Reuse the bounded GTK transport and profile process registry from the unfinished adapter. Add a CEF runner, with no Servo linked in a CEF build. Keep the installed WebKit release until the replacement passes native tests.
- One CEF browser process and request context per profile, shared among that profile's tabs. Preserve Chromium's renderer isolation; no single-process or web-security-disabling switches.
- Create renderer pages only when needed. Hide background pages through the native API. Close pages through CEF's asynchronous close lifecycle and drop references after OnBeforeClose.
- Track resident, unloaded, closing, and discarded states. Discard only inactive pages for which the engine confirms no unsaved form work, audio/video, capture, loading, or downloads. Unknown protection state is protected. Restore the URL when selected; never promise preservation of discarded in-memory state.
- Start with a target of three eligible resident tabs, not a hard limit on protected pages. Under pressure, reclaim least recently used eligible pages. A 3 GiB app pressure target and 2 GiB system available-memory floor trigger reclamation; they do not promise an absolute browser allocation cap.
- Report browser process-tree PSS, and mark RSS fallback/partial coverage. Compare the same fixtures before and after tab closure, including delayed cleanup. No unmeasured percentage savings claims.
- Every build must run through scripts/build-limited.py, with a kernel-enforced 2 GiB RAM limit (optional 1 GiB), zero build swap, one job, and a 2 GiB system available-memory stop threshold. A failure must not silently increase the cap.

## Validation

Verify real page rendering, input, profile cookie sharing/isolation, YouTube playback, tab close/reopen cycles, forms/audio protection, native dialog cleanup, download ownership, and shutdown. Measure whole-tree PSS for one tab, eight tabs, and repeated return to one tab. Test fresh and cached builds within the enforced build limit. Packaging must include the matching runtime and licenses. Do not promote a partially working backend.
