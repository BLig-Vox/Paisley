#!/usr/bin/env python3
"""Bundle prebuilt Paisley executables and the pinned CEF runtime without compiling."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
CEF_VERSION = '#define CEF_VERSION "156.0.8+g426920c+chromium-156.0.8078.48"'
REQUIRED = ('libcef.so', 'icudtl.dat', 'resources.pak', 'chrome_100_percent.pak',
            'chrome_200_percent.pak', 'v8_context_snapshot.bin', 'chrome-sandbox',
            'locales/en-US.pak', 'include/cef_version.h', 'CREDITS.html', 'archive.json')


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binaries', type=Path, required=True)
    parser.add_argument('--runtime', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--source-date-epoch', type=int, required=True)
    args = parser.parse_args()
    if not 0 <= args.source_date_epoch < 8 ** 11:
        parser.error('Invalid archive timestamp')
    for name in REQUIRED:
        if not (args.runtime / name).is_file():
            parser.error(f'Missing runtime file: {name}')
    if CEF_VERSION not in (args.runtime / 'include/cef_version.h').read_text().splitlines():
        parser.error('Wrong CEF runtime version')
    for name in ('paisley', 'paisley-engine'):
        with (args.binaries / name).open('rb') as stream:
            header = stream.read(20)
        if header[:5] != b'\x7fELF\x02' or header[18:20] != b'\x3e\x00':
            parser.error(f'{name} is not a Linux x86_64 executable')
    version = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    prefix = f'paisley-v{version}-linux-x86_64'
    files = [(f'bin/{name}', args.binaries / name, 0o755) for name in ('paisley', 'paisley-engine')]
    for path in sorted(args.runtime.rglob('*')):
        relative = path.relative_to(args.runtime).as_posix()
        # Distribute runtime data, not the C++ build SDK. The version header
        # also preserves CEF's license and is used by the launcher validator.
        selected = (path.parent == args.runtime and
                    ('.so' in path.name or path.suffix in ('.pak', '.dat', '.bin', '.json') or
                     path.name in ('chrome-sandbox', 'chrome_crashpad_handler', 'CREDITS.html')))
        selected |= relative.startswith('locales/') or relative == 'include/cef_version.h'
        if selected and path.is_file():
            if not path.resolve().is_relative_to(args.runtime.resolve()):
                parser.error('Runtime symlink escapes its directory')
            files.append(('lib/paisley/cef/' + relative, path,
                          0o755 if path.name in ('chrome-sandbox', 'chrome_crashpad_handler') else 0o644))
    for name in ('LICENSE', 'README.md', 'THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_LICENSES.txt',
                 'docs/PACKAGING.md', 'docs/CEF_PRERELEASE.md', 'engine/cef-gtk/LICENSE',
                 'engine/cef-gtk/UPSTREAM.md', 'packaging/CEF-LICENSE.txt'):
        files.append(('share/doc/paisley/' + name, ROOT / name, 0o644))
    files.extend([
        ('share/applications/site.blig.Paisley.desktop', ROOT / 'packaging/site.blig.Paisley.desktop', 0o644),
        ('share/icons/hicolor/256x256/apps/site.blig.Paisley.png', ROOT / 'assets/branding/paisley-logo-stand.png', 0o644),
    ])
    manifest = json.dumps({'version': version, 'cef': '156.0.8',
                           'files': {name: digest(path) for name, path, _ in files}},
                          sort_keys=True, indent=2).encode() + b'\n'
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=args.output.parent, prefix='.paisley-cef-') as raw:
        with gzip.GzipFile(fileobj=raw, mode='wb', filename='', mtime=0, compresslevel=6) as compressed:
            with tarfile.open(fileobj=compressed, mode='w|', format=tarfile.PAX_FORMAT) as archive:
                for name, path, mode in sorted(files):
                    info = tarfile.TarInfo(prefix + '/' + name)
                    info.size, info.mode, info.mtime = path.stat().st_size, mode, args.source_date_epoch
                    with path.open('rb') as stream:
                        archive.addfile(info, stream)
                info = tarfile.TarInfo(prefix + '/MANIFEST.json')
                info.size, info.mode, info.mtime = len(manifest), 0o644, args.source_date_epoch
                archive.addfile(info, io.BytesIO(manifest))
        raw.flush()
        os.fsync(raw.fileno())
        os.link(raw.name, args.output)
    print(json.dumps({'archive': str(args.output), 'version': version,
                      'sha256': digest(args.output), 'files': len(files)}))


if __name__ == '__main__':
    main()
