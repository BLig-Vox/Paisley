#!/usr/bin/env python3
"""Create a reproducible, unsigned local-development Browny tarball."""

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
DESKTOP = ROOT / "packaging" / "site.blig.Browny.desktop"
PACKAGING_DOC = ROOT / "docs" / "PACKAGING.md"


def read_regular(path, limit):
    """Read a bounded regular leaf from one no-follow descriptor."""
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as source:
        before = os.fstat(source.fileno())
        if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= limit:
            raise ValueError("packaging input must be a nonempty bounded regular file")
        data = source.read(limit + 1)
        after = os.fstat(source.fileno())
        if len(data) != before.st_size or len(data) > limit or (
            before.st_size, before.st_mtime_ns, before.st_ctime_ns
        ) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns):
            raise ValueError("packaging input changed while reading")
        return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, help="already-built release binary")
    parser.add_argument("--output", required=True, help="new .tar.gz output path")
    parser.add_argument("--prefix", required=True, help="absolute install prefix, e.g. /opt/browny")
    parser.add_argument("--source-date-epoch", required=True, help="reproducible USTAR timestamp")
    args = parser.parse_args()
    try:
        if not re.fullmatch(r"[0-9]{1,10}", args.source_date_epoch):
            raise ValueError("source-date-epoch must be a base-10 USTAR timestamp")
        epoch = int(args.source_date_epoch, 10)
        if epoch >= 8 ** 11:
            raise ValueError("source-date-epoch exceeds USTAR timestamp range")
        if len(args.prefix) > 100 or not re.fullmatch(
            r"(?:/[A-Za-z0-9_-][A-Za-z0-9_.-]*)+", args.prefix
        ):
            raise ValueError("prefix must be canonical absolute ASCII components, at most 100 characters")
        root = args.prefix[1:]
        output = Path(args.output)
        if output.exists() or output.is_symlink():
            raise ValueError("output already exists; refusing to overwrite")
        desktop = read_regular(DESKTOP, 65536)
        if desktop.count(b"Exec=browny\n") != 1:
            raise ValueError("desktop launcher template must have exactly one Exec=browny")
        desktop = desktop.replace(b"Exec=browny\n", f"Exec={args.prefix}/bin/browny\n".encode("ascii"))
        payloads = [
            (f"{root}/bin/browny", read_regular(args.binary, 256 * 1024 * 1024), 0o755),
            (f"{root}/share/applications/site.blig.Browny.desktop", desktop, 0o644),
            (f"{root}/share/doc/browny/PACKAGING.md", read_regular(PACKAGING_DOC, 1024 * 1024), 0o644),
        ]
        hashes = {name[len(root) + 1:]: hashlib.sha256(data).hexdigest()
                  for name, data, _mode in payloads}
        manifest = json.dumps({"files": hashes}, sort_keys=True, indent=2) + "\n"
        payloads.append((f"{root}/share/doc/browny/MANIFEST.json", manifest.encode("utf-8"), 0o644))
        payloads.sort(key=lambda item: item[0])
        output.parent.mkdir(parents=True, exist_ok=True)
        # Only expose a finished archive. Hard-link creation refuses races with
        # existing files/symlinks; the private staging file is always removed.
        with tempfile.NamedTemporaryFile(dir=output.parent, prefix=".browny-package-") as raw:
            with gzip.GzipFile(fileobj=raw, mode="wb", filename="", mtime=0, compresslevel=9) as compressed:
                with tarfile.open(fileobj=compressed, mode="w|", format=tarfile.USTAR_FORMAT) as archive:
                    for name, data, mode in payloads:
                        info = tarfile.TarInfo(name)
                        info.size = len(data)
                        info.mode = mode
                        info.uid = info.gid = 0
                        info.uname = info.gname = ""
                        info.mtime = epoch
                        archive.addfile(info, io.BytesIO(data))
            raw.flush()
            os.fsync(raw.fileno())
            os.link(raw.name, output)
        return 0
    except (OSError, ValueError, tarfile.TarError) as error:
        print(f"package.py: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
