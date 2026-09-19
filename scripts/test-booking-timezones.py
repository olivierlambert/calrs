#!/usr/bin/env python3
"""Run real HTTP and CLI booking checks in isolated UTC/Berlin server processes.
Run after cargo build: python3 scripts/test-booking-timezones.py
Only the Python standard library is required. No real calendars or email used.
"""
import datetime as dt
import http.client
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import tempfile
import time
from urllib.parse import urlencode
from zoneinfo import ZoneInfo

binary = str(Path(os.environ.get('CALRS_BINARY', 'target/debug/calrs')).resolve())
root = Path(tempfile.mkdtemp(prefix='calrs-timezones-'))
results = []
day = dt.date.today() + dt.timedelta(days=10)
while day.weekday() != 0:
    day += dt.timedelta(days=1)

for server_zone in ('UTC', 'Europe/Berlin'):
    directory = root / server_zone.replace('/', '-')
    directory.mkdir()
    # Do not inherit real SMTP/SMS/provider configuration into the test server.
    env = {k: v for k, v in os.environ.items() if not k.startswith('CALRS_')}
    env['TZ'] = server_zone
    # Let the binary initialize a fresh database and run its normal migrations.
    subprocess.run([binary, '--data-dir', str(directory), 'user', 'list'], env=env,
                   check=True, stdout=subprocess.DEVNULL)
    db = sqlite3.connect(directory / 'calrs.db')
    db.executescript("""
        INSERT INTO users(id,email,name,username,role,timezone) VALUES ('u','host@test.example','Host','host','admin','Europe/Paris');
        INSERT INTO accounts(id,name,email,user_id) VALUES ('a','Host','host@test.example','u');
        INSERT INTO event_types(id,account_id,slug,title,duration_min,min_notice_min,timezone) VALUES ('e','a','meeting','Timezone meeting',30,0,'Europe/Paris');
        INSERT INTO sessions(id,user_id,expires_at) VALUES ('functional','u',datetime('now','+1 day'));
        INSERT INTO availability_rules(id,event_type_id,day_of_week,start_time,end_time) VALUES ('r','e',1,'09:00','17:00');
        INSERT INTO bookings(id,event_type_id,uid,guest_name,guest_email,guest_timezone,start_at,end_at,cancel_token,reschedule_token)
        VALUES ('legacy','e','legacy','Legacy','old@test.example','Europe/Paris','2026-01-01T10:00:00','2026-01-01T10:30:00','old-cancel','old-reschedule');
    """)
    db.commit()
    legacy = db.execute("SELECT * FROM bookings WHERE id='legacy'").fetchone()
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    log = open(directory / 'server.log', 'w')
    process = subprocess.Popen([binary, '--data-dir', str(directory), 'serve', '--port', str(port)],
                               env=env, stdout=log, stderr=log)

    def request(method, path, fields=None, admin=False):
        headers = {'Cookie': '__Host-calrs_csrf=functional' + ('; __Host-calrs_session=functional' if admin else '')}
        body = None
        if fields is not None:
            body = urlencode(dict(fields, _csrf='functional'))
            headers['Content-Type'] = 'application/x-www-form-urlencoded'
        conn = http.client.HTTPConnection('127.0.0.1', port, timeout=10)
        conn.request(method, path, body, headers)
        response = conn.getresponse()
        text = response.read().decode()
        status = response.status
        conn.close()
        assert status < 500, (status, text)
        return status, text

    try:
        for _ in range(100):
            try:
                request('GET', '/auth/login')
                break
            except (ConnectionRefusedError, ConnectionResetError):
                assert process.poll() is None, (directory / 'server.log').read_text()
                time.sleep(.1)
        else:
            raise RuntimeError('Server did not start')
        request('POST', '/u/host/meeting/book', dict(date=str(day), time='10:00', tz='Europe/Paris', name='Guest', email='guest@test.example'))
        row = db.execute("SELECT start_at,end_at,time_version,cancel_token,reschedule_token FROM bookings WHERE guest_email='guest@test.example'").fetchone()
        assert row, 'HTTP booking was not created'
        start, end, version, cancel, reschedule = row
        expected = dt.datetime.combine(day, dt.time(10), ZoneInfo('Europe/Paris')).astimezone(dt.timezone.utc)
        assert start == expected.strftime('%Y-%m-%dT%H:%M:%SZ'), row
        assert version == 1
        assert '10:00 AM' in request('GET', '/dashboard/bookings', admin=True)[1]
        ics = request('GET', '/booking/ics/' + cancel)[1]
        assert 'DTSTART:' + expected.strftime('%Y%m%dT%H%M%SZ') in ics
        # A later event-zone change must not move an existing UTC booking.
        db.execute("UPDATE event_types SET timezone='Pacific/Honolulu' WHERE id='e'")
        db.commit()
        assert '10:00' in request('GET', '/booking/cancel/' + cancel)[1]
        assert request('GET', '/booking/ics/' + cancel)[1].split('DTSTART:')[1].splitlines()[0] == expected.strftime('%Y%m%dT%H%M%SZ')
        db.execute("UPDATE event_types SET timezone='Europe/Paris' WHERE id='e'")
        db.commit()
        request('POST', '/booking/reschedule/' + reschedule, dict(date=str(day), time='11:00', tz='Europe/Paris'))
        moved = db.execute("SELECT start_at,time_version,cancel_token FROM bookings WHERE guest_email='guest@test.example'").fetchone()
        assert moved[:2] == ((expected + dt.timedelta(hours=1)).strftime('%Y-%m-%dT%H:%M:%SZ'), 1), moved
        request('POST', '/booking/cancel/' + moved[2], {})
        assert db.execute("SELECT status FROM bookings WHERE guest_email='guest@test.example'").fetchone()[0] == 'cancelled'
        subprocess.run([binary, '--data-dir', str(directory), 'booking', 'create', 'meeting',
                        '--date', str(day), '--time', '12:00', '--timezone', 'Europe/Paris',
                        '--name', 'CLI Guest', '--email', 'cli@test.example'],
                       check=True, env=env, stdout=subprocess.DEVNULL)
        cli = db.execute("SELECT start_at,time_version FROM bookings WHERE guest_email='cli@test.example'").fetchone()
        assert cli == ((expected + dt.timedelta(hours=2)).strftime('%Y-%m-%dT%H:%M:%SZ'), 1), cli
        # Exercise both directions of overlap across the autumn clock rollback.
        fold = dt.date(dt.date.today().year, 10, 31)
        while fold.weekday() != 6:
            fold -= dt.timedelta(days=1)
        if fold <= dt.date.today():
            fold = dt.date(fold.year + 1, 10, 31)
            while fold.weekday() != 6:
                fold -= dt.timedelta(days=1)
        db.execute("INSERT INTO availability_rules(id,event_type_id,day_of_week,start_time,end_time) VALUES ('fold','e',0,'00:00','06:00')")
        db.execute("INSERT INTO bookings(id,event_type_id,uid,guest_name,guest_email,guest_timezone,start_at,end_at,time_version,cancel_token,reschedule_token) VALUES ('fold-existing','e','fold-existing','Existing','existing@test.example','UTC',?,?,1,'fold-cancel','fold-reschedule')",
                   (str(fold) + 'T00:50:00Z', str(fold) + 'T01:00:00Z'))
        db.commit()
        fields = dict(date=str(fold), time='00:45', tz='UTC', name='Fold', email='fold@test.example')
        request('POST', '/u/host/meeting/book', fields)
        assert db.execute("SELECT count(*) FROM bookings WHERE guest_email='fold@test.example'").fetchone()[0] == 0, 'Cross-fold proposal ignored existing conflict'
        db.execute("DELETE FROM bookings WHERE id='fold-existing'")
        db.commit()
        fold_response = request('POST', '/u/host/meeting/book', fields)
        folded = db.execute("SELECT start_at,end_at FROM bookings WHERE guest_email='fold@test.example'").fetchone()
        assert folded == (str(fold) + 'T00:45:00Z', str(fold) + 'T01:15:00Z'), (folded, fold_response)
        request('POST', '/u/host/meeting/book', dict(fields, time='01:05', email='overlap@test.example'))
        assert db.execute("SELECT count(*) FROM bookings WHERE guest_email='overlap@test.example'").fetchone()[0] == 0, 'Stored cross-fold booking disappeared from conflicts'
        assert db.execute("SELECT * FROM bookings WHERE id='legacy'").fetchone() == legacy
        results.append((start, end, version, moved[:2], cli))
        print('PASS:', server_zone, 'HTTP create/display/ICS/reschedule/cancel, CLI creation, DST overlap rejection, legacy unchanged')
    finally:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        db.close()
        log.close()
assert results[0] == results[1], results
print('PASS: identical results under UTC and Europe/Berlin')
print('Isolated data and server logs:', root)
