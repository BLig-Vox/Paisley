"""Bounded native-input fixture: python3 -I tests/mcp_smoke.py /absolute/browny.
No external site, personal profile or raw host WebDriver listener is used.
"""
import base64
import http.server
import json
import os
from pathlib import Path
import selectors
import struct
import subprocess
import sys
import tempfile
import threading
import time

PAGE = b'''<!doctype html><meta charset="utf-8"><title>Interactive fixture</title>
<style>body{height:3000px;font:20px sans-serif}input,button{font:inherit}#result{white-space:pre-wrap}</style>
<h1>Browny native automation fixture</h1><form onsubmit="event.preventDefault();document.querySelector('#result').textContent=JSON.stringify({text:document.querySelector('#message').value,trusted:event.isTrusted})"><label>Message <input id="message"></label>
<button id="send" type="submit">Send</button></form>
<pre id="result">Waiting</pre><pre id="keyboard">Keyboard waiting</pre><p style="margin-top:1700px">Scroll target</p>
<script>document.querySelector('#message').addEventListener('keydown', e => {document.querySelector('#keyboard').textContent='Keyboard trusted: '+e.isTrusted})</script>'''


class Fixture(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.server.requests.append(self.path)
        if self.path == '/redirect':
            self.send_response(302)
            self.send_header('Location', 'http://127.0.0.1:1/denied')
            self.end_headers()
            return
        page = PAGE
        if self.path == '/egress':
            page += ('<pre id="egress">egress waiting</pre><script>fetch("http://127.0.0.1:' + str(self.server.foreign_port) + '/foreign").then(()=>document.querySelector("#egress").textContent="egress allowed").catch(()=>document.querySelector("#egress").textContent="egress denied")</script>').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(page)))
        self.end_headers()
        self.wfile.write(page)

    def log_message(self, *_):
        pass


class Client:
    def __init__(self, binary, origin, log, extra_args=()):
        self.proc = subprocess.Popen([binary, '--mcp', '--allow-origin', origin, *extra_args], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log)
        self.counter = 0
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.proc.stdout, selectors.EVENT_READ)

    def request(self, method, params=None):
        self.counter += 1
        value = {'jsonrpc': '2.0', 'id': self.counter, 'method': method}
        if params is not None:
            value['params'] = params
        self.proc.stdin.write(json.dumps(value).encode() + b'\n')
        self.proc.stdin.flush()
        assert self.selector.select(30), f'timeout waiting for {method}'
        line = self.proc.stdout.readline(20 * 1024 * 1024)
        assert line, f'no response to {method}; process exit={self.proc.poll()}'
        assert line.startswith(b'{'), f'non-protocol stdout: {line[:200]!r}'
        result = json.loads(line)
        assert result.get('id') == self.counter, result
        assert 'error' not in result, result
        return result['result']

    def tool(self, name, arguments=None, error=False):
        result = self.request('tools/call', {'name': name, 'arguments': arguments or {}})
        assert bool(result.get('isError')) == error, (name, result)
        return result

    def data(self, result):
        return json.loads(next(item['text'] for item in result['content'] if item['type'] == 'text'))

    def close(self):
        self.proc.stdin.close()
        try:
            self.proc.wait(8)
        except subprocess.TimeoutExpired:
            self.proc.terminate()
            self.proc.wait(5)
        self.selector.close()


def descendants(root):
    parents = {}
    for directory in Path('/proc').iterdir():
        if not directory.name.isdecimal():
            continue
        try:
            parent = next(line.split()[1] for line in (directory / 'status').read_text().splitlines() if line.startswith('PPid:'))
            parents[int(directory.name)] = int(parent)
        except (OSError, StopIteration):
            pass
    owned = {root}
    for _ in range(20):
        found = {pid for pid, parent in parents.items() if parent in owned}
        if found <= owned:
            break
        owned |= found
    return owned


def main(binary, screenshot=None):
    site = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
    site.requests = []
    foreign = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
    foreign.requests = []
    site.foreign_port = foreign.server_port
    foreign.foreign_port = site.server_port
    threading.Thread(target=foreign.serve_forever, daemon=True).start()
    threading.Thread(target=site.serve_forever, daemon=True).start()
    origin = f'http://127.0.0.1:{site.server_port}'
    with tempfile.TemporaryDirectory(prefix='browny-mcp-smoke-') as directory:
        with open(Path(directory) / 'stderr.log', 'w+b') as log:
            client = Client(binary, origin, log)
            try:
                client.request('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'native-fixture', 'version': '1'}})
                client.proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
                client.proc.stdin.flush()
                assert 'browser_type' in [tool['name'] for tool in client.request('tools/list')['tools']]
                tabs = client.data(client.tool('browser_tabs'))['tabs']
                assert len(tabs) == 1 and tabs[0]['ephemeral']
                tab = tabs[0]['id']
                driver_pids = []
                for pid in descendants(client.proc.pid):
                    try:
                        command = Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')
                        if command[0] == b'/usr/bin/WebKitWebDriver':
                            driver_pids.append(pid)
                            assert os.readlink(f'/proc/{pid}/ns/net') != os.readlink('/proc/self/ns/net'), 'raw driver must not share the host network namespace'
                    except FileNotFoundError:
                        pass
                assert len(driver_pids) == 1, driver_pids
                client.tool('browser_navigate', {'tab': tab, 'url': origin + '/'})
                state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                assert state['origin'] == origin and 'Message' in state['page']['text'], state
                observation = state['observation']
                text = 'Hello, Browny — 世界 🦀'
                client.tool('browser_type', {'tab': tab, 'observation': observation, 'selector': '#message', 'text': text, 'clear': True})
                client.tool('browser_type', {'tab': tab, 'observation': observation, 'selector': '#message', 'text': 'stale'}, error=True)
                state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                assert 'Keyboard trusted: true' in state['page']['text'], 'keyboard input must be trusted, not DOM mutation'
                client.tool('browser_key', {'tab': tab, 'observation': state['observation'], 'key': 'Enter'})
                client.tool('browser_wait', {'tab': tab, 'selector': '#result', 'timeout_ms': 1000})
                state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                assert text in state['page']['text'] and '"trusted":true' in state['page']['text'], 'Enter must submit the form with trusted input'
                client.tool('browser_click', {'tab': tab, 'observation': state['observation'], 'selector': '#send'})
                observed = client.tool('browser_observe', {'tab': tab})
                state = client.data(observed)
                assert text in state['page']['text'] and '"trusted":true' in state['page']['text'], state
                assert isinstance(state.get('cursor'), dict), 'native selector input must publish its trusted cursor position'
                image = next(item for item in observed['content'] if item['type'] == 'image')
                png = base64.b64decode(image['data'], validate=True)
                assert png[:8] == b'\x89PNG\r\n\x1a\n'
                width, height = struct.unpack('>II', png[16:24])
                assert width >= 640 and height >= 300, (width, height)
                if screenshot is not None:
                    with Path(screenshot).open('xb') as output:
                        output.write(png)
                # Exercise coordinate input independently of selector-based native click.
                button = next(item for item in state['page']['elements'] if item['id'] == 'send')
                rect = button['rect']
                client.tool('browser_click_at', {'tab': tab, 'observation': state['observation'], 'x': int(rect['x'] + rect['width'] / 2), 'y': int(rect['y'] + rect['height'] / 2)})
                state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                client.tool('browser_scroll', {'tab': tab, 'observation': state['observation'], 'x': 300, 'y': 250, 'delta_x': 0, 'delta_y': 500})
                time.sleep(0.3)
                state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                assert state['page']['scrollY'] > 0, state
                client.tool('browser_navigate', {'tab': tab, 'url': 'http://127.0.0.1:1/forbidden'}, error=True)
                client.tool('browser_observe', {'tab': 'ungranted-window'}, error=True)
                client.tool('browser_navigate', {'tab': tab, 'url': origin + '/egress'})
                deadline = time.monotonic() + 3
                while True:
                    state = client.data(client.tool('browser_observe', {'tab': tab, 'screenshot': False}))
                    if 'egress denied' in state['page']['text']:
                        break
                    assert time.monotonic() < deadline, state
                    time.sleep(0.05)
                assert foreign.requests == [], foreign.requests
                # Close the transport while a second workspace is alive below;
                # first verify explicit Stop revokes and reaps this driver.
                client.tool('browser_stop')
                client.tool('browser_tabs', error=True)
                assert all(not Path(f'/proc/{pid}').exists() for pid in driver_pids), 'Stop must reap the driver'
                client.close()
                client = Client(binary, origin, log)
                client.request('initialize', {'protocolVersion': '2025-06-18', 'capabilities': {}, 'clientInfo': {'name': 'disconnect-fixture', 'version': '1'}})
                client.proc.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
                client.proc.stdin.flush()
                client.tool('browser_tabs')
                children = descendants(client.proc.pid) - {client.proc.pid}
                client.close()  # EOF, deliberately no browser_stop tool.
                deadline = time.monotonic() + 3
                while any(Path(f'/proc/{pid}').exists() for pid in children):
                    assert time.monotonic() < deadline, 'Disconnect leaked a workspace process'
                    time.sleep(0.05)
                print('PASS: private network namespace, trusted click/Unicode/coordinate/wheel input, PNG, stale/wrong-tab/origin/egress rejection, Stop and disconnect process cleanup')
            except BaseException:
                log.flush()
                log.seek(0)
                print(log.read(20000).decode(errors='replace'), file=sys.stderr)
                raise
            finally:
                client.close()
                site.shutdown()
                site.server_close()
                foreign.shutdown()
                foreign.server_close()


if __name__ == '__main__':
    main(str(Path(sys.argv[1]).resolve()), sys.argv[2] if len(sys.argv) > 2 else None)
