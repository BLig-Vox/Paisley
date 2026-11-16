# Packaging and runtime requirements

Published prerelease archives are unsigned. Their SHA-256 manifests detect payload changes but are not signatures. The local packager described below is a separate development workflow.

The archive contains an explicitly supplied, already-built release executable, a desktop entry, this document, and a payload checksum manifest. The packager never executes the supplied binary or extracts archives. It does not bundle GTK, WebKitGTK, or other shared libraries: the target system must provide compatible GTK 4 and WebKitGTK 6 runtimes and all required shared libraries. Users must obtain upstream Paisley updates independently; this packaging slice defines no update mechanism or policy.

Paisley uses the repository’s MIT license, declared in Cargo.toml. Release archives include LICENSE and bundled dependency notices. They are unsigned prerelease artifacts; checksums detect changes but are not signatures. Updates are installed explicitly from Cargo or GitHub Releases.

## First published release

The v0.1.0 Linux x86_64 binary requires glibc 2.39+ and compatible GTK4/WebKitGTK 6
runtimes. It was built with a newer Linux SDK and manually launched in Bot Screen
on Ubuntu 24.04, GTK 4.14.5, WebKitGTK 2.52.6 and Pango 1.52.1. Cargo source
builds still require Pango 1.55+ development files because of the locked bindings.
No automated tests were run for this release. Prior native UI checks covered
profiles, browsing sheets, downloads, media playback, multilingual text and
favicon persistence; this is not broad website or platform certification.

Install the source with `cargo install vox-paisley --version 0.1.0 --locked`.
Extract the binary archive, then install `paisley` on your PATH. The archive
includes the desktop entry, runtime notes, MIT license and third-party notices.
The application does not install system dependencies or update itself.

## Web video playback

WebKitGTK plays HTML5 media through GStreamer plugins provided by the operating system. Available video and audio formats depend on the installed plugins; DRM-protected streams may still be unavailable. The Paisley archive does not bundle GStreamer or codecs.

On Ubuntu or Debian, install the common playback plugin sets with:

```sh
sudo apt install \
  gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad \
  gstreamer1.0-plugins-ugly \
  gstreamer1.0-libav
```

For diagnostics, install `gstreamer1.0-tools` and inspect available elements with `gst-inspect-1.0`. The tools package is optional and is not required for playback.

## Emoji, symbols and language fonts

Paisley's GTK interface and WebKit pages use system font fallback. Install language fonts in the desktop running Paisley, then restart the browser. On Ubuntu/Debian:

```sh
sudo apt install fonts-noto-core fonts-noto-extra fonts-noto-cjk fonts-noto-color-emoji fonts-symbola
fc-cache -f
```

Noto CJK covers Chinese (Simplified/Traditional), Japanese and Korean. Noto Core/Extra adds many other scripts, Noto Color Emoji provides emoji and supported joined sequences, and Symbola adds symbol fallbacks. Toolbar icons are already bundled through Lucide; favicons still come from the website/cache. Websites with their own icon fonts must successfully load those fonts. These are operating system runtime dependencies, not fonts bundled into the Paisley archive. Bot Screen inspection on 2026-11-16 confirmed Simplified/Traditional Chinese, Japanese, Korean, color emoji (including skin tones, flags and joined sequences), symbols, Arabic, Hindi, Bengali, Thai, Greek, Cyrillic, Hebrew, Tamil, Georgian, Ethiopic, and mixed Unicode in the native bookmark title field. Font coverage varies by Unicode version; this is not a promise that every historic script, private-use icon or future emoji exists. Recreated Bot Screens need the packages installed again.

## Build and package locally

From the `Browny/` project directory, build the release binary using the project's locally available Rust toolchain and development libraries:

```sh
cargo build --locked --release
python3 -I scripts/package.py \
  --binary "$PWD/target/release/paisley" \
  --output "$PWD/dist/paisley-local.tar.gz" \
  --prefix /opt/paisley \
  --source-date-epoch 1735689600
```

The source epoch is required: a base-10 integer from 0 through 8589934591 (USTAR's timestamp range). Output paths must not already exist; completed archives are linked into place without overwriting, from a private staging file. Input leaf symlinks/nonregular files are refused using no-follow descriptors, and changing inputs fail. Binary size is bounded to 256 MiB, the desktop template to 64 KiB and this document to 1 MiB. Sources and the destination parent must be trusted local directories; this is not a sandbox for hostile filesystem ancestors.

The prefix is a canonical absolute ASCII path, at most 100 characters, with components beginning with a letter, digit, underscore or hyphen and continuing with those characters or dots. Spaces, shell/desktop field-code characters, repeated slashes and dot/parent components are refused. The packaged desktop entry uses the absolute `<prefix>/bin/paisley` executable, rather than depending on `/opt/paisley/bin` being in PATH. Packaging does not install files. To inspect without extracting, use:

```sh
tar -tzf dist/paisley-local.tar.gz
tar -tvzf dist/paisley-local.tar.gz
```

## Tests and desktop-entry validation

Run the focused standard-library tests:

```sh
python3 -I -m unittest discover -s tests -p test_package.py -v
```

If the desktop-entry validation utility `desktop-file-validate` is available, validate the source launcher without installing it:

```sh
desktop-file-validate packaging/site.blig.Paisley.desktop
```

The desktop entry launches `paisley` without URL arguments: the current application CLI supports `--profile ID` and `--private`, not a URL positional argument or URL association. It uses the bundled pink Paisley logo (`site.blig.Paisley`). The local packager includes the PNG and uses an absolute icon path so an `/opt` installation does not depend on a system icon search path. After a Cargo or extracted binary install, run `paisley --install-desktop` as your normal desktop user to register the launcher and logo under `$XDG_DATA_HOME` (normally `~/.local/share`). The logo and desktop template are embedded in the executable; no network fetch or source checkout is needed. This command is available starting with the next release.

If YouTube says “Your browser can’t play this video”, check the runtime inside
its actual desktop environment, not the build host. In the 2026-11-16 Bot Screen
incident, `dpkg-query` showed bad/ugly/libav absent and Paisley's log reported
`GStreamer element fakevideosink not found`. Installing the plugin sets above
and restarting Paisley restored advancing YouTube Shorts playback. The
`fakevideosink` element belongs to the bad plugin set; installing decoders alone
is not enough for that failure. Recreated desktops need these runtime packages
installed again. A browser reload alone may retain cached codec availability;
restart the browser after installation.

Post-install manual checks in Paisley: the same YouTube Short advanced through
multiple frames and moved its timeline; the downloaded MDN MP4, served from a
temporary loopback-only page, reached 5.05 seconds with 149 decoded frames and no
media error. `canPlayType` reported `probably` for H.264/MP4, VP9/WebM, and AAC.
The temporary playback server was stopped after checking. This validates those
examples, not every codec or protected stream.
