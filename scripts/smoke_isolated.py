#!/usr/bin/env python3
"""Run the compiled ignored GTK fixture with no external network or host profiles.

This is a test runner, not an automation backend. Supply Cargo's compiled browny
unit-test executable, not target/debug/browny. Uses installed tools only.
"""
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile


def main():
    restart_stress = len(sys.argv) == 3 and sys.argv[2] == '--restart-stress'
    restart_processes = len(sys.argv) == 3 and sys.argv[2] == '--restart-processes'
    inspector_modes = ('--inspector-lifecycle', '--inspector-preface', '--inspector-open-close')
    inspector_lifecycle = len(sys.argv) == 3 and sys.argv[2] in inspector_modes
    inspector_preface = len(sys.argv) == 3 and sys.argv[2] == '--inspector-preface'
    inspector_open_close = len(sys.argv) == 3 and sys.argv[2] == '--inspector-open-close'
    if not (len(sys.argv) == 2 or restart_stress or restart_processes or inspector_lifecycle or (len(sys.argv) == 4 and sys.argv[2] == '--screenshots')):
        raise ValueError("usage: python3 -I scripts/smoke_isolated.py ABSOLUTE_TEST_EXECUTABLE [--screenshots NEW_ABSOLUTE_DIRECTORY | --restart-stress | --restart-processes | --inspector-lifecycle | --inspector-preface | --inspector-open-close]")
    binary = Path(sys.argv[1])
    if not binary.is_absolute() or not stat.S_ISREG(binary.lstat().st_mode):
        raise ValueError("expected an absolute regular, nonsymlink test executable")
    uid, gid = os.getuid(), os.getgid()
    runtime = Path(os.environ['XDG_RUNTIME_DIR'])
    display = os.environ['WAYLAND_DISPLAY']
    if '/' in display or not (runtime / display).is_socket():
        raise ValueError("a local Wayland display socket is required")
    with tempfile.TemporaryDirectory(prefix='browny-ordinary-smoke-') as scratch:
        root = Path(scratch)
        (root / 'passwd').write_text(f'agent:x:{uid}:{gid}:Owned fixture:/home/agent:/usr/bin/false\n')
        (root / 'group').write_text(f'agent:x:{gid}:\n')
        (root / 'machine-id').write_text('0123456789abcdef0123456789abcdef\n')
        (root / 'bus.conf').write_text('<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth><policy context="default"><allow send_destination="*"/><allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        private = f'/run/user/{uid}'
        command = ['/usr/bin/bwrap', '--unshare-all', '--die-with-parent', '--new-session',
            '--ro-bind', '/usr', '/usr', '--symlink', 'usr/lib', '/lib', '--symlink', 'usr/lib', '/lib64',
            '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp',
            '--size', '1048576', '--tmpfs', '/tmp/browny-download-capacity',
            '--dir', '/home/agent',
            '--dir', '/sys/block', '--dir', '/sys/bus', '--dir', '/sys/class', '--dir', '/sys/dev', '--dir', '/sys/devices',
            '--dir', '/var/tmp', '--dir', '/var/cache/fontconfig', '--dir', '/app',
            '--ro-bind', str(binary), '/app/browny-test',
            '--ro-bind', str(root / 'passwd'), '/etc/passwd', '--ro-bind', str(root / 'group'), '/etc/group',
            '--ro-bind', str(root / 'machine-id'), '/etc/machine-id', '--ro-bind', str(root / 'bus.conf'), '/app/bus.conf',
            '--perms', '0700', '--dir', private, '--ro-bind', str(runtime / display), private + '/' + display,
            '--clearenv', '--setenv', 'PATH', '/usr/bin', '--setenv', 'HOME', '/home/agent',
            '--setenv', 'XDG_RUNTIME_DIR', private, '--setenv', 'WAYLAND_DISPLAY', display,
            '--setenv', 'GDK_BACKEND', 'wayland', '--setenv', 'LANG', 'C.UTF-8',
            '--setenv', 'GSK_RENDERER', 'cairo', '--setenv', 'GTK_A11Y', 'none',
            '--setenv', 'LIBGL_ALWAYS_SOFTWARE', '1', '--setenv', 'G_DEBUG', 'fatal-criticals',
            '--setenv', 'BROWNY_DOWNLOAD_CAPACITY_FIXTURE', '/tmp/browny-download-capacity',
            '--chdir', '/home/agent']
        if restart_stress:
            command += ['--setenv', 'BROWNY_RESTART_STRESS', '1',
                        '--setenv', 'WEBKIT_DEBUG', 'Process']
        if restart_processes:
            command += ['--setenv', 'BROWNY_RESTART_PROCESSES', '1']
        if inspector_lifecycle:
            command += ['--setenv', 'BROWNY_INSPECTOR_LIFECYCLE', '1']
        if inspector_preface:
            command += ['--setenv', 'BROWNY_INSPECTOR_PREFACE', '1']
        if inspector_open_close:
            command += ['--setenv', 'BROWNY_INSPECTOR_OPEN_CLOSE', '1']
        if len(sys.argv) == 4:
            output = Path(sys.argv[3])
            if not output.is_absolute():
                raise ValueError('screenshot directory must be absolute and new')
            output.mkdir(mode=0o700)
            command += ['--bind', str(output), '/evidence',
                        '--setenv', 'BROWNY_SETTINGS_SCREENSHOT', '/evidence/settings.png',
                        '--setenv', 'BROWNY_PROFILES_SCREENSHOT', '/evidence/profiles.png',
                        '--setenv', 'BROWNY_BOOKMARKS_SCREENSHOT', '/evidence/bookmarks.png',
                        '--setenv', 'BROWNY_BOOKMARK_EXPORT_SCREENSHOT', '/evidence/bookmark-export.png',
                        '--setenv', 'BROWNY_PLACEHOLDER_SCREENSHOT', '/evidence/placeholder.png']
        if Path('/etc/fonts').is_dir():
            command += ['--ro-bind', '/etc/fonts', '/etc/fonts']
        command += ['/usr/bin/dbus-run-session', '--config-file=/app/bus.conf', '--', '/app/browny-test',
                    'browser_smoke', '--ignored', '--test-threads=1', '--nocapture']
        return subprocess.run(command, timeout=90).returncode


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        print(f'smoke_isolated: {error}', file=sys.stderr)
        raise SystemExit(2)
