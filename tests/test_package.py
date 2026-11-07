import hashlib
import io
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "package.py"
DESKTOP = Path(__file__).resolve().parents[1] / "packaging" / "site.blig.Browny.desktop"


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binary = self.root / "browny"
        self.binary.write_bytes(b"dummy release executable\x00\xff")
        self.binary.chmod(0o755)
        self.output = self.root / "out.tar.gz"

    def run_package(self, *args):
        return subprocess.run(
            [sys.executable, "-I", str(SCRIPT), "--binary", str(self.binary),
             "--output", str(self.output), "--prefix", "/opt/browny",
             "--source-date-epoch", "123456789", *args],
            text=True, capture_output=True,
        )

    def archive(self):
        with tarfile.open(self.output, "r:gz") as archive:
            return archive, archive.getmembers()

    def test_packages_binary_manifest_reproducibly_with_prefix_layout(self):
        first = self.run_package()
        self.assertEqual(first.returncode, 0, first.stderr)
        first_bytes = self.output.read_bytes()
        with tarfile.open(self.output, "r:gz") as archive:
            members = archive.getmembers()
            expected = [
                "opt/browny/bin/browny",
                "opt/browny/share/applications/site.blig.Browny.desktop",
                "opt/browny/share/doc/browny/MANIFEST.json",
                "opt/browny/share/doc/browny/PACKAGING.md",
            ]
            self.assertEqual([item.name for item in members], expected)
            for member in members:
                self.assertEqual(member.uid, 0)
                self.assertEqual(member.gid, 0)
                self.assertEqual(member.mtime, 123456789)
            binary = archive.extractfile(members[0]).read()
            manifest = json.load(archive.extractfile(members[2]))
            self.assertEqual(binary, self.binary.read_bytes())
            desktop = archive.extractfile(members[1]).read()
            self.assertIn(b"Exec=/opt/browny/bin/browny\n", desktop)
            expected_hashes = {
                "bin/browny": hashlib.sha256(binary).hexdigest(),
                "share/applications/site.blig.Browny.desktop": hashlib.sha256(DESKTOP.read_bytes().replace(b"Exec=browny\n", b"Exec=/opt/browny/bin/browny\n")).hexdigest(),
                "share/doc/browny/PACKAGING.md": hashlib.sha256((DESKTOP.parents[1] / "docs" / "PACKAGING.md").read_bytes()).hexdigest(),
            }
            self.assertEqual(manifest, {"files": expected_hashes})
        self.output.unlink()
        second = self.run_package()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(self.output.read_bytes(), first_bytes)

    def test_rejects_existing_output_without_overwrite(self):
        self.output.write_bytes(b"keep me")
        result = self.run_package()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.output.read_bytes(), b"keep me")

    def test_rejects_negative_or_noninteger_source_epoch(self):
        for epoch in ("-1", "not-an-epoch"):
            with self.subTest(epoch=epoch):
                result = self.run_package("--source-date-epoch", epoch)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.output.exists())

    def test_rejects_ustar_epoch_overflow_before_creating_output(self):
        result = self.run_package("--source-date-epoch", str(8 ** 11))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.output.exists(), "invalid timestamp left a partial archive")

    def test_rejects_noncanonical_or_launcher_unsafe_prefix(self):
        for index, prefix in enumerate(("/opt/a b", "/opt/%f", "/opt/../browny", "//opt/browny", "/opt/./browny", "/opt/" + "x" * 101)):
            with self.subTest(prefix=prefix):
                output = self.root / f"invalid-{index}.tar.gz"
                result = self.run_package("--prefix", prefix, "--output", str(output))
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(output.exists())

    def test_rejects_symlink_and_nonregular_binary(self):
        link = self.root / "link"
        link.symlink_to(self.binary)
        result = self.run_package("--binary", str(link))
        self.assertNotEqual(result.returncode, 0)
        fifo = self.root / "fifo"
        os.mkfifo(fifo)
        result = self.run_package("--binary", str(fifo))
        self.assertNotEqual(result.returncode, 0)

    def test_desktop_launcher_matches_cli_and_has_no_url_handler(self):
        text = DESKTOP.read_text()
        self.assertIn("Exec=browny", text)
        self.assertIn("Icon=web-browser", text)
        self.assertNotIn("%u", text)
        self.assertNotIn("MimeType=", text)
        self.assertIn("Terminal=false", text)


if __name__ == "__main__":
    unittest.main()
