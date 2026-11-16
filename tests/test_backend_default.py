"""Backend migration contracts, including Cargo's installed binary set."""
import pathlib
import tomllib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]

class BackendDefault(unittest.TestCase):
    def test_default_is_cef_and_legacy_is_explicit(self):
        cargo = tomllib.loads((ROOT / 'Cargo.toml').read_text())
        self.assertEqual(cargo['features']['default'], ['cef'])
        self.assertIn('legacy-webkit', cargo['features'])
        bins = {b['name']: b['path'] for b in cargo['bin']}
        self.assertEqual(bins['paisley'], 'src/launcher.rs')
        self.assertEqual(bins['paisley-engine'], 'src/main.rs')

    def test_desktop_installer_uses_launcher(self):
        source = (ROOT / 'src/desktop_install.rs').read_text()
        self.assertIn('launcher_executable', source)

if __name__ == '__main__':
    unittest.main()
