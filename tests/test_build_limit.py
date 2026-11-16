import importlib.util
from pathlib import Path
import sys
import unittest
import json
import tempfile
from types import SimpleNamespace
from unittest.mock import patch, MagicMock

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('build_limited', ROOT/'scripts/build-limited.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

class BuildLimitTests(unittest.TestCase):
    def test_three_decimal_gb_is_passed_as_exact_bytes_to_cgroup(self):
        process = MagicMock()
        process.wait.return_value = 0
        with patch.object(sys, 'argv', ['build-limited.py', '--limit-mb', '3000', '--', 'cargo', 'check']), patch.object(module.subprocess, 'Popen', return_value=process) as popen, patch.object(module.subprocess, 'run'):
            self.assertEqual(module.main(), 0)
        command = popen.call_args.args[0]
        self.assertIn('--property=MemoryMax=3000000000', command)
        self.assertIn('--property=MemorySwapMax=0', command)
        self.assertEqual(command[command.index('--limit-mb')+1], '3000')

    def test_worker_verifies_page_rounded_cap_and_one_job(self):
        with tempfile.TemporaryDirectory() as directory:
            group = Path(directory)
            requested = 3_000_000_000
            actual = requested // module.os.sysconf('SC_PAGE_SIZE') * module.os.sysconf('SC_PAGE_SIZE')
            for name, value in [('memory.max', str(actual)), ('memory.swap.max', '0'),
                                ('memory.events', 'oom 0\noom_kill 0\n'), ('memory.peak', '1024')]:
                (group/name).write_text(value)
            args = SimpleNamespace(limit_mb=3000, limit_gib=2, report=group/'report.json')
            process = MagicMock(returncode=0)
            process.poll.return_value = 0
            with patch.object(module, 'cgroup_directory', return_value=group), patch.object(module, 'available_bytes', return_value=8*1024**3), patch.object(module.subprocess, 'Popen', return_value=process) as popen:
                self.assertEqual(module.worker(args, ['cargo', 'check']), 0)
            self.assertEqual(popen.call_args.kwargs['env']['CARGO_BUILD_JOBS'], '1')
            result = json.loads(args.report.read_text())
            self.assertEqual(result['requested_limit_bytes'], requested)
            self.assertEqual(result['limit_bytes'], actual)
            self.assertLessEqual(result['limit_bytes'], requested)
            self.assertEqual(result['swap_limit_mib'], 0)
            for name, value in [('memory.max', 'max'), ('memory.swap.max', '1')]:
                (group/name).write_text(value)
                with patch.object(module, 'cgroup_directory', return_value=group), patch.object(module.subprocess, 'Popen') as launch:
                    with self.assertRaisesRegex(RuntimeError, 'Unsafe build limits'):
                        module.worker(args, ['cargo', 'check'])
                    launch.assert_not_called()
                (group/name).write_text(str(actual) if name == 'memory.max' else '0')

if __name__ == '__main__':
    unittest.main()
