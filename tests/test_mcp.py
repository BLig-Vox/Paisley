"""Run with python3 -I tests/test_mcp.py; no browser or service is started."""
import importlib.util
from pathlib import Path
import unittest
import tempfile
import queue
import socket
import threading
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('browny_mcp', Path(__file__).resolve().parents[1] / 'assets/mcp.py')
mcp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mcp)


class ProtocolTests(unittest.TestCase):
    def ready(self):
        server = mcp.Server('http://127.0.0.1:18080')
        response = server.handle({'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'fixture', 'version': '1'}}})
        self.assertIsInstance(response, dict, 'initialize must return a protocol response')
        self.assertEqual(response['result']['protocolVersion'], '2025-11-25')
        self.assertIsNone(server.handle({'jsonrpc': '2.0', 'method': 'notifications/initialized'}))
        return server

    def test_fixture_origin_rejects_external_credentials_paths_and_implicit_ports(self):
        self.assertEqual(mcp.parse_origin('http://127.0.0.1:18080'), 'http://127.0.0.1:18080')
        for origin in ['http://example.org:18080', 'http://localhost:18080', 'file:///tmp/a', 'http://user@127.0.0.1:18080', 'http://127.0.0.1:18080/path', 'http://127.0.0.1', 'http://127.0.0.1:0', 'http://127.0.0.1:18080?x=1', 'http://127.0.0.1:18080#x', 'http://127.0.0.1:18080\n']:
            with self.subTest(origin=origin), self.assertRaises(ValueError):
                mcp.parse_origin(origin)

    def test_canonical_https_origins_and_exact_navigation_scope(self):
        for origin in ['https://example.org', 'https://example.org:8443', 'https://127.0.0.1:18443', 'https://[2606:4700:4700::1111]']:
            try:
                parsed = mcp.parse_origin(origin)
            except ValueError as error:
                self.fail(f'Canonical HTTPS grant rejected: {error}')
            self.assertEqual(parsed, origin)
            self.assertTrue(mcp.in_origin(origin + '/path?q=1', origin))
        for origin in ['https://EXAMPLE.org', 'https://example.org/', 'https://example.org:443', 'https://example.org:08443', 'https://user@example.org', 'https://example.org?x=1', 'https://example.org#x', 'https://example.org\\evil', 'https://example.org\n', 'https://example.org.', 'http://127.0.0.1:08080']:
            with self.subTest(origin=origin), self.assertRaises(ValueError):
                mcp.parse_origin(origin)
        for uri in ['http://example.org/', 'https://example.org.evil.test/', 'https://user@example.org/', 'https://example.org:8443/', 'https://example.org/\n', 'https://[invalid/']:
            self.assertFalse(mcp.in_origin(uri, 'https://example.org'), uri)
        self.assertTrue(mcp.in_origin('https://example.org:443/a', 'https://example.org'))

    def test_https_relay_accepts_only_exact_authority_connect_without_body(self):
        self.assertIsNone(mcp.validate_relay_request('CONNECT', 'example.org:443', {'Host': 'example.org:443'}, 'https://example.org'))
        self.assertIsNone(mcp.validate_relay_request('CONNECT', 'example.org:8443', {'Host': 'example.org:8443'}, 'https://example.org:8443'))
        for method, target, headers in [
                ('GET', 'https://example.org/a', {'Host': 'example.org'}),
                ('CONNECT', 'example.org:8443', {'Host': 'example.org:8443'}),
                ('CONNECT', 'example.org:443/extra', {'Host': 'example.org:443'}),
                ('CONNECT', 'user@example.org:443', {'Host': 'example.org:443'}),
                ('CONNECT', '127.0.0.1:443', {'Host': 'example.org:443'}),
                ('CONNECT', 'example.org:443', {'Host': 'other.example:443'}),
                ('CONNECT', 'example.org:443', {'Host': 'example.org:443', 'Content-Length': '1'}),
                ('CONNECT', 'example.org:443', {'Host': 'example.org:443', 'Transfer-Encoding': 'chunked'}),
                ('CONNECT', 'example.org:443', {'Host': 'example.org:443', 'Proxy-Authorization': 'dummy'})]:
            with self.subTest(method=method, target=target, headers=headers), self.assertRaises(ValueError):
                mcp.validate_relay_request(method, target, headers, 'https://example.org')

    def test_peer_pinning_rejects_nonpublic_and_mixed_answers(self):
        validate = getattr(mcp, 'validate_public_addresses', None)
        self.assertTrue(callable(validate), 'HTTPS peer validation is not implemented')
        self.assertEqual(validate(['1.1.1.1', '2606:4700:4700::1111', '1.1.1.1']), ('1.1.1.1', '2606:4700:4700::1111'))
        for addresses in [[], ['127.0.0.1'], ['10.0.0.1'], ['169.254.169.254'], ['100.64.0.1'], ['192.0.2.1'], ['0.0.0.0'], ['224.0.0.1'], ['240.0.0.1'], ['::1'], ['::'], ['fe80::1'], ['fc00::1'], ['ff02::1'], ['::ffff:127.0.0.1'], ['1.1.1.1', '127.0.0.1'], ['not-an-ip'], ['1.1.1.1'] * 17]:
            with self.subTest(addresses=addresses), self.assertRaises(ValueError):
                validate(addresses)

    def test_fixture_ca_cannot_grant_public_or_plain_http_origins(self):
        fixture = getattr(mcp, 'fixture_scope', None)
        self.assertTrue(callable(fixture), 'Scoped HTTPS fixture trust is not implemented')
        self.assertTrue(fixture('http://127.0.0.1:18080', None))
        self.assertTrue(fixture('https://127.0.0.1:18443', '/owned/ca.pem'))
        self.assertFalse(fixture('https://example.org', None))
        for origin, ca in [('https://127.0.0.1:18443', None), ('https://127.0.0.1', '/owned/ca.pem'), ('https://example.org', '/owned/ca.pem'), ('http://127.0.0.1:18080', '/owned/ca.pem')]:
            with self.subTest(origin=origin), self.assertRaises(ValueError):
                fixture(origin, ca)

    def test_resolution_pins_all_answers_and_rejects_mixed_destinations(self):
        # Only the resolver boundary is replaced: tests must never query public DNS.
        public = [(2, 1, 6, '', ('1.1.1.1', 443))]
        with patch.object(mcp.socket, 'getaddrinfo', return_value=public) as dns:
            self.assertEqual(mcp.resolve_peers('https://example.org', False), ('1.1.1.1',))
            dns.assert_called_once_with('example.org', 443, type=mcp.socket.SOCK_STREAM, proto=mcp.socket.IPPROTO_TCP)
        with patch.object(mcp.socket, 'getaddrinfo', return_value=public + [(2, 1, 6, '', ('127.0.0.1', 443))]):
            with self.assertRaises(ValueError):
                mcp.resolve_peers('https://example.org', False)
        with patch.object(mcp.socket, 'getaddrinfo', side_effect=AssertionError('fixture must not use DNS')):
            self.assertEqual(mcp.resolve_peers('https://127.0.0.1:18443', True), ('127.0.0.1',))

    def test_trust_reader_rejects_symlinks_private_keys_and_oversized_files(self):
        load = getattr(mcp, 'load_trust_bundle', None)
        self.assertTrue(callable(load), 'Bounded public-only CA reader is not implemented')
        with tempfile.TemporaryDirectory(prefix='browny-trust-test-') as directory:
            root = Path(directory)
            source = root / 'input.pem'
            for data in [b'-----BEGIN PRIVATE KEY-----\ndummy\n-----END PRIVATE KEY-----\n', b'not a certificate', b'A' * (2 * 1024 * 1024 + 1)]:
                source.write_bytes(data)
                with self.subTest(size=len(data)), self.assertRaises(ValueError):
                    load(source)
            link = root / 'link.pem'
            link.symlink_to(source)
            with self.assertRaises((ValueError, OSError)):
                load(link)
            with self.assertRaises(ValueError):
                load(root)

    def test_backend_does_not_read_tabs_without_native_grant_and_ready_filter(self):
        backend = object.__new__(mcp.Backend)
        for granted, ready in [(False, True), (True, False), (False, False)]:
            with self.subTest(granted=granted, ready=ready):
                with patch.object(backend, 'status', return_value={'granted': granted, 'filter_ready': ready}), patch.object(backend, 'command', return_value=['fixture']) as engine:
                    with self.assertRaises(ValueError):
                        backend.call('browser_tabs', {})
                    engine.assert_not_called()

    def test_changed_target_after_native_approval_is_not_clicked(self):
        backend = object.__new__(mcp.Backend)
        backend.snapshot = None
        state = {'granted': True, 'filter_ready': True, 'generation': 1, 'layout': 'unchanged-geometry'}
        old = {'element': {'element-6066-11e4-a52e-4f735466cecf': 'target'}, 'state': {'tag': 'BUTTON', 'label': 'Preview'}}
        new = {'element': old['element'], 'state': {'tag': 'BUTTON', 'label': 'Delete'}}
        with patch.object(backend, 'status', return_value=state), patch.object(backend, 'check_tab'), patch.object(backend, 'input_guard', return_value=state), patch.object(backend, 'layout', return_value='unchanged-geometry'), patch.object(backend, 'authorize'), patch.object(backend, 'capture_target', create=True, side_effect=[old, new]), patch.object(backend, 'element', return_value='/element/target'), patch.object(backend, 'mark_element'), patch.object(backend, 'command') as engine:
            with self.assertRaises(ValueError):
                backend.call('browser_click', {'tab': 'fixture', 'selector': '#send', 'observation': 1})
            engine.assert_not_called()

    def test_transport_eof_interrupts_pending_native_approval(self):
        pump = getattr(mcp, 'pump_input', None)
        self.assertTrue(callable(pump), 'MCP EOF is not watched while a native decision is pending')
        requests = queue.Queue(maxsize=16)
        disconnected, interrupted = threading.Event(), threading.Event()
        reader, writer = socket.socketpair()
        try:
            with reader.makefile('rb') as stream:
                worker = threading.Thread(target=pump, args=(stream, requests, disconnected, interrupted.set), daemon=True)
                worker.start()
                writer.sendall(b'{"jsonrpc":"2.0","id":1,"method":"tools/call"}\n')
                self.assertEqual(requests.get(timeout=1), b'{"jsonrpc":"2.0","id":1,"method":"tools/call"}\n')
                # The consumer is deliberately not reading another request while
                # awaiting consent. EOF must still interrupt that native wait.
                writer.shutdown(socket.SHUT_WR)
                self.assertTrue(interrupted.wait(1), 'disconnect did not interrupt pending control')
                self.assertTrue(disconnected.is_set())
                worker.join(1)
                self.assertFalse(worker.is_alive())
        finally:
            reader.close()
            writer.close()

    def test_tunnel_byte_quota_and_idle_deadline_close_forwarding(self):
        for payload, maximum, idle in [(b'exceeds', 4, .2), (b'', 32, .02)]:
            sockets = [*socket.socketpair(), *socket.socketpair()]
            left, sender, right, receiver = sockets
            errors = []
            def forward():
                try:
                    mcp.copy_tunnel(left, right, max_bytes=maximum, lifetime=.5, idle=idle)
                except BaseException as error:
                    errors.append(error)
            try:
                worker = threading.Thread(target=forward, daemon=True)
                worker.start()
                if payload:
                    sender.sendall(payload)
                worker.join(1)
                self.assertFalse(worker.is_alive(), 'forwarding outlived byte/idle bound')
                self.assertEqual(errors, [])
                receiver.settimeout(.02)
                with self.assertRaises(TimeoutError):
                    receiver.recv(32)
            finally:
                for endpoint in sockets:
                    endpoint.close()

    def test_pending_request_queue_is_bounded_and_overflow_revokes(self):
        import io
        requests = queue.Queue(maxsize=16)
        disconnected, interrupted = threading.Event(), threading.Event()
        mcp.pump_input(io.BytesIO(b'{}\n' * 17), requests, disconnected, interrupted.set)
        self.assertEqual(requests.qsize(), 16)
        self.assertTrue(disconnected.is_set() and interrupted.is_set())

    def test_initialization_lists_interactive_tools_without_starting_driver(self):
        server = self.ready()
        result = server.handle({'jsonrpc': '2.0', 'id': 2, 'method': 'tools/list'})
        tools = {tool['name']: tool for tool in result['result']['tools']}
        for name in ['browser_tabs', 'browser_observe', 'browser_navigate', 'browser_click', 'browser_click_at', 'browser_type', 'browser_key', 'browser_scroll', 'browser_wait', 'browser_stop']:
            self.assertIn(name, tools)
            self.assertFalse(tools[name]['inputSchema']['additionalProperties'])
        self.assertNotIn('evaluate', tools)
        self.assertIsNone(server.backend)

    def test_uninitialized_and_unknown_requests_are_errors(self):
        server = mcp.Server('http://127.0.0.1:18080')
        response = server.handle({'jsonrpc': '2.0', 'id': 1, 'method': 'tools/call', 'params': {'name': 'browser_tabs', 'arguments': {}}})
        self.assertIsInstance(response, dict, 'uninitialized calls must receive an error')
        self.assertEqual(response['error']['code'], -32002)
        response = server.handle({'jsonrpc': '2.0', 'id': 2, 'method': 'host_shell', 'params': {}})
        self.assertEqual(response['error']['code'], -32601)
        self.assertIsNone(server.backend)

    def test_malformed_arguments_rejected_before_backend_startup(self):
        server = self.ready()
        for name, arguments in [('browser_type', {}), ('browser_tabs', {'binary': '/bin/sh'}), ('browser_navigate', {'tab': 'x', 'url': 'http://example.org/'}), ('browser_click_at', {'tab': 'x', 'observation': 1, 'x': -1, 'y': 4}), ('browser_key', {'tab': 'x', 'observation': 1, 'key': 'RunShell'}), ('browser_stop', {'extra': True})]:
            result = server.handle({'jsonrpc': '2.0', 'id': 3, 'method': 'tools/call', 'params': {'name': name, 'arguments': arguments}})
            self.assertTrue(result['result']['isError'], (name, result))
            self.assertIsNone(server.backend)

    def test_stop_revokes_without_starting_driver(self):
        server = self.ready()
        result = server.handle({'jsonrpc': '2.0', 'id': 3, 'method': 'tools/call', 'params': {'name': 'browser_stop', 'arguments': {}}})
        self.assertFalse(result['result'].get('isError', False))
        self.assertTrue(server.stopped)
        result = server.handle({'jsonrpc': '2.0', 'id': 4, 'method': 'tools/call', 'params': {'name': 'browser_tabs', 'arguments': {}}})
        self.assertTrue(result['result']['isError'])
        self.assertIsNone(server.backend)

    def test_invalid_rpc_ids_and_notifications_never_trigger_actions(self):
        server = self.ready()
        self.assertIsNone(server.handle({'jsonrpc': '2.0', 'method': 'tools/call', 'params': {'name': 'browser_tabs', 'arguments': {}}}))
        response = server.handle({'jsonrpc': '2.0', 'id': [], 'method': 'tools/list'})
        self.assertEqual(response['error']['code'], -32600)
        self.assertIsNone(server.backend)

    def test_input_limits_and_observation_validation(self):
        for name, args in [('browser_type', {'tab': 'x', 'observation': True, 'selector': '#a', 'text': 'x'}), ('browser_type', {'tab': 'x', 'observation': 1, 'selector': '#a', 'text': 'x' * 32769}), ('browser_click', {'tab': '../x', 'observation': 1, 'selector': 'a'}), ('browser_wait', {'tab': 'x', 'selector': '#a', 'timeout_ms': 99999}), ('browser_scroll', {'tab': 'x', 'observation': 1, 'x': 4, 'y': 4, 'delta_y': 9999999, 'delta_x': 0})]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                mcp.validate_call(name, args)

    def test_text_rejects_reserved_webdriver_keys_and_invalid_unicode(self):
        for text in ['ab', 'a\ud800b', 'a\x00b', 'a\x1bb']:
            with self.subTest(codepoint=ord(text[1])), self.assertRaises(ValueError):
                mcp.validate_call('browser_type', {'tab': 'x', 'observation': 1, 'selector': '#a', 'text': text})

    def test_relay_only_accepts_the_granted_origin_without_tunnels(self):
        headers = {'Host': '127.0.0.1:18080'}
        self.assertEqual(mcp.validate_relay_request('GET', 'http://127.0.0.1:18080/a?q=1', headers, 'http://127.0.0.1:18080'), '/a?q=1')
        for method, target, extra in [('CONNECT', '127.0.0.1:18080', {}), ('GET', 'http://127.0.0.1:4444/', {}), ('GET', 'http://user@127.0.0.1:18080/', {}), ('GET', 'http://127.0.0.1:18080/', {'Upgrade': 'websocket'}), ('POST', 'http://127.0.0.1:18080/', {'Transfer-Encoding': 'chunked'}), ('GET', 'http://127.0.0.1:18080/', {'Host': 'example.org'}), ('GET', 'http://127.0.0.1:18080/\r\nX: y', {})]:
            with self.subTest(method=method, target=target), self.assertRaises(ValueError):
                mcp.validate_relay_request(method, target, dict(headers, **extra), 'http://127.0.0.1:18080')


if __name__ == '__main__':
    unittest.main()
