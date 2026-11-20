# Embedding Booking Pages

Open **Embed** on a personal or team event type to generate a snippet for your website. The generator previews three ways to offer booking:

- **Inline**: a booking picker inside your page, with automatic height adjustment.
- **Floating button**: a button that opens the picker in a modal.
- **Element click**: attach the modal to an existing link or button.

Use your public calrs origin in the script URL and the full booking URL as `link`. Keep the script before the code that calls `Calrs`.

## Inline example

```html
<div id="booking"></div>
<script src="https://cal.example.com/embed.js"></script>
<script>
  Calrs.inline({
    selector: '#booking',
    link: 'https://cal.example.com/u/alice/intro',
    config: { layout: 'month', theme: 'auto' }
  });
</script>
```

The runtime adds `embed=1` and handles resizing messages from that iframe's window and origin.

## Floating button example

```html
<script src="https://cal.example.com/embed.js"></script>
<script>
  Calrs.floatingButton({
    link: 'https://cal.example.com/u/alice/intro',
    buttonText: 'Book a meeting',
    config: { theme: 'light', brand: '#2563eb' }
  });
</script>
```

Use the generator to choose button position, colors and icon settings.

## Existing link or button

```html
<a href="https://cal.example.com/u/alice/intro"
   data-calrs-link="https://cal.example.com/u/alice/intro"
   data-calrs-config='{"layout":"week","theme":"auto"}'>
  Book a meeting
</a>
<script src="https://cal.example.com/embed.js"></script>
```

The runtime automatically binds elements with `data-calrs-link`. The ordinary `href` also provides a destination when JavaScript is unavailable.

## Display options

| Option | Values | Effect |
|---|---|---|
| `layout` | `month`, `week`, `column` | Initial calendar view |
| `theme` | `auto`, `light`, `dark` | Color scheme |
| `brand` | Hex color | Accent color |
| `timeFormat` | `"24"`, `"12"` | Fix the clock format and hide the format switch |
| `fixedTimezone` | `true` | Use the event type's timezone and replace the timezone picker with a note |

Leave the last two options out to let visitors use the usual clock and timezone controls. Fixed timezone is useful for in-person appointments: it follows the event type's current timezone, rather than a hard-coded offset. These controls apply inside embeds only; a guest rescheduling from an email opens the regular reschedule page.

## Access and booking rules

An embed uses the same availability, date range, questions, captcha and booking rules as the direct link. Embedding does not make a private or internal event type public. If a booking URL needs an invitation, preserve its query parameters and remember that the invitation's expiry and usage limit still apply.

## Campaign tracking

The runtime passes the hosting page's `utm_source`, `utm_medium`, `utm_campaign`, `utm_term` and `utm_content` parameters on to the booking page, so a visitor who reaches `https://example.com/contact?utm_source=newsletter` books with that attribution. A parameter already present in `link` wins. See [Campaign tracking](./booking-flow.md#campaign-tracking-utm).

Set the public base URL correctly before generating snippets. If your website has a Content Security Policy, allow your calrs origin for the script and frame. If the picker does not load, check the browser console and any framing policy added by your reverse proxy.
