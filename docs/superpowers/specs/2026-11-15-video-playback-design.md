# Video Playback Support

## Intent

Make common HTML5 video playback work in Browny on the Ubuntu Bot Screen, including YouTube, and state the Linux runtime requirements for other installations.

## Goals

- Keep HTML video playback in the existing WebKitGTK engine.
- Install the missing GStreamer codec and subtitle plugins in the Bot Screen.
- Document the recommended GStreamer runtime packages alongside Browny's Linux runtime requirements.
- Verify playback in Browny with a user-initiated video.

## Non-goals

- Bundle GTK, WebKitGTK, or GStreamer into Browny's executable/archive.
- Guarantee playback for every website, DRM-protected stream, codec, or hardware decoder.
- Change autoplay or media-permission policy.

## Constraints

- Browny already creates WebKitGTK WebViews and allows media playback after a user gesture.
- The Bot Screen has GStreamer base/good plugins, but lacks bad, libav, and ugly plugin packages. Its `gst-inspect-1.0` utility is absent, so runtime codec inspection will be available after installing GStreamer tools or packages that provide it.
- The warning observed so far concerns WebVTT subtitle handling; it does not by itself identify the video decoder failure.
- Packaged Browny currently relies on system-provided GTK4/WebKitGTK and other native runtime libraries.

## Approaches

1. **Use system GStreamer plugins (selected).** Install `gstreamer1.0-plugins-bad`, `gstreamer1.0-libav`, and `gstreamer1.0-plugins-ugly` in the Bot Screen; document the standard plugin sets and explain that codec availability remains OS-provided. Small, consistent with the existing WebKitGTK architecture and packaging model.
2. **Bundle a separate media runtime.** Ship and maintain GStreamer/codec libraries with Browny. This duplicates native system components and substantially expands packaging and maintenance.
3. **Add a custom video player/decoder path.** Replace WebKit's native HTML media stack. This would not reliably support YouTube's site/player integration and is outside the browser's current architecture.

## Implementation decisions

- Use WebKitGTK's existing GStreamer integration; do not alter playback gesture settings.
- Install the bad, libav, and ugly plugin sets in the current Bot Screen, retaining already installed base/good sets.
- Document the packages as Linux runtime dependencies, with tools available for diagnosing missing plugins.
- A video is considered verified only after it produces advancing playback in the running browser; subtitle-related warnings are tracked separately.
