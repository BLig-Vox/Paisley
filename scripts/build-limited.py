#!/usr/bin/env python3
"""Run a build inside a kernel-enforced memory cgroup; never silently fall back."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid

MIB = 1024 * 1024


def cgroup_directory():
    for line in Path('/proc/self/cgroup').read_text().splitlines():
        if line.startswith('0::'):
            return Path('/sys/fs/cgroup') / line[3:].lstrip('/')
    raise RuntimeError('cgroup v2 is required')


def available_bytes():
    for line in Path('/proc/meminfo').read_text().splitlines():
        if line.startswith('MemAvailable:'):
            return int(line.split()[1]) * 1024
    raise RuntimeError('MemAvailable is unavailable')


def stop(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def configured_limit(args):
    # --limit-mb is decimal: 3000 MB is 3,000,000,000 bytes, not 3 GiB.
    return args.limit_mb * 1_000_000 if args.limit_mb is not None else args.limit_gib * 1024 * MIB


def worker(args, command):
    group = cgroup_directory()
    limit = configured_limit(args)
    actual = (group / 'memory.max').read_text().strip()
    swap = (group / 'memory.swap.max').read_text().strip()
    # The kernel rounds memory.max down to a whole page. Never accept a
    # larger value or an arbitrary smaller limit as proof of this cap.
    expected = limit // os.sysconf('SC_PAGE_SIZE') * os.sysconf('SC_PAGE_SIZE')
    if actual != str(expected) or swap != '0':
        raise RuntimeError(f'Unsafe build limits: memory.max={actual}, swap.max={swap}')
    if available_bytes() < 4096 * MIB:
        raise RuntimeError('Less than 4 GiB system memory available; build was not started')
    env = os.environ.copy()
    env.update(CARGO_BUILD_JOBS='1', CARGO_PROFILE_DEV_DEBUG='0',
               CARGO_PROFILE_TEST_DEBUG='0', CARGO_PROFILE_DEV_CODEGEN_UNITS='256',
               CARGO_PROFILE_RELEASE_LTO='false', CARGO_PROFILE_RELEASE_CODEGEN_UNITS='256',
               CMAKE_BUILD_PARALLEL_LEVEL='1', NUM_JOBS='1', RAYON_NUM_THREADS='1')
    # Cargo propagates its one-job jobserver to native build tools.
    report = {'requested_limit_bytes': limit, 'limit_bytes': int(actual), 'cargo_jobs': 1, 'limit_mib': round(int(actual) / MIB, 2), 'swap_limit_mib': 0, 'running': True,
              'command': command, 'peak_mib': 0, 'reason': None}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2))
    print(f'Build cap: {limit // MIB} MiB RAM, no swap, one build job', flush=True)
    start = time.monotonic()
    process = subprocess.Popen(command, env=env, start_new_session=True)
    reason = None
    try:
        while process.poll() is None:
            current = int((group / 'memory.current').read_text())
            report.update(peak_mib=max(report['peak_mib'], round(current / MIB, 2)),
                          seconds=round(time.monotonic() - start, 1))
            args.report.write_text(json.dumps(report, indent=2))
            if available_bytes() < 4096 * MIB:
                reason = 'System available memory fell below 2 GiB'
                stop(process)
                break
            time.sleep(0.25)
    except (KeyboardInterrupt, SystemExit):
        reason = 'Interrupted'
        stop(process)
    finally:
        if process.poll() is None:
            stop(process)
        events = (group / 'memory.events').read_text()
        peak = group / 'memory.peak'
        if peak.exists():
            report['peak_mib'] = round(int(peak.read_text()) / MIB, 2)
        report.update(running=False, exit_code=process.returncode, reason=reason,
                      seconds=round(time.monotonic() - start, 1), memory_events=events)
        args.report.write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2), flush=True)
    return process.returncode if process.returncode >= 0 else 128 - process.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    limits = parser.add_mutually_exclusive_group()
    limits.add_argument('--limit-gib', type=int, choices=(1, 2), default=2)
    limits.add_argument('--limit-mb', type=int, choices=(2000, 3000,), help='Hard decimal RAM cap in MB')
    parser.add_argument('--report', type=Path,
                        default=Path(__file__).resolve().parents[1] / 'target/build-limited.json')
    parser.add_argument('--worker', action='store_true', help=argparse.SUPPRESS)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command:
        parser.error('provide the build command after --')
    args.report = args.report.resolve()
    if args.worker:
        return worker(args, command)
    # A scope inherits this shell's environment (CEF_PATH, Cargo target dir,
    # toolchain and linker settings) while limiting every build descendant.
    unit = 'paisley-build-' + uuid.uuid4().hex[:12]
    limit = configured_limit(args)
    limit_args = ['--limit-mb', str(args.limit_mb)] if args.limit_mb is not None else ['--limit-gib', str(args.limit_gib)]
    runner = ['systemd-run', '--user', '--scope', '--quiet',
              '--unit=' + unit,
              f'--property=MemoryMax={limit}',
              f'--property=MemoryHigh={limit * 7 // 8}',
              '--property=MemorySwapMax=0', '--', sys.executable,
              str(Path(__file__).resolve()), '--worker', *limit_args,
              '--report', str(args.report), '--', *command]
    process = subprocess.Popen(runner)
    try:
        return process.wait()
    except KeyboardInterrupt:
        return 130
    finally:
        # Stop all scope members, including children that changed process groups.
        subprocess.run(['systemctl', '--user', 'stop', unit + '.scope'],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if process.poll() is None:
            process.wait()


if __name__ == '__main__':
    def terminate(_signal, _frame):
        raise SystemExit(143)
    signal.signal(signal.SIGTERM, terminate)
    try:
        sys.exit(main())
    except (OSError, RuntimeError) as error:
        print(f'Build not started or stopped: {error}', file=sys.stderr)
        sys.exit(1)
