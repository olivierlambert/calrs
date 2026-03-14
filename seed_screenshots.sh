#!/usr/bin/env bash
#
# Seed a fresh calrs instance with realistic data for taking screenshots.
# Usage:
#   cargo build --release
#   ./seed_screenshots.sh
#   ./target/release/calrs serve --port 3000 --data-dir /tmp/calrs-screenshots
#
# Then open http://localhost:3000 and take screenshots.
# Login: alice@example.com / password123
#
set -euo pipefail

CALRS="./target/release/calrs"
DATA_DIR="/tmp/calrs-screenshots"
DB="$DATA_DIR/calrs.db"
PORT=39876

mkdir -p "$DATA_DIR"
rm -f "$DB"

# Helper: run SQL via python3 (no sqlite3 CLI needed)
sql() { python3 -c "import sqlite3,sys; c=sqlite3.connect('$DB'); c.execute(sys.argv[1]); c.commit()" "$1"; }
sql_val() { python3 -c "import sqlite3,sys; c=sqlite3.connect('$DB'); print(c.execute(sys.argv[1]).fetchone()[0])" "$1"; }

echo "=== Seeding calrs for screenshots ==="

# ── Start server, register users via HTTP, then stop ──────────────────
echo "[1/7] Registering users..."
$CALRS serve --port $PORT --data-dir "$DATA_DIR" &
SERVE_PID=$!
sleep 2

BASE="http://localhost:$PORT"

COOKIE_JAR="/tmp/calrs-seed-cookies.txt"
register_user() {
  local name="$1" email="$2" password="$3"
  rm -f "$COOKIE_JAR"
  curl -s -c "$COOKIE_JAR" -b "$COOKIE_JAR" "$BASE/auth/register" > /dev/null
  local csrf
  csrf=$(grep calrs_csrf "$COOKIE_JAR" | tail -1 | awk '{print $NF}')
  local encoded_name
  encoded_name=$(python3 -c "import urllib.parse; print(urllib.parse.quote('$name'))")
  curl -s -o /dev/null -c "$COOKIE_JAR" -b "$COOKIE_JAR" \
    -d "_csrf=$csrf&name=$encoded_name&email=$email&password=$password" \
    "$BASE/auth/register"
}

register_user "Alice Martin" "alice@example.com" "password123"
register_user "Bob Chen" "bob@example.com" "password123"
register_user "Carol Davis" "carol@example.com" "password123"

kill $SERVE_PID 2>/dev/null || true
wait $SERVE_PID 2>/dev/null || true

# ── Set usernames and profiles ────────────────────────────────────────
echo "[2/7] Setting up profiles..."
sql "UPDATE users SET username = 'alice', title = 'Engineering Lead', bio = 'Building great products. Based in Paris.' WHERE email = 'alice@example.com'"
sql "UPDATE users SET username = 'bob', title = 'Product Designer' WHERE email = 'bob@example.com'"
sql "UPDATE users SET username = 'carol', title = 'Sales Manager' WHERE email = 'carol@example.com'"

# ── Get IDs ───────────────────────────────────────────────────────────
ALICE_ID=$(sql_val "SELECT id FROM users WHERE email = 'alice@example.com'")
BOB_ID=$(sql_val "SELECT id FROM users WHERE email = 'bob@example.com'")
CAROL_ID=$(sql_val "SELECT id FROM users WHERE email = 'carol@example.com'")
ALICE_ACCT=$(sql_val "SELECT id FROM accounts WHERE user_id = '$ALICE_ID'")
BOB_ACCT=$(sql_val "SELECT id FROM accounts WHERE user_id = '$BOB_ID'")

# ── Seed all data via a single python3 script ─────────────────────────
echo "[3/7] Creating event types, bookings, team links..."

python3 << PYEOF
import sqlite3, uuid, datetime

db = sqlite3.connect("$DB")
c = db.cursor()

def uid(): return str(uuid.uuid4())
today = datetime.date.today()
def future(days): return (today + datetime.timedelta(days=days)).isoformat()

ALICE_ID = "$ALICE_ID"
BOB_ID = "$BOB_ID"
CAROL_ID = "$CAROL_ID"
ALICE_ACCT = "$ALICE_ACCT"
BOB_ACCT = "$BOB_ACCT"

# ── Event types ───────────────────────────────────────────────────────
et1 = uid()  # 30-min intro
et2 = uid()  # 60-min deep dive (requires confirmation)
et3 = uid()  # 15-min quick chat
et4 = uid()  # VIP demo (private)
et5 = uid()  # disabled
et_bob = uid()

c.execute("INSERT INTO event_types (id, account_id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type, location_value, requires_confirmation, reminder_minutes) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    (et1, ALICE_ACCT, "intro", "30-Minute Intro Call", "A quick intro call to discuss your needs and see if we are a good fit.", 30, 5, 5, 60, 1, "link", "https://meet.example.com/alice", 0, 60))

c.execute("INSERT INTO event_types (id, account_id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type, location_value, requires_confirmation, reminder_minutes, max_additional_guests) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    (et2, ALICE_ACCT, "deep-dive", "60-Minute Deep Dive", "In-depth technical discussion. Please share context in the notes.", 60, 10, 10, 120, 1, "link", "https://meet.example.com/alice-dd", 1, 1440, 3))

c.execute("INSERT INTO event_types (id, account_id, slug, title, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type, location_value) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
    (et3, ALICE_ACCT, "quick-chat", "15-Minute Quick Chat", 15, 0, 5, 30, 1, "phone", "+33 6 12 34 56 78"))

c.execute("INSERT INTO event_types (id, account_id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type, location_value, is_private, requires_confirmation) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    (et4, ALICE_ACCT, "vip-demo", "VIP Product Demo", "Exclusive demo for selected prospects.", 45, 10, 10, 240, 1, "link", "https://meet.example.com/vip", 1, 1))

c.execute("INSERT INTO event_types (id, account_id, slug, title, duration_min, enabled) VALUES (?,?,?,?,?,?)",
    (et5, ALICE_ACCT, "old-meeting", "Old Meeting Type", 30, 0))

c.execute("INSERT INTO event_types (id, account_id, slug, title, description, duration_min, buffer_before, buffer_after, min_notice_min, enabled, location_type, location_value) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    (et_bob, BOB_ACCT, "design-review", "Design Review", "Review your designs and get feedback.", 45, 5, 5, 60, 1, "link", "https://meet.example.com/bob"))

# ── Availability rules (Mon-Fri 09:00-12:00, 14:00-18:00) ────────────
for et in [et1, et2, et3, et4, et5, et_bob]:
    for day in [1, 2, 3, 4, 5]:
        c.execute("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?,?,?,?,?)",
            (uid(), et, day, "09:00", "12:00"))
        c.execute("INSERT INTO availability_rules (id, event_type_id, day_of_week, start_time, end_time) VALUES (?,?,?,?,?)",
            (uid(), et, day, "14:00", "18:00"))

# ── Confirmed bookings ───────────────────────────────────────────────
guests = [
    (1, "David Park",    "david@startup.io",   "10:00", "10:30", et1),
    (2, "Emma Wilson",   "emma@design.co",      "14:00", "15:00", et2),
    (3, "Frank Mueller", "frank@company.de",   "11:00", "11:30", et1),
    (5, "Grace Kim",     "grace@agency.kr",     "09:00", "09:15", et3),
    (7, "Hiro Tanaka",   "hiro@tech.jp",        "16:00", "16:30", et1),
]
for days_ahead, name, email, t_start, t_end, et in guests:
    d = future(days_ahead)
    bid = uid()
    c.execute("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token) VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        (bid, et, f"{bid}@calrs", name, email, "UTC", f"{d}T{t_start}:00", f"{d}T{t_end}:00", "confirmed", uid(), uid()))

# ── Pending bookings ─────────────────────────────────────────────────
for days_ahead, name, email in [(2, "Ines Garcia", "ines@consulting.es"), (4, "Jake Thompson", "jake@venture.vc")]:
    d = future(days_ahead)
    bid = uid()
    c.execute("INSERT INTO bookings (id, event_type_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token, reschedule_token, confirm_token, notes) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
        (bid, et2, f"{bid}@calrs", name, email, "Europe/Paris", f"{d}T15:00:00", f"{d}T16:00:00", "pending", uid(), uid(), uid(), "Would love to discuss the enterprise plan."))

# ── Team links ────────────────────────────────────────────────────────
tl1 = uid()
tl1_token = uid()
c.execute("INSERT INTO team_links (id, token, title, duration_min, created_by_user_id, description, location_type, location_value) VALUES (?,?,?,?,?,?,?,?)",
    (tl1, tl1_token, "Product Sync", 30, ALICE_ID, "Weekly product sync with the team", "link", "https://meet.example.com/product"))

c.execute("INSERT INTO team_link_members (id, team_link_id, user_id) VALUES (?,?,?)", (uid(), tl1, ALICE_ID))
c.execute("INSERT INTO team_link_members (id, team_link_id, user_id) VALUES (?,?,?)", (uid(), tl1, BOB_ID))

tl2 = uid()
tl2_token = uid()
c.execute("INSERT INTO team_links (id, token, title, duration_min, created_by_user_id, description, one_time_use) VALUES (?,?,?,?,?,?,?)",
    (tl2, tl2_token, "Hiring Interview", 60, ALICE_ID, "Technical interview — all panelists must be free", 1))

c.execute("INSERT INTO team_link_members (id, team_link_id, user_id) VALUES (?,?,?)", (uid(), tl2, ALICE_ID))
c.execute("INSERT INTO team_link_members (id, team_link_id, user_id) VALUES (?,?,?)", (uid(), tl2, BOB_ID))
c.execute("INSERT INTO team_link_members (id, team_link_id, user_id) VALUES (?,?,?)", (uid(), tl2, CAROL_ID))

# Team link booking
d3 = future(3)
tlb = uid()
c.execute("INSERT INTO team_link_bookings (id, team_link_id, uid, guest_name, guest_email, guest_timezone, start_at, end_at, status, cancel_token) VALUES (?,?,?,?,?,?,?,?,?,?)",
    (tlb, tl1, f"{tlb}@calrs", "Lisa Wang", "lisa@partner.com", "Asia/Shanghai", f"{d3}T10:00:00", f"{d3}T10:30:00", "confirmed", uid()))

# ── Invite for private event type ─────────────────────────────────────
c.execute("INSERT INTO booking_invites (id, event_type_id, token, guest_name, guest_email, message, max_uses, used_count, created_by_user_id) VALUES (?,?,?,?,?,?,?,?,?)",
    (uid(), et4, uid(), "Sam Rivera", "sam@enterprise.com", "Hi Sam, here is your exclusive demo link!", 1, 0, ALICE_ID))

# ── Fake CalDAV sources ──────────────────────────────────────────────
src1 = uid()
src2 = uid()
c.execute("INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, enabled, last_synced, write_calendar_href) VALUES (?,?,?,?,?,?,?,datetime('now', '-2 minutes'),?)",
    (src1, ALICE_ACCT, "Nextcloud", "https://cloud.example.com/remote.php/dav", "alice", "0000", 1, "/remote.php/dav/calendars/alice/personal/"))

c.execute("INSERT INTO caldav_sources (id, account_id, name, url, username, password_enc, enabled, last_synced) VALUES (?,?,?,?,?,?,?,datetime('now', '-1 hour'))",
    (src2, ALICE_ACCT, "Google Calendar", "https://apidata.googleusercontent.com/caldav/v2/alice@gmail.com/", "alice@gmail.com", "0000", 1))

# Calendars
cal1 = uid()
cal2 = uid()
cal3 = uid()
c.execute("INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?,?,?,?,?)", (cal1, src1, "/calendars/alice/personal/", "Personal", 1))
c.execute("INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?,?,?,?,?)", (cal2, src1, "/calendars/alice/work/", "Work", 1))
c.execute("INSERT INTO calendars (id, source_id, href, display_name, is_busy) VALUES (?,?,?,?,?)", (cal3, src2, "/calendars/alice@gmail.com/events/", "Google Events", 0))

# ── Calendar events (busy times for troubleshoot) ─────────────────────
for i in range(1, 6):
    d = future(i)
    eid = uid()
    c.execute("INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, raw_ical, etag, all_day, status) VALUES (?,?,?,?,?,?,?,?,?,?)",
        (eid, cal2, f"{eid}@nextcloud", "Team Standup", f"{d}T09:30:00", f"{d}T10:00:00", "BEGIN:VCALENDAR\r\nEND:VCALENDAR", f"etag-{i}", 0, "CONFIRMED"))

d2 = future(2)
c.execute("INSERT INTO events (id, calendar_id, uid, summary, start_at, end_at, raw_ical, etag, all_day, status) VALUES (?,?,?,?,?,?,?,?,?,?)",
    (uid(), cal1, f"{uid()}@nextcloud", "Lunch with Sarah", f"{d2}T12:00:00", f"{d2}T13:30:00", "BEGIN:VCALENDAR\r\nEND:VCALENDAR", "etag-lunch", 0, "CONFIRMED"))

# Print event type ID for troubleshoot URL
print(f"ET1_ID={et1}")

db.commit()
db.close()
PYEOF

echo "[4/7] Done seeding data."
echo ""
echo "=== Ready! ==="
echo ""
echo "Login credentials:"
echo "  alice@example.com / password123  (admin)"
echo "  bob@example.com   / password123"
echo "  carol@example.com / password123"
echo ""
echo "Start the server:"
echo "  $CALRS serve --port 3000 --data-dir $DATA_DIR"
echo ""
echo "Then visit http://localhost:3000 and take screenshots."
echo ""
echo "Pages to screenshot:"
echo "  Dashboard overview     http://localhost:3000/dashboard"
echo "  Event types            http://localhost:3000/dashboard/event-types"
echo "  Bookings               http://localhost:3000/dashboard/bookings"
echo "  Calendar sources       http://localhost:3000/dashboard/sources"
echo "  Team links             http://localhost:3000/dashboard/team-links"
echo "  Admin panel            http://localhost:3000/dashboard/admin"
echo "  Profile & Settings     http://localhost:3000/dashboard/settings"
echo "  Public profile         http://localhost:3000/u/alice"
echo "  Slot picker            http://localhost:3000/u/alice/intro"
echo "  Login page             http://localhost:3000/auth/login (sign out first)"
echo "  Reschedule             Click 'Reschedule' on a booking in dashboard"
echo "  Light mode             Toggle theme in settings, retake key pages"
echo "  Mobile                 Resize browser to 375px width"
