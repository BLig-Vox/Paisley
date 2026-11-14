# Video Playback Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable common HTML5 video playback in Browny on the Bot Screen and document the Linux media runtime requirements.

**Architecture:** Keep playback in WebKitGTK and supply codecs through the system GStreamer plugin sets. Install missing plugin packages in the current Bot Screen and document the runtime package requirement without bundling native libraries in Browny.

**Tech Stack:** Rust, GTK4, WebKitGTK 6, GStreamer, Ubuntu apt.

**Spec:** `docs/superpowers/specs/2026-11-15-video-playback-design.md`

## Global Constraints

- Use WebKitGTK's existing GStreamer integration; do not alter playback gesture settings.
- Install the bad, libav, and ugly plugin sets in the current Bot Screen, retaining already installed base/good sets.
- Document the packages as Linux runtime dependencies, with tools available for diagnosing missing plugins.
- A video is considered verified only after it produces advancing playback in the running browser; subtitle-related warnings are tracked separately.

## Review Focus

- Missing GStreamer plugins: verify that the installed plugin sets are discoverable with `gst-inspect-1.0`.
- Common HTML5 playback: manually verify a video starts and advances after a user click in Browny.
- Subtitle warning versus decoder failure: report these separately; do not treat the WebVTT warning as proof of video decoding failure.
- DRM-protected media: document that installation cannot guarantee playback where the site or stream requires unsupported DRM.
- Other Linux installations: ensure the packaging instructions name the runtime package sets while preserving the existing no-native-library-bundling statement.

---

### Task 1: Document Linux media runtime requirements

**Files:**
- Modify: `docs/PACKAGING.md`

**Interfaces:**
- Consumes: the project runtime requirements and package choices from the approved spec.
- Produces: Linux setup guidance naming `gstreamer1.0-plugins-base`, `gstreamer1.0-plugins-good`, `gstreamer1.0-plugins-bad`, `gstreamer1.0-plugins-ugly`, and `gstreamer1.0-libav`; distinguish optional diagnostic tools from playback dependencies.

- [x] Add an Ubuntu/Debian package-install command for the named runtime plugin sets.
- [x] State that WebKitGTK uses system GStreamer codecs; available formats vary by installed plugin and some DRM-protected streams may not play.
- [x] Keep the existing statement that Browny's local archive does not bundle GTK, WebKitGTK, GStreamer, or other shared libraries.
- [x] Review the Markdown section and diff for accuracy and consistency; `git diff --check` passes.

### Task 2: Install media plugins and verify Bot Screen playback

**Files:**
- No source files; install packages in the Bot Screen Ubuntu environment.

**Interfaces:**
- Consumes: the package list documented in Task 1.
- Produces: installed GStreamer bad/libav/ugly sets and evidence of one user-initiated video playing in Browny.

- [x] Install `gstreamer1.0-plugins-bad`, `gstreamer1.0-libav`, `gstreamer1.0-plugins-ugly`, and `gstreamer1.0-tools` in the Bot Screen; the tools package is for diagnostics only.
- [x] Run `gst-inspect-1.0` for available H.264, VP9, and WebVTT elements; `avdec_h264`, `vp9dec`, `webvttenc`, and `h264parse` are available.
- [x] Relaunch Browny, reload the YouTube video, click Play, and confirm the video advances beyond its poster frame.
- [ ] If YouTube remains blocked, use a public non-DRM HTML5 video as a second playback check and report the precise remaining limitation.
