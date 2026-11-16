# Paisley Servo migration implementation plan

**Goal:** Replace the normal browsing backend with Servo and verify bounded memory after repeated tab use.
**Authority:** User explicitly requests a real integration, testing and optimization; further design questions waived by proceeding instruction.
**Architecture:** Preserve GTK navigation, tabs, profiles and sheets. Adapt engine-specific navigation, events, permissions, storage and downloads. Keep the WebKit automation harness separate until migrated. Never expose unsupported engine functionality as working.

## Tasks
- [ ] Establish that pinned servo-gtk and Servo dependencies build on the available toolchain with one Cargo job, no debug symbols and a memory watchdog.
- [ ] Add failing lifecycle tests for tab destruction and IPC queue bounds; establish profile storage isolation and private ephemeral storage.
- [ ] Implement engine interfaces in Paisley, retaining existing session, library, download and notification behavior.
- [ ] Replace per-tab engine startup with shared profile-level engine ownership where supported; otherwise measure and document process overhead before adopting.
- [ ] Run native end-to-end navigation, history, bookmark, download, media, multilingual and tab-close tests on Bot Screen.
- [ ] Compare controlled repeated tab cycles using external PSS samples and inspect child-process exit; investigate retained objects separately from allocator caching.
- [ ] Install only after build and functional checks pass. Record unsupported behavior and measured memory results.

## Review focus
Profile separation; shutdown before destruction; unbounded frame/event queues; backend crash recovery; downloads and private-mode persistence.

## Ledger
- Existing source changes and E2E evidence preserved.
- Upstream servo-gtk c78c94079337803173fd40e69f5f7cd62347cac7 examined. MPL-2.0. Current runner uses SoftwareRenderingContext, one engine subprocess per widget, shared default config directory and unbounded IPC channels. These are implementation constraints, not solved by selecting Servo.

## Verified progress
- Existing suite: 49 library + 21 binary + 1 persistence integration tests passed.
- New native `PAISLEY_TAB_LIFETIME=1` check on Bot Screen: 12/12 cycles released WebView and UserContentManager; passed in 3.47 seconds. This tests app ownership, not engine allocator memory or all browsing workloads. GTK allocation warnings were emitted.
- Adapter dependencies are compiling; a completed adapter build or Paisley integration is not yet established. Build watchdog monitors only the owned Cargo tree, stopping it if RSS exceeds 4 GiB or system available RAM drops below 2 GiB.
- Further source inspection confirms `SoftwareRenderingContext::new` calls `create_software_adapter`; frames cross IPC as RGBA bytes. IPC currently exposes no history-change, favicon, notification, cookie or download events, even where underlying Servo exposes delegates.

## Resource stop
The owned build watchdog stopped dependency checking when system available RAM fell below 2 GiB (last reading 2,003,620 KiB), with approximately 200 MiB RSS in its owned tree at that instant. System swap subsequently measured almost full. A separate existing-runtime compatibility launch with `dom_adoptedstylesheet_enabled=true` was stopped immediately at 1,843 MiB available, before page loading; no compatibility conclusion follows. No Servo executable was built, Paisley integration was not implemented, and the installed backend was not changed. Do not present this stage as a successful production migration or memory fix.

Resume with the retained Cargo cache after sufficient system RAM is available, keeping one build job and memory monitoring. Do not close unrelated applications or clear unrelated caches. The `.cargo` checkout includes Servo test fixtures with deliberately invalid Cargo manifests; Cargo logged such a fixture during discovery but continued compilation. It was not the reason for stopping.
