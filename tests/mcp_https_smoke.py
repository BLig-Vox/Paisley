"""Owned HTTPS fixtures only; never contacts a public website or changes host trust.
Run: python3 -I tests/mcp_https_smoke.py /absolute/path/to/paisley [new-png-path]
Requires the normal isolated MCP dependencies and installed openssl.
"""
import base64
import http.server
import importlib.util
import os
from pathlib import Path
import ssl
import subprocess
import sys
import tempfile
import threading
import time

spec = importlib.util.spec_from_file_location('paisley_fixture', Path(__file__).with_name('mcp_smoke.py'))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


class HTTPSFixture(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.server.requests.append((self.headers.get('Host'), self.path))
        page = fixture.PAGE
        if self.path == '/mutation-gate':
            assert self.server.mutation.wait(5), 'owned mutation fixture timed out'
            page = b'change'
        if self.path == '/same-geometry':
            page += b'''<button id="stable" style="width:120px;height:40px">Preview</button>
<script>fetch('/mutation-gate').then(()=>{document.querySelector('#stable').textContent='Delete';const marker=document.createElement('span');marker.id='changed';marker.hidden=true;document.body.append(marker)})</script>'''
        if self.path == '/egress':
            page += ('<pre id="egress">waiting</pre><script>fetch("https://127.0.0.1:' + str(self.server.foreign_port) + '/foreign").then(()=>document.querySelector("#egress").textContent="foreign allowed").catch(()=>document.querySelector("#egress").textContent="foreign denied")</script>').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(page)))
        self.end_headers()
        self.wfile.write(page)

    def log_message(self, *_):
        pass


def certificate(directory, stem):
    cert, key = directory / (stem + '.pem'), directory / (stem + '.key')
    subprocess.run(['/usr/bin/openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
                    '-subj', '/CN=Paisley owned HTTPS fixture', '-addext', 'subjectAltName=IP:127.0.0.1',
                    '-addext', 'basicConstraints=critical,CA:TRUE', '-keyout', str(key), '-out', str(cert)],
                   check=True, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                   timeout=20, cwd='/')
    os.chmod(key, 0o600)
    return cert, key


def ready(client):
    client.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'owned-https-fixture', 'version': '1'}})
    client.proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
    client.proc.stdin.flush()
    tabs = client.data(client.tool('browser_tabs'))['tabs']
    assert len(tabs) == 1 and tabs[0]['ephemeral'], tabs
    return tabs[0]['id']


def main(binary, screenshot=None):
    with tempfile.TemporaryDirectory(prefix='paisley-https-fixture-') as name:
        directory = Path(name)
        cert, key = certificate(directory, 'fixture')
        wrong, _ = certificate(directory, 'unrelated')
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(cert, key)
        sites = []
        for _ in range(2):
            site = http.server.ThreadingHTTPServer(('127.0.0.1', 0), HTTPSFixture)
            site.requests = []
            site.mutation = threading.Event()
            site.socket = context.wrap_socket(site.socket, server_side=True)
            sites.append(site)
        site, foreign = sites
        site.foreign_port = foreign.server_port
        foreign.foreign_port = site.server_port
        for server in sites:
            threading.Thread(target=server.serve_forever, daemon=True).start()
        origin = f'https://127.0.0.1:{site.server_port}'
        client = None
        with (directory / 'stderr.log').open('w+b') as log:
            try:
                client = fixture.Client(binary, origin, log, ['--fixture-ca', str(cert)])
                tab = ready(client)
                client.tool('browser_navigate', {'tab': tab, 'url': origin + '/'})
                state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                assert state['origin'] == origin and 'Message' in state['page']['text'], state
                text = 'HTTPS native input — 世界 🦀'
                client.tool('browser_type', {'tab': tab, 'observation': state['observation'], 'selector': '#message', 'text': text, 'clear': True})
                state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                client.tool('browser_click', {'tab': tab, 'observation': state['observation'], 'selector': '#send'})
                observed = client.tool('browser_observe', {'tab': tab})
                state = client.data(observed)
                assert text in state['page']['text'] and '"trusted":true' in state['page']['text'], state
                assert 'Keyboard trusted: true' in state['page']['text']
                image = next(item for item in observed['content'] if item['type'] == 'image')
                png = base64.b64decode(image['data'], validate=True)
                assert png.startswith(b'\x89PNG\r\n\x1a\n')
                if screenshot:
                    with Path(screenshot).open('xb') as out:
                        out.write(png)
                client.tool('browser_scroll', {'tab': tab, 'observation': state['observation'], 'x': 300, 'y': 250, 'delta_x': 0, 'delta_y': 500})
                deadline = time.monotonic() + 3
                while True:
                    state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                    if state['page']['scrollY'] > 0:
                        break
                    assert time.monotonic() < deadline, state
                client.tool('browser_navigate', {'tab': tab, 'url': f'https://127.0.0.1:{foreign.server_port}/'}, error=True)
                client.tool('browser_navigate', {'tab': tab, 'url': origin + '/egress'})
                deadline = time.monotonic() + 4
                while True:
                    state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                    if 'foreign denied' in state['page']['text']:
                        break
                    assert time.monotonic() < deadline, state
                assert foreign.requests == [], foreign.requests
                client.tool('browser_navigate', {'tab': tab, 'url': origin + '/same-geometry'})
                old = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                before = next(item for item in old['page']['elements'] if item['id'] == 'stable')
                assert before['label'] == 'Preview', before
                site.mutation.set()
                client.tool('browser_wait', {'tab': tab, 'selector': '#changed', 'timeout_ms': 3000})
                client.tool('browser_click', {'tab': tab, 'observation': old['observation'], 'selector': '#stable'}, error=True)
                after = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                changed = next(item for item in after['page']['elements'] if item['id'] == 'stable')
                assert changed['label'] == 'Delete' and changed['rect'] == before['rect'], (before, changed)
                assert all(host == f'127.0.0.1:{site.server_port}' for host, _ in site.requests)
                children = fixture.descendants(client.proc.pid) - {client.proc.pid}
                client.tool('browser_stop')
                client.tool('browser_tabs', error=True)
                client.close()
                deadline = time.monotonic() + 4
                while any(Path(f'/proc/{pid}').exists() for pid in children):
                    assert time.monotonic() < deadline, 'HTTPS Stop leaked owned processes'
                    time.sleep(.05)
                # Wrong CA must fail at TLS, not load any fixture HTTP content.
                count = len(site.requests)
                client = fixture.Client(binary, origin, log, ['--fixture-ca', str(wrong)])
                tab = ready(client)
                result = client.request('tools/call', {'name': 'browser_navigate', 'arguments': {'tab': tab, 'url': origin + '/untrusted-cert'}})
                if not result.get('isError'):
                    state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                    assert 'Paisley native automation fixture' not in state['page']['text'], state
                assert len(site.requests) == count, 'untrusted certificate reached HTTP fixture handler'
                client.close()
                print('PASS: owned HTTPS TLS validation, trusted Unicode/click/wheel input, PNG, foreign-origin denial and Stop cleanup; no live sites contacted')
            except BaseException:
                log.flush()
                log.seek(0)
                print(log.read(30000).decode(errors='replace'), file=sys.stderr)
                raise
            finally:
                if client:
                    client.close()
                for server in sites:
                    server.shutdown()
                    server.server_close()


if __name__ == '__main__':
    main(str(Path(sys.argv[1]).resolve()), sys.argv[2] if len(sys.argv) > 2 else None)
