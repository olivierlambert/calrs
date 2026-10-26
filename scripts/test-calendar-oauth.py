#!/usr/bin/env python3
"""Exercise calendar OAuth configuration and HTTP authorization boundaries.

Run after cargo build: python3 scripts/test-calendar-oauth.py
Only the standard library is required. Uses an isolated database and synthetic
OAuth credentials; never follows an external redirect or contacts a provider.
"""
import hashlib
import http.client
import json
import os
import secrets
import socket
import sqlite3
import subprocess
import tempfile
import time
from http.cookies import SimpleCookie
from pathlib import Path
from urllib.parse import parse_qs, urlencode, urlsplit

binary = str(Path(os.environ.get('CALRS_BINARY', 'target/debug/calrs')).resolve())
env = {key: value for key, value in os.environ.items() if not key.startswith('CALRS_')}
env['CALRS_BASE_URL'] = 'https://calendar.example.test'
env['RUST_LOG'] = 'calrs=info,tower_http=trace'

with tempfile.TemporaryDirectory(prefix='calrs-calendar-oauth-') as temporary:
    directory = Path(temporary)

    def cli(*args, payload=None):
        return subprocess.run(
            [binary, '--data-dir', temporary, *args], env=env,
            input=json.dumps(payload) if payload is not None else None,
            capture_output=True, text=True, check=True,
        ).stdout

    client_id = '00000000-0000-0000-0000-000000000001'
    secret = secrets.token_urlsafe(32)
    for provider in ('google', 'microsoft'):
        cli('config', 'calendar-oauth', '--provider', provider,
            payload={'client_id': client_id, 'client_secret': secret})
    dump = cli('config', 'dump')
    assert secret not in dump, 'Config dump disclosed the client secret'
    db = sqlite3.connect(directory / 'calrs.db')
    encrypted = db.execute('SELECT microsoft_oauth2_client_secret FROM auth_config').fetchone()[0]
    assert encrypted != secret and secret not in encrypted, 'Client secret was stored in clear'

    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    with open(directory / 'server.log', 'w+') as log:
        process = subprocess.Popen(
            [binary, '--data-dir', temporary, 'serve', '--port', str(port)],
            env=env, stdout=log, stderr=log,
        )

        def request(method, path, cookies, fields=None):
            headers = {'Cookie': '; '.join(f'{key}={value}' for key, value in cookies.items())}
            body = None
            if fields is not None:
                headers['Content-Type'] = 'application/x-www-form-urlencoded'
                body = urlencode(fields)
            conn = http.client.HTTPConnection('127.0.0.1', port, timeout=10)
            try:
                conn.request(method, path, body, headers)
                response = conn.getresponse()
                text = response.read().decode()
                for key, value in response.getheaders():
                    if key.lower() == 'set-cookie':
                        cookies.update({name: item.value for name, item in SimpleCookie(value).items()})
                return response.status, dict(response.getheaders()), text
            finally:
                conn.close()

        def register(name):
            cookies = {}
            assert request('GET', '/auth/register', cookies)[0] == 200
            status, _, body = request('POST', '/auth/register', cookies, {
                '_csrf': cookies['__Host-calrs_csrf'], 'name': name,
                'email': f'{name}@example.test', 'password': secrets.token_urlsafe(24),
            })
            assert status == 303, (status, body)
            assert '__Host-calrs_session' in cookies
            return cookies

        try:
            for _ in range(100):
                try:
                    request('GET', '/auth/login', {})
                    break
                except (ConnectionRefusedError, ConnectionResetError):
                    assert process.poll() is None, 'Server exited during startup'
                    time.sleep(.1)
            else:
                raise RuntimeError('Server did not start')

            alice, bob = register('alice'), register('bob')
            assert request('GET', '/dashboard/sources/new', alice)[0] == 200
            assert request('POST', '/dashboard/sources/microsoft/connect', alice, {})[0] == 403
            assert db.execute('SELECT count(*) FROM microsoft_oauth_states').fetchone()[0] == 0
            status, headers, _ = request('POST', '/dashboard/sources/microsoft/connect', alice,
                                        {'_csrf': alice['__Host-calrs_csrf']})
            assert status == 303, status
            target = urlsplit(headers['location'])
            query = parse_qs(target.query)
            assert (target.scheme, target.netloc, target.path) == (
                'https', 'login.microsoftonline.com', '/common/oauth2/v2.0/authorize')
            assert query['redirect_uri'] == ['https://calendar.example.test/dashboard/sources/microsoft/callback']
            assert query['code_challenge_method'] == ['S256']
            assert set(query['scope'][0].split()) == {
                'https://graph.microsoft.com/Calendars.ReadWrite',
                'https://graph.microsoft.com/User.Read', 'offline_access',
            }
            state = query['state'][0]
            stored_hash, verifier = db.execute('SELECT state_hash,verifier_enc FROM microsoft_oauth_states').fetchone()
            assert stored_hash == hashlib.sha256(state.encode()).hexdigest()
            assert state not in verifier
            denied = '/dashboard/sources/microsoft/callback?' + urlencode({'state': state, 'error': 'access_denied'})
            # A second account cannot consume the first account's pending consent.
            assert request('GET', denied, bob)[0] == 400
            assert db.execute('SELECT count(*) FROM microsoft_oauth_states').fetchone()[0] == 1
            # A localized denial consumes state exactly once, without sending any token request.
            db.execute("UPDATE users SET language='fr' WHERE email='alice@example.test'")
            db.commit()
            status, headers, body = request('GET', denied, alice)
            assert status == 400 and 'autorisation' in body
            assert headers['cache-control'] == 'no-store'
            assert headers['referrer-policy'] == 'no-referrer'
            assert db.execute('SELECT count(*) FROM microsoft_oauth_states').fetchone()[0] == 0, body
            assert request('GET', denied, alice)[0] == 400
            assert db.execute('SELECT count(*) FROM caldav_sources').fetchone()[0] == 0
            # Request tracing must not retain callback credentials at trace level.
            probe = 'synthetic-code-' + secrets.token_hex(16)
            callback = '/dashboard/sources/microsoft/callback?' + urlencode({'state': 'invalid', 'code': probe})
            assert request('GET', callback, alice)[0] == 400
            print('PASS: CLI encryption/dump, real registration, CSRF, PKCE/scopes, account-bound state, localized denial and replay rejection')
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            db.close()
        log.seek(0)
        trace = log.read()
        assert probe not in trace and state not in trace and secret not in trace, 'OAuth credentials leaked to request tracing'
        print('PASS: callback query and client secret absent from trace-level logs; no external calendar was contacted')
