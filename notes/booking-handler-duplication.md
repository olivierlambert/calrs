# Booking handler duplication — anti-pattern & test ideas

Discovered while extracting the lead-capture layer from `src/web/mod.rs`. The
lead-capture feature touched four near-identical handlers, which made the
duplication impossible to ignore. This doc captures the shape so a future
refactor doesn't get derailed by re-discovery.

## Symptom

`BookForm` is the request body for every guest-facing booking POST. It is
declared once in `src/web/mod.rs` and consumed by four handlers:

| Handler                          | Approx. range            | Lines |
|----------------------------------|--------------------------|-------|
| `handle_group_booking`           | `mod.rs:9391-9834`       | ~444  |
| `handle_dynamic_group_booking`   | `mod.rs:10252-10636`     | ~385  |
| `handle_booking_for_user`        | `mod.rs:11031-11469`     | ~439  |
| `handle_booking` (legacy single) | `mod.rs:13223-13622`     | ~400  |

Total: **~1 670 lines for what is structurally one workflow.** Each handler
implements the same steps in the same order:

1. `verify_csrf_token`
2. Captcha verify
3. Rate-limit by client IP
4. `validate_booking_input` (name / email / notes)
5. Resolve event type (different SELECT per route, but the same shape:
   `(et_id, account_id, et_title, duration, buffers, requires_confirmation,
   reminder, location, max_additional_guests, …)`)
6. `validate_phone_input` (added by lead-capture)
7. Parse additional guests
8. Parse date/time, validate not-too-far
9. Resolve guest TZ + host TZ
10. Frequency limit check
11. `BEGIN IMMEDIATE` transaction
12. `INSERT INTO bookings (…, guest_phone)`
13. Insert additional attendees
14. Resolve booking location (Jitsi / webhook auto-gen)
15. `caldav_push_booking`
16. `crate::email::send_*` (confirmation / pending / host notification)
17. Watcher team notifications
18. `crate::leads::mark_completed(lead_id)` (added by lead-capture)
19. Redirect to `/confirmed/{uid}`

## Why this hurts

- Adding a single field to the booking flow (e.g. `guest_phone`) is a
  **4-way edit** with high risk of one handler drifting from the others.
  That happened during phase 2: the `mark_completed` call site and the
  `validate_phone_input` injection had to be replicated four times. Each
  one is a chance for a divergence to slip in.
- Each handler has its own `tx.rollback()` paths, error responses, and
  rate-limit messages. Sticking with the convention requires reading the
  three other handlers to know what error string to use.
- Tests today cover individual scenarios per handler, not the cross-handler
  invariants. A behaviour change in handler #1 can silently leave handlers
  #2/#3/#4 inconsistent until a customer hits the broken path.

## Why it has not been refactored yet

- The handlers diverge in *event-type resolution*: the SELECT joins
  differ between single-user, team-slug, dynamic-group, and legacy paths.
  A shared helper has to take a function or trait for the lookup.
- They diverge in *post-booking side effects*: dynamic groups assign a
  member at booking time, teams notify watchers, etc.
- They diverge in *templates rendered on error*: `handle_booking` uses the
  legacy single-host error page; the others use `render_booking_action_error`.

These are real differences, but they are localised. The body of each
handler is ~400 lines, of which roughly 300 are identical.

## Proposed shape (not implemented)

```
struct BookingFlow<'a> {
    state:       &'a AppState,
    form:        BookForm,
    headers:     HeaderMap,
    et_resolver: Box<dyn EventTypeResolver>, // single / team / dynamic
    response:    BookingResponseKind,        // legacy_html / action_error
}

impl BookingFlow {
    async fn run(self) -> Response { /* steps 1-19 above */ }
}
```

Each existing handler becomes a thin shell:

```rust
async fn handle_booking(state, headers, Path(slug), Form(form)) -> Response {
    BookingFlow {
        state: &state,
        form,
        headers,
        et_resolver: Box::new(SingleHostResolver { slug }),
        response: BookingResponseKind::LegacyHtml,
    }
    .run()
    .await
}
```

Estimated outcome: ~1 670 lines → ~600 lines for the flow + 4×~30 lines per
handler. About **-900 lines net**, plus all future field additions are
single-edit instead of 4-edit.

## Test ideas a refactor would unlock

A unified `BookingFlow` makes it cheap to write **cross-route invariants**
that today have to be hand-rolled per handler:

1. **`mark_completed` always runs when `lead_id` is present** — currently
   tested for `handle_booking` only. Refactor would test once on the flow.
2. **`validate_phone_input` always runs before INSERT** — phase 2's main
   risk was forgetting one of the 4 sites. A `tx.rollback()` on error
   would be assertable from a single test.
3. **CalDAV write-back is skipped when `needs_approval == true`** —
   replicated logic in 4 handlers, currently tested in only one.
4. **Watcher notifications fire for team event types regardless of route**
   — a single guest can reach a team event via `/team/x/y` (group) or
   `/u/host+host2/y` (dynamic group); both must notify watchers. Easy to
   forget in one handler.
5. **Booking confirmation email contains `guest_phone` row when the
   event type has `collect_phone>0`** — the email composition is shared,
   but the path from BookForm to BookingDetails is duplicated.
6. **Rate limit applies per IP regardless of route** — today the limiter
   key is computed independently per handler. Cross-handler test would
   confirm.
7. **`additional_guests` cap is enforced uniformly** — `max=0` event types
   reject any additional guest; `max=3` accepts up to 3. The parsing
   logic is identical across handlers, but a test per handler is needed
   today.

Tests that should exist either way (not blocked by refactor, but easier
once the flow is unified):

8. **`COALESCE`-preserve regression** for lead-capture: an upsert with
   all fields blank must not wipe prior values. Already covered by
   `upsert_preserves_captured_fields_when_blanked` in `src/leads/db.rs`,
   but a higher-level test that drives the public `/api/lead-capture`
   endpoint (rather than the DB helper) would catch JSON binding
   regressions.
9. **Required-phone gate on every booking route**: today only
   `handle_booking` is tested with `collect_phone=2`; the other three
   routes share the same code path but lack explicit coverage.

## Out of scope for this branch

The duplication predates lead-capture; phase 2 only made it more visible
by adding two cross-cutting touch points (`validate_phone_input`,
`mark_completed`). The refactor is large (~1 day of careful work + tests)
and should land in its own PR with no behaviour changes — pure structural
move with the test suite as the safety net.
