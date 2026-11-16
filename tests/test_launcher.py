"""Run with PAISLEY_TEST_LAUNCHER pointing to the capped-build launcher."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

@unittest.skipUnless(os.environ.get('PAISLEY_TEST_LAUNCHER'), 'requires capped native launcher build')
class Launcher(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='paisley-launcher-e2e-')
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root/'bin').mkdir()
        self.launcher = self.root / 'bin/paisley'
        shutil.copyfile(os.environ['PAISLEY_TEST_LAUNCHER'], self.launcher)
        self.launcher.chmod(0o755)
        self.runtime = self.root / 'runtime with spaces'
        files = ['libcef.so', 'icudtl.dat', 'resources.pak', 'chrome_100_percent.pak',
                 'chrome_200_percent.pak', 'v8_context_snapshot.bin', 'chrome-sandbox',
                 'locales/en-US.pak', 'include/cef_version.h']
        for file in files:
            p = self.runtime / file
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text('#define CEF_VERSION "156.0.30+g6028375+chromium-156.0.8078.96"\n')
        self.engine = self.root / 'bin/paisley-engine'
        self.engine.write_text('#!/bin/sh\nprintf "%s\\n" "$PAISLEY_CEF_RUNTIME" "$LD_LIBRARY_PATH" "$@"\nexit 37\n')
        self.engine.chmod(0o755)
        self.env = dict(os.environ, PAISLEY_CEF_RUNTIME=str(self.runtime), LD_LIBRARY_PATH='/previous/path')

    def run_launcher(self, *args):
        return subprocess.run([self.launcher, *args], env=self.env, text=True, capture_output=True)

    def test_exec_forwards_arguments_runtime_environment_and_exit_status(self):
        args = ['--profile', 'two words', '--private', 'https://example.com/?a=1&b=2']
        result = self.run_launcher(*args)
        self.assertEqual(result.returncode, 37, result.stderr)
        self.assertEqual(result.stdout.splitlines(), [str(self.runtime), str(self.runtime)+':/previous/path', *args])

    def test_help_without_runtime_or_engine(self):
        self.engine.unlink()
        self.env['PAISLEY_CEF_RUNTIME'] = '/nonexistent'
        result = self.run_launcher('--help')
        self.assertEqual(result.returncode, 0)
        self.assertIn('Backend: CEF', result.stdout)

    def test_invalid_explicit_runtime_fails_without_starting_engine(self):
        self.env['PAISLEY_CEF_RUNTIME'] = '/nonexistent'
        result = self.run_launcher('https://example.com')
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, '')
        self.assertIn('PAISLEY_CEF_RUNTIME', result.stderr)

    def test_wrong_version_is_rejected(self):
        (self.runtime/'include/cef_version.h').write_text('#define CEF_VERSION "155.0.0"\n')
        self.assertEqual(self.run_launcher('--backend-info').returncode, 1)

    def test_missing_engine_fails_before_exec(self):
        self.engine.unlink()
        self.assertIn('Missing sibling paisley-engine', self.run_launcher().stderr)

    def test_relative_explicit_runtime_is_rejected(self):
        self.env['PAISLEY_CEF_RUNTIME'] = 'runtime with spaces'
        self.assertEqual(self.run_launcher().returncode, 1)

    def test_relocatable_prefix_runtime(self):
        runtime = self.root/'lib/paisley/cef'
        runtime.parent.mkdir(parents=True)
        self.runtime.rename(runtime)
        self.env.pop('PAISLEY_CEF_RUNTIME', None)
        self.env.pop('CEF_PATH', None)
        result = self.run_launcher('--backend-info')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(str(runtime), result.stdout)

    def test_user_data_runtime(self):
        data = self.root/'data'
        runtime = data/'paisley/cef'
        runtime.parent.mkdir(parents=True)
        self.runtime.rename(runtime)
        self.env['XDG_DATA_HOME'] = str(data)
        self.env.pop('PAISLEY_CEF_RUNTIME', None)
        self.env.pop('CEF_PATH', None)
        result = self.run_launcher('--backend-info')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(str(runtime), result.stdout)

    def test_backend_info_reports_the_resolved_runtime(self):
        result = self.run_launcher('--backend-info')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(str(self.runtime), result.stdout)

if __name__ == '__main__':
    unittest.main()
