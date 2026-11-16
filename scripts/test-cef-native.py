#!/usr/bin/env python3
"""Supervise a native CEF fixture test in the desktop session, with owned-process PSS evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

MIB = 1024


def process_table():
    result = {}
    for directory in Path('/proc').glob('[0-9]*'):
        try:
            fields = dict(line.split(':', 1) for line in (directory / 'status').read_text().splitlines() if ':' in line)
            # starttime disambiguates PID reuse; comm may contain spaces/parentheses.
            stamp = (directory / 'stat').read_text().rsplit(')', 1)[1].split()[19]
            result[int(directory.name)] = (int(fields['PPid']), stamp, fields['State'].strip(), int(fields.get('VmRSS', '0 kB').split()[0]))
        except (OSError, ValueError, KeyError, IndexError):
            pass
    return result


def alive_owned(seen):
    table = process_table()
    return [pid for pid, stamp in seen.items() if pid in table and table[pid][1] == stamp and not table[pid][2].startswith('Z')]


def runtime_criticals(log):
    return [line for line in log.splitlines() if "GLib-GIO-CRITICAL" in line or "GLib-CRITICAL" in line]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--runtime', required=True, type=Path)
    parser.add_argument('--evidence', required=True, type=Path)
    parser.add_argument('--allow-root-test-no-sandbox', action='store_true', help='Explicit temporary exception; requires debug cef-test-support binary')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    runtime = args.runtime.resolve(strict=True)
    if binary.open('rb').read(4) != b'\x7fELF' or not (runtime / 'libcef.so').is_file():
        parser.error('Expected a native test ELF and extracted CEF runtime')
    if os.geteuid() == 0 and not args.allow_root_test_no_sandbox:
        parser.error('CEF sandbox requires a normal desktop user; root fixture tests require the explicit temporary exception')
    available = next(int(line.split()[1]) for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:'))
    if available < 2 * 1024 * MIB:
        parser.error('Less than 2 GiB system memory available; test was not started')
    args.evidence.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(PAISLEY_SERVO_NATIVE='1', PAISLEY_ENGINE_DIAGNOSTICS='1', GTK_A11Y='none', RAYON_NUM_THREADS='1', LP_NUM_THREADS='1', PAISLEY_CEF_RUNTIME=str(runtime))
    env['LD_LIBRARY_PATH'] = str(runtime) + (':' + env['LD_LIBRARY_PATH'] if env.get('LD_LIBRARY_PATH') else '')
    env.pop('PAISLEY_CEF_TEST_NO_SANDBOX', None)
    if args.allow_root_test_no_sandbox:
        env['PAISLEY_CEF_TEST_NO_SANDBOX'] = '1'
    report = {'binary': str(binary), 'sha256': hashlib.file_digest(binary.open('rb'), 'sha256').hexdigest(), 'temporary_no_sandbox': args.allow_root_test_no_sandbox, 'samples': [], 'running': True}
    start = time.monotonic()
    seen = {}
    reason = None
    with (args.evidence / 'runtime.log').open('w') as log:
        child = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            while child.poll() is None:
                table = process_table()
                owned = {child.pid}
                while True:
                    previous = len(owned)
                    owned.update(pid for pid, fields in table.items() if fields[0] in owned)
                    if len(owned) == previous:
                        break
                pss = 0
                fallback = False
                for pid in owned:
                    if pid not in table:
                        continue
                    seen[pid] = table[pid][1]
                    try:
                        pss += next(int(line.split()[1]) for line in Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines() if line.startswith('Pss:'))
                    except (OSError, StopIteration):
                        pss += table[pid][3]
                        fallback = True
                available = next(int(line.split()[1]) for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:'))
                report['samples'].append({'seconds': round(time.monotonic() - start, 2), 'mib': round(pss / MIB, 2), 'metric': 'PSS+RSS fallback' if fallback else 'PSS', 'pids': sorted(owned), 'available_mib': available // MIB})
                (args.evidence / 'memory.json').write_text(json.dumps(report, indent=2))
                if pss > 3 * 1024 * MIB or available < 2 * 1024 * MIB or time.monotonic() - start > 240:
                    reason = 'Memory/time safety cutoff'
                    break
                time.sleep(0.5)
        except KeyboardInterrupt:
            reason = 'Interrupted'
        finally:
            if child.poll() is None:
                try:
                    os.killpg(child.pid, signal.SIGTERM)
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
                except ProcessLookupError:
                    child.wait()
    deadline = time.monotonic() + 3
    live = alive_owned(seen)
    while live and time.monotonic() < deadline:
        time.sleep(0.2)
        live = alive_owned(seen)
    criticals = runtime_criticals((args.evidence / "runtime.log").read_text(errors="replace"))
    report.update(owned_processes_after_exit=live, running=False, exit_code=child.returncode, reason=reason, runtime_criticals=criticals)
    (args.evidence / 'memory.json').write_text(json.dumps(report, indent=2))
    print('CEF native exit', child.returncode, 'peak', max((s['mib'] for s in report['samples']), default=0), 'MiB', reason, 'remaining owned processes', live)
    # Clean only identities observed in this test, after recording the failure.
    for pid in alive_owned(seen):
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    return 1 if reason or live or criticals or child.returncode != 0 else 0


if __name__ == '__main__':
    sys.exit(main())
