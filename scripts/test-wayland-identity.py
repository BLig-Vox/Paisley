#!/usr/bin/env python3
"""Exercise the real CLI, profile routing and desktop identity on nested Wayland.

Run inside Bot Screen: dbus-run-session -- python3 scripts/test-wayland-identity.py
--binary /absolute/path/to/paisley --evidence /absolute/output/directory
Requires Weston (X11 backend), GTK4 Python introspection and an X11 display.
Uses isolated HOME/XDG directories, loopback fixtures and its own session bus.
"""
import argparse
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import threading
import time
import urllib.parse

ROOT = Path(__file__).resolve().parents[1]
DESKTOP_ID = "site.blig.Paisley"


def terminate(signum, _frame):
    # Python's default SIGTERM action skips finally blocks. Unwind instead so
    # detached compositor/browser sessions are cleaned up on test timeouts.
    raise SystemExit(128 + signum)


def last_app_ids(trace):
    """Each toplevel's final ID, including changes after realization/remapping."""
    result = {}
    for object_id, app_id in re.findall(r'xdg_toplevel(?:@|#)(\d+)\.set_app_id\("([^"\n]*)"\)', trace):
        result[object_id] = app_id
    return result


def first_painted_id(trace):
    """ID of the first real toplevel when its first rendered buffer is sent."""
    surface = re.search(r'get_xdg_surface\(new id xdg_surface(?:@|#)(\d+), wl_surface(?:@|#)(\d+)\)', trace)
    if not surface:
        return None
    toplevel = re.search(r'xdg_surface(?:@|#)'+surface[1]+r'\.get_toplevel\(new id xdg_toplevel(?:@|#)(\d+)\)', trace)
    paint = re.search(r'wl_surface(?:@|#)'+surface[2]+r'\.attach\(wl_buffer', trace)
    if not toplevel or not paint:
        return None
    return last_app_ids(trace[:paint.start()]).get(toplevel[1])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--evidence', required=True, type=Path)
    parser.add_argument('--test-binary', type=Path, help='Optional Paisley unit-test ELF for realize/unrealize regression')
    parser.add_argument('--interrupt-after-first-window', action='store_true', help='Exercise SIGTERM cleanup (expected exit 143)')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    spec = importlib.util.spec_from_file_location('owned_processes', ROOT/'scripts/test-cef-native.py')
    owned = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(owned)
    seen, children, logs, cases = {}, [], [], []
    reports, incoming = {}, {}
    result = {'binary_sha256': hashlib.file_digest(binary.open('rb'), 'sha256').hexdigest(), 'passed': False}

    def observe():
        table = owned.process_table()
        pids = {os.getpid()}
        while True:
            before = len(pids)
            pids.update(pid for pid, value in table.items() if value[0] in pids)
            if len(pids) == before:
                break
        seen.update((pid, table[pid][1]) for pid in pids if pid != os.getpid() and pid in table)

    def wait(label, condition, seconds=20):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            observe()
            if condition():
                return
            available = next(int(line.split()[1]) for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:'))
            if available < 2 * 1024 * 1024:
                raise AssertionError('Stopped: less than 2 GiB system memory available')
            time.sleep(.15)
        raise AssertionError('Timed out: ' + label)

    class Fixture(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            url = urllib.parse.urlsplit(self.path)
            query = urllib.parse.parse_qs(url.query)
            name = query.get('case', [''])[0]
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Cache-Control', 'no-store')
            if url.path == '/report':
                reports[name] = self.headers.get('Cookie', '')
                body = b'ok'
            else:
                incoming[name] = self.headers.get('Cookie', '')
                if url.path == '/set':
                    self.send_header('Set-Cookie', 'identity=' + query['value'][0] + '; Path=/; HttpOnly; SameSite=Lax')
                body = ('<!doctype html><title>Paisley identity '+name+'</title><h1>'+name+'</h1><script>fetch("/report?case='+name+'")</script>').encode()
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_args):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base = 'http://127.0.0.1:' + str(server.server_port)
    with tempfile.TemporaryDirectory(prefix='paisley-wayland-identity-') as scratch:
        scratch = Path(scratch)
        env = os.environ.copy()
        for key in ('HOME', 'XDG_RUNTIME_DIR', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'XDG_CONFIG_HOME'):
            path = scratch/key.lower()
            path.mkdir(mode=0o700)
            env[key] = str(path)
        env.update(GDK_BACKEND='wayland', WAYLAND_DISPLAY='paisley-identity', GTK_A11Y='none',
                   GSK_RENDERER='cairo', LP_NUM_THREADS='1', RAYON_NUM_THREADS='1')
        # Do not inherit old engine fixture switches or alter engine sandboxing.
        for key in list(env):
            if key.startswith('PAISLEY_NATIVE_') or key.startswith('PAISLEY_CEF_') or key == 'PAISLEY_SERVO_NATIVE':
                env.pop(key)

        def spawn(command, name, trace=False):
            log = (evidence/(name+'.log')).open('w')
            logs.append(log)
            process = subprocess.Popen(command, env=dict(env, **({'WAYLAND_DEBUG': 'client'} if trace else {})),
                                       stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            children.append(process)
            observe()
            return process

        try:
            compositor = spawn(['weston', '--backend=x11-backend.so', '--renderer=pixman', '--width=1200', '--height=720',
                                '--socket=paisley-identity', '--idle-time=0', '--no-config'], 'weston')
            wait('nested Wayland socket', lambda: (Path(env['XDG_RUNTIME_DIR'])/'paisley-identity').exists())
            installed = subprocess.run([str(binary), '--install-desktop'], env=env, capture_output=True, timeout=20)
            assert installed.returncode == 0, installed.stderr.decode(errors='replace')
            desktop = Path(env['XDG_DATA_HOME'])/'applications'/f'{DESKTOP_ID}.desktop'
            icon = Path(env['XDG_DATA_HOME'])/'icons/hicolor/256x256/apps'/f'{DESKTOP_ID}.png'
            assert desktop.is_file() and f'Icon={DESKTOP_ID}\n' in desktop.read_text()
            assert icon.read_bytes() == (ROOT/'assets/branding/paisley-logo-stand.png').read_bytes()

            def launch(name, profile, private=False, value=None):
                path = '/set' if value else '/read'
                url = base+path+'?case='+name+('&value='+value if value else '')
                command = [str(binary), '--profile', profile] + (['--private'] if private else []) + [url]
                process = spawn(command, name, trace=True)
                wait(name+' loaded', lambda: name in reports or process.poll() is not None)
                assert process.poll() is None, name+' exited before displaying: '+(evidence/(name+'.log')).read_text(errors='replace')[-3000:]
                assert incoming[name] == '', (name, 'shared another profile cookie', incoming[name])
                assert reports[name] == ('identity='+value if value else ''), (name, reports[name])
                wait(name+' first painted frame', lambda: first_painted_id((evidence/(name+'.log')).read_text(errors='replace')))
                cases.append((name, profile, private, process))
                return process

            launch('normal-a', 'identity_a', value='A')
            if args.interrupt_after_first_window:
                os.kill(os.getpid(), signal.SIGTERM)
            launch('normal-b', 'identity_b', value='B')
            for profile, expected in [('identity_a', 'A'), ('identity_b', 'B')]:
                name = 'forward-'+expected
                remote = spawn([str(binary), '--profile', profile, base+'/read?case='+name], name, trace=True)
                wait(name+' forwarded', lambda: name in reports and remote.poll() is not None)
                assert remote.returncode == 0 and reports[name] == 'identity='+expected
                assert not last_app_ids((evidence/(name+'.log')).read_text(errors='replace')), 'Forwarding client created a window'
            launch('private-a', 'identity_a', private=True, value='PA')
            launch('private-a-second', 'identity_a', private=True)
            launch('private-b', 'identity_b', private=True, value='PB')
            assert all(process.poll() is None for _, _, _, process in cases)
            identities = {}
            for name, profile, private, process in cases:
                trace = (evidence/(name+'.log')).read_text(errors='replace')
                ids = last_app_ids(trace)
                assert ids, name+' did not create a native Wayland toplevel'
                identities[name] = ids
            result['toplevels'] = identities
            result['profile_cookie_checks'] = dict(reports)
            assert all(app_id == DESKTOP_ID for ids in identities.values() for app_id in ids.values()), identities
            result['first_painted_ids'] = {name: first_painted_id((evidence/(name+'.log')).read_text(errors='replace')) for name, _, _, _ in cases}
            assert set(result['first_painted_ids'].values()) == {DESKTOP_ID}, result['first_painted_ids']

            broken = Path(env['XDG_CONFIG_HOME'])/'browny/profiles/identity_broken'
            broken.mkdir(parents=True, mode=0o700)
            (broken/'settings.json').write_text('invalid settings fixture')
            error_window = spawn([str(binary), '--profile', 'identity_broken'], 'profile-error', trace=True)
            wait('profile error window painted', lambda: first_painted_id((evidence/'profile-error.log').read_text(errors='replace')))
            error_trace = (evidence/'profile-error.log').read_text(errors='replace')
            assert error_window.poll() is None and 'profile error' in error_trace
            result['profile_error_toplevels'] = last_app_ids(error_trace)
            assert set(result['profile_error_toplevels'].values()) == {DESKTOP_ID}

            lookup = '''import gi,json
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk,Gio,Gdk
Gtk.init()
app=Gio.DesktopAppInfo.new("site.blig.Paisley.desktop")
assert app and app.get_icon().to_string()=="site.blig.Paisley"
theme=Gtk.IconTheme.get_for_display(Gdk.Display.get_default())
icon=theme.lookup_icon("site.blig.Paisley",[],256,1,Gtk.TextDirection.NONE,Gtk.IconLookupFlags.FORCE_REGULAR)
assert icon and icon.get_file()
print(json.dumps({"desktop":app.get_filename(),"icon":icon.get_file().get_path()}))
'''
            resolved = subprocess.run(['python3', '-c', lookup], env=env, capture_output=True, text=True, timeout=20)
            assert resolved.returncode == 0, resolved.stderr
            result['desktop_lookup'] = json.loads(resolved.stdout)
            assert Path(result['desktop_lookup']['icon']).read_bytes() == icon.read_bytes()
            if args.test_binary:
                remap = spawn([str(args.test_binary.resolve(strict=True)), '--exact',
                               'wayland_recreated_surface', '--ignored', '--nocapture'],
                              'surface-recreation', trace=True)
                wait('recreated Wayland surface', lambda: remap.poll() is not None)
                trace = (evidence/'surface-recreation.log').read_text(errors='replace')
                assert remap.returncode == 0, trace[-3000:]
                ids = last_app_ids(trace)
                assert len(ids) >= 2 and set(ids.values()) == {DESKTOP_ID}, ids
                result['recreated_surfaces'] = ids
            result['passed'] = True
            print('PASS: five native Wayland windows, two normal profiles, three private instances, isolated cookies, single-instance link forwarding, canonical app IDs and pink icon lookup.', flush=True)
        except (Exception, SystemExit, KeyboardInterrupt) as error:
            result['error'] = str(error)
            result['interrupted'] = isinstance(error, (SystemExit, KeyboardInterrupt))
            raise
        finally:
            observe()
            for process in reversed(children):
                if process.poll() is None:
                    process.terminate()
            for process in reversed(children):
                try:
                    process.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
            for pid in owned.alive_owned(seen):
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            for log in logs:
                log.close()
            server.shutdown()
            time.sleep(.2)
            result['owned_processes_after_exit'] = owned.alive_owned(seen)
            (evidence/'result.json').write_text(json.dumps(result, indent=2)+'\n')
            assert not result['owned_processes_after_exit'], result['owned_processes_after_exit']


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, terminate)
    main()
