# Local development packaging

This is a local-development-only packaging path. The generated archive is unsigned; its SHA-256 manifest detects accidental payload changes but is not a signature and does not establish publisher identity or provenance. This is not distribution or release readiness.

The archive contains an explicitly supplied, already-built release executable, a desktop entry, this document, and a payload checksum manifest. The packager never executes the supplied binary or extracts archives. It does not bundle GTK, WebKitGTK, or other shared libraries: the target system must provide compatible GTK 4 and WebKitGTK 6 runtimes and all required shared libraries. Users must obtain upstream Browny updates independently; this packaging slice defines no update mechanism or policy.

There is no license choice in `Cargo.toml`; redistribution/release is blocked pending the project owner's license choice, dependency notices and provenance, signing, and update policy. Do not infer a license or treat checksums as signatures.

## Web video playback

WebKitGTK plays HTML5 media through GStreamer plugins provided by the operating system. Available video and audio formats depend on the installed plugins; DRM-protected streams may still be unavailable. The Browny archive does not bundle GStreamer or codecs.

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

## Build and package locally

From the `Browny/` project directory, build the release binary using the project's locally available Rust toolchain and development libraries:

```sh
cargo build --locked --release
python3 -I scripts/package.py \
  --binary "$PWD/target/release/browny" \
  --output "$PWD/dist/browny-local.tar.gz" \
  --prefix /opt/browny \
  --source-date-epoch 1735689600
```

The source epoch is required: a base-10 integer from 0 through 8589934591 (USTAR's timestamp range). Output paths must not already exist; completed archives are linked into place without overwriting, from a private staging file. Input leaf symlinks/nonregular files are refused using no-follow descriptors, and changing inputs fail. Binary size is bounded to 256 MiB, the desktop template to 64 KiB and this document to 1 MiB. Sources and the destination parent must be trusted local directories; this is not a sandbox for hostile filesystem ancestors.

The prefix is a canonical absolute ASCII path, at most 100 characters, with components beginning with a letter, digit, underscore or hyphen and continuing with those characters or dots. Spaces, shell/desktop field-code characters, repeated slashes and dot/parent components are refused. The packaged desktop entry uses the absolute `<prefix>/bin/browny` executable, rather than depending on `/opt/browny/bin` being in PATH. Packaging does not install files. To inspect without extracting, use:

```sh
tar -tzf dist/browny-local.tar.gz
tar -tvzf dist/browny-local.tar.gz
```

## Tests and desktop-entry validation

Run the focused standard-library tests:

```sh
python3 -I -m unittest discover -s tests -p test_package.py -v
```

If the desktop-entry validation utility `desktop-file-validate` is available, validate the source launcher without installing it:

```sh
desktop-file-validate packaging/site.blig.Browny.desktop
```

The desktop entry launches `browny` without URL arguments: the current application CLI supports `--profile ID` and `--private`, not a URL positional argument or URL association. It uses the generic `web-browser` icon name.
