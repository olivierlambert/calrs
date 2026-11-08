# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Buchung ausstehend
confirmed-page-title-booked = Buchung bestätigt

confirmed-heading-reschedule-requested = Verschiebung angefordert
confirmed-heading-rescheduled = Verschoben!
confirmed-heading-pending = Bestätigung ausstehend
confirmed-heading-booked = Termin gebucht!

confirmed-subtitle-reschedule-requested = Deine Anfrage zur Terminverschiebung wurde an { $host } gesendet. Du erhältst eine E-Mail an { $email }, sobald sie genehmigt ist.
confirmed-subtitle-rescheduled = Deine Buchung wurde verschoben. Eine Bestätigungs-E-Mail wurde an { $email } gesendet.
confirmed-subtitle-pending = Deine Buchungsanfrage wurde an { $host } gesendet. Du erhältst eine E-Mail an { $email }, sobald sie bestätigt ist.
confirmed-subtitle-booked = Eine Bestätigungs-E-Mail wurde an { $email } gesendet.

confirmed-detail-event = Termin:
confirmed-detail-date = Datum:
confirmed-detail-time = Uhrzeit:
confirmed-detail-with = Mit:
confirmed-detail-location = Ort:
confirmed-detail-notes = Notizen:
confirmed-detail-additional-guests = Weitere Teilnehmer:

confirmed-book-another = Weiteren Termin buchen

confirmed-add-to-calendar = Zum Kalender hinzufügen

# Slot picker (templates/slots.html)

slots-location-video = Videoanruf
slots-location-phone = Telefonanruf
slots-location-google-meet = Google Meet

slots-tz-label = Deine Zeitzone
slots-tz-fixed = Zeiten werden in { $tz } angezeigt
slots-time-format-label = Zeitformat
tz-group-common = Häufige Zeitzonen
tz-region-africa = Afrika
tz-region-america = Amerika
tz-region-antarctica = Antarktis
tz-region-arctic = Arktis
tz-region-asia = Asien
tz-region-atlantic = Atlantik
tz-region-australia = Australien
tz-region-europe = Europa
tz-region-indian = Indischer Ozean
tz-region-pacific = Pazifik
slots-offset-note-day = Die Zeiten am %1 sind in %2 (Zeitumstellung am %3).
slots-offset-note-range = Zeitumstellung am %1: ab dann sind die Zeiten in %2.

slots-view-month = Monatsansicht
slots-view-week = Wochenansicht
slots-view-column = Listenansicht

slots-weekday-mon = Mo
slots-weekday-tue = Di
slots-weekday-wed = Mi
slots-weekday-thu = Do
slots-weekday-fri = Fr
slots-weekday-sat = Sa
slots-weekday-sun = So

slots-weekday-mon-short = M
slots-weekday-tue-short = D
slots-weekday-wed-short = M
slots-weekday-thu-short = D
slots-weekday-fri-short = F
slots-weekday-sat-short = S
slots-weekday-sun-short = S

slots-select-date = Datum auswählen
slots-loading-availability = Verfügbarkeit wird geladen...
slots-click-highlighted = Klicke auf ein hervorgehobenes Datum, um verfügbare Zeiten zu sehen
slots-bookable-range-both = Buchbar vom { $from } bis { $until }
slots-bookable-range-from = Buchbar ab { $from }
slots-bookable-range-until = Buchbar bis { $until }
slots-no-longer-bookable = Dieser Termin kann nicht mehr gebucht werden.
slots-no-times-month = Keine verfügbaren Zeiten in diesem Monat
slots-no-times-day = Keine verfügbaren Zeiten an diesem Tag
slots-no-availability-participants = Keine gemeinsame Verfügbarkeit aller Teilnehmer in diesem Monat
slots-week-more = mehr

# Booking form (templates/book.html)

book-page-title = { $title } buchen
book-back-to-times = Zurück zu den Zeiten
book-name-label = Dein Name
book-name-placeholder = Max Mustermann
book-email-label = E-Mail
book-email-placeholder = max@example.com
book-email-invalid = Bitte gib eine vollständige E-Mail-Adresse samt Domain an (z. B. jane@example.com).
book-notes-label = Notizen
book-notes-optional = (optional)
book-notes-placeholder = Möchtest du etwas Bestimmtes besprechen?
book-additional-guests-label = Weitere Teilnehmende
book-additional-guests-hint = (optional, bis zu { $max })
book-add-guest-btn = + Teilnehmer hinzufügen
book-guest-email-placeholder = kollege@example.com
book-phone-label = Telefonnummer
book-phone-placeholder = 0151 23456789
book-phone-help = Lokale Nummern sind in Ordnung; ohne führendes + wird { $country } angenommen.
book-phone-optional-consequence = Lass das Feld leer, wenn du keine SMS zu dieser Buchung erhalten möchtest.
book-phone-required = Für diese Buchung ist eine Telefonnummer erforderlich.
book-phone-invalid-title = Ungültige Telefonnummer
book-phone-invalid = Bitte gib eine per SMS erreichbare Nummer an oder lass das Feld leer.
book-phone-country-search = Suchen
book-phone-country-label = Land auswählen
book-phone-country-none = Kein Land ausgewählt
book-phone-country-no-results = Keine Länder entsprechen dieser Suche
captcha-label = Sicherheitsüberprüfung
captcha-initial-state = Bestätige, dass du ein Mensch bist
captcha-verifying = Überprüfung läuft...
captcha-solved = Du bist ein Mensch
captcha-error = Fehler
captcha-troubleshooting = Fehlerbehebung
captcha-wasm-disabled = WASM aktivieren für deutlich schnellere Lösung
captcha-verify-aria = Klicke, um zu bestätigen, dass du ein Mensch bist
captcha-verifying-aria = Überprüfung läuft, bitte warten
captcha-verified-aria = Bestätigt
captcha-required = Bitte bestätige, dass du ein Mensch bist
captcha-error-aria = Ein Fehler ist aufgetreten, bitte versuche es erneut
book-confirm-button = Buchung bestätigen

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Buchung bestätigt: { $event }, { $date } um { $time } ({ $tz }).
sms-cancelled = Buchung storniert: { $event }, { $date } um { $time } ({ $tz }).
sms-rescheduled = Buchung verschoben: { $event } findet jetzt am { $date } um { $time } statt ({ $tz }).
sms-reminder = Erinnerung: { $event } beginnt am { $date } um { $time } ({ $tz }).

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Gast:
common-detail-reason = Grund:
common-reason-optional = (optional)
common-close-page = Du kannst diese Seite schließen.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Buchung stornieren
cancel-heading = Buchung stornieren
cancel-subtitle = Du bist dabei, deine Buchung zu stornieren.
cancel-reason-label = Grund
cancel-reason-placeholder-host = Teile dem Gastgeber den Grund mit...
cancel-button = Buchung stornieren
cancelled-heading = Buchung storniert
cancelled-subtitle = Deine Buchung wurde storniert und der Gastgeber wurde benachrichtigt.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Buchung ablehnen
decline-heading = Buchung ablehnen
decline-subtitle = Du bist dabei, diese Buchungsanfrage abzulehnen.
decline-reason-placeholder-guest = Teile dem Gast den Grund mit...
decline-button = Buchung ablehnen
declined-heading = Buchung abgelehnt
declined-subtitle = Die Buchung wurde abgelehnt und der Gast wurde benachrichtigt.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Buchung genehmigen
approve-heading = Buchung genehmigen
approve-subtitle = Du bist dabei, diese Buchungsanfrage zu genehmigen.
approve-button = Buchung genehmigen
approved-heading = Buchung genehmigt
approved-subtitle = Die Buchung wurde bestätigt und eine Bestätigungs-E-Mail wurde an { $email } gesendet.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Buchung übernehmen
claim-heading = Buchung übernehmen
claim-subtitle = Du bist dabei, diese Buchung zu übernehmen. Du wirst als Teilnehmer hinzugefügt.
claim-assigned-to = Zugewiesen an:
claim-button = Diese Buchung übernehmen
claimed-page-title = Buchung übernommen
claimed-heading = Buchung übernommen
claimed-subtitle = Du hast diese Buchung übernommen. Eine Kalendereinladung wurde an deine E-Mail-Adresse gesendet.
already-claimed-page-title = Bereits übernommen
already-claimed-heading = Bereits übernommen
already-claimed-subtitle = Diese Buchung wurde bereits von { $name } übernommen.

# Generic error page (booking_action_error.html)

action-error-page-title = Fehler bei der Buchungsaktion

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Buchung verschieben — calrs
host-resched-heading = Buchung verschieben
host-resched-subtitle = { $guest } erhält eine E-Mail mit der Bitte, einen neuen Termin auszuwählen.
host-resched-currently = Aktuell:
host-resched-button = Verschiebungsanfrage senden
host-resched-cancel-link = Abbrechen

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Verschiebung bestätigen
resched-confirm-heading = Verschiebung bestätigen
resched-confirm-subtitle = Du bist dabei, deine Buchung auf einen neuen Termin zu verschieben.
resched-was = Vorher:
resched-new = Neu:
resched-button = Verschiebung bestätigen
resched-back-to-picker = Zurück zur Terminauswahl

# Base layout chrome (templates/base.html)

base-loader-checking = Verfügbarkeit wird geprüft
base-loader-please-wait = Bitte warte, die neuesten Kalenderdaten werden geladen...
base-stop-impersonating = Identitätswechsel beenden
base-theme-toggle = Design wechseln
base-powered-by = Läuft mit

# Profile (templates/profile.html)

profile-pick-event-type-invite = Wähle eine Terminart, um einen Termin zu buchen.
profile-no-event-type = Noch keine Terminarten verfügbar.

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

common-month-1 = Januar
common-month-2 = Februar
common-month-3 = März
common-month-4 = April
common-month-5 = Mai
common-month-6 = Juni
common-month-7 = Juli
common-month-8 = August
common-month-9 = September
common-month-10 = Oktober
common-month-11 = November
common-month-12 = Dezember
common-month-short-1 = Jan.
common-month-short-2 = Feb.
common-month-short-3 = März
common-month-short-4 = Apr.
common-month-short-5 = Mai
common-month-short-6 = Juni
common-month-short-7 = Juli
common-month-short-8 = Aug.
common-month-short-9 = Sept.
common-month-short-10 = Okt.
common-month-short-11 = Nov.
common-month-short-12 = Dez.

common-weekday-long-mon = Montag
common-weekday-long-tue = Dienstag
common-weekday-long-wed = Mittwoch
common-weekday-long-thu = Donnerstag
common-weekday-long-fri = Freitag
common-weekday-long-sat = Samstag
common-weekday-long-sun = Sonntag

# Format patterns are parametric per locale to handle word order. Translators
# pick where each placeholder lands. Example outputs:
#   EN: April 2026  /  Tuesday, March 12, 2026
#   FR: avril 2026  /  mardi 12 mars 2026
#   ES: abril 2026  /  martes, 12 de marzo de 2026
common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday }, { $day }. { $month } { $year }
common-format-date = { $day }. { $month } { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Verschieben
email-action-cancel-booking = Buchung stornieren

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
email-confirm-greeting = Hallo { $name },
email-confirm-headline = Deine Buchung wurde bestätigt!
email-confirm-ics-attached-plain = Eine Kalendereinladung ist beigefügt.
email-confirm-ics-attached-html = Eine Kalendereinladung ist dieser E-Mail beigefügt.
email-confirm-need-to-cancel = Stornieren? { $url }

# Email: guest reminder

email-reminder-subject = Erinnerung: { $event } um { $time }
email-reminder-headline = Dein Termin steht bevor.

# Email: guest cancellation

email-cancel-subject = Storniert: { $event } — { $date }
email-cancel-headline-by-host = Deine Buchung wurde von { $host } storniert.
email-cancel-headline-by-guest = Deine Buchung wurde storniert.
email-cancel-ics-attached-plain = Eine Kalenderstornierung ist beigefügt.
email-cancel-ics-attached-html = Eine Kalenderstornierung ist dieser E-Mail beigefügt.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Hinweis: Für eine Stornierung sind mindestens { $minutes } Minuten Vorlaufzeit nötig.
email-confirm-reschedule-notice = Hinweis: Für eine Verschiebung sind mindestens { $minutes } Minuten Vorlaufzeit nötig.

# Event type form: cancel/reschedule minimum notice (templates/event_type_form.html)

# Google Meet (templates/event_type_form.html, src/google_meet.rs)
event-type-form-location-google-meet = Google Meet (automatisch erzeugter Link)
event-type-form-location-google-meet-hint = Bei der Bestätigung wird ein eigener Google-Meet-Link erstellt, im Namen des zugewiesenen Gastgebers. Jeder Gastgeber (du oder jedes berechtigte Teammitglied) muss Google Kalender verbunden und einen Schreibkalender ausgewählt haben.
google-meet-prereq-no-host = Google Meet setzt einen Gastgeber mit verbundenem Google Kalender voraus.
google-meet-prereq-no-eligible = Google Meet setzt mindestens ein berechtigtes Teammitglied mit verbundenem Google Kalender voraus.
google-meet-prereq-missing = Google Meet setzt voraus, dass jeder Gastgeber Google Kalender verbunden und einen Schreibkalender ausgewählt hat. Es fehlen noch: { $names }. Verbinde sie unter Dashboard → Kalenderquellen.
google-meet-unavailable-title = Google Meet ist nicht verfügbar
google-meet-dynamic-group-unavailable = Der Gastgeber braucht einen verbundenen Google Kalender mit ausgewähltem Schreibkalender.

event-type-form-cancel-notice-label = Mindestvorlaufzeit für Stornierungen
event-type-form-reschedule-notice-label = Mindestvorlaufzeit für Verschiebungen
event-type-form-notice-help = Leer lassen für keine Einschränkung.
event-type-form-resources-label = Erforderliche Ressourcen
event-type-form-resources-hint = Zeitfenster werden nur angeboten, wenn die ausgewählten Ressourcen gemäß dem Modus unten verfügbar sind.
event-type-form-resources-mode-all = Alle ausgewählten Ressourcen müssen frei sein
event-type-form-resources-mode-round-robin = Eine freie Ressource genügt (sie wird der Buchung zugewiesen)
event-type-form-notice-unit-minutes = Minuten
event-type-form-notice-unit-hours = Stunden
event-type-form-notice-unit-days = Tage
event-type-form-booking-horizon-label = Buchungshorizont
event-type-form-booking-horizon-help = Wie viele Tage im Voraus Gäste buchen können. Leer lassen für kein Limit, 0 für nur heute.
event-type-form-bookable-range-label = Buchbarer Zeitraum
event-type-form-bookable-from = von
event-type-form-bookable-until = bis
event-type-form-bookable-range-help = Nur an diesen Tagen kann gebucht werden. Leer lassen für kein Limit. Für andere Zeiten an einzelnen Tagen nutze Ausnahmen.
event-type-form-bookable-range-past-warning = Dieser Zeitraum ist bereits vorbei. Der Termintyp wird nicht mehr öffentlich angezeigt.
event-type-form-bookable-range-too-far-warning = Dieser Zeitraum beginnt in mehr als einem Jahr. Gäste können höchstens ein Jahr im Voraus buchen, die Buchung öffnet daher am { $date }.
event-type-form-bookable-range-horizon-warning = Der Buchungshorizont endet, bevor dieser Zeitraum beginnt. Gäste können ab { $date } buchen.
event-type-form-bookable-range-bookings-outside =
    { $count ->
        [one] 1 bestehende Buchung liegt außerhalb des buchbaren Zeitraums.
       *[other] { $count } bestehende Buchungen liegen außerhalb des buchbaren Zeitraums.
    }
event-type-form-bookable-range-bookings-link = Buchungen ansehen
event-type-form-saved = Gespeichert.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = Eine Stornierung erfordert mindestens { $minutes } Minuten Vorlaufzeit vor dem Termin.
confirmed-reschedule-notice-info = Eine Verschiebung erfordert mindestens { $minutes } Minuten Vorlaufzeit vor dem Termin.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Diese Buchung kann online nicht mehr storniert werden
booking-blocked-title-reschedule = Diese Buchung kann online nicht mehr verschoben werden
booking-blocked-body = Der Gastgeber verlangt mindestens { $minutes } Minuten Vorlaufzeit. Falls du nicht teilnehmen kannst, schreib bitte direkt an <a href="mailto:{ $host_email }">{ $host_email }</a>.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Kopieren
dashboard-event-types-copied = Kopiert!
dashboard-event-types-copy-title = Buchungslink kopieren
dashboard-event-types-copy-failed = Kopieren fehlgeschlagen

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Terminplanung
nav-overview = Übersicht
nav-event-types = Terminarten
nav-bookings = Buchungen
nav-teams = Teams
nav-section-shared-links = Geteilte Links
nav-invite-links = Einladungslinks
nav-section-calendars = Kalender
nav-sources = Quellen
nav-section-personal = Persönlich
nav-settings = Profil & Einstellungen
nav-troubleshoot = Diagnose
nav-section-admin = Administration
nav-admin-panel = Administrationsbereich
nav-sign-out = Abmelden
nav-release-notes = Versionshinweise ansehen
dash-skip-to-content = Zum Inhalt springen
dash-open-menu = Menü öffnen
dash-shortcuts-title = Tastenkürzel
dash-shortcut-search = Diese Seite durchsuchen
dash-shortcut-new = Neu erstellen
dash-shortcut-save = Formular speichern
dash-shortcut-close = Menüs und Dialoge schließen
dash-shortcut-help = Diese Hilfe anzeigen
dash-shortcut-go = Zu { $page } wechseln
dash-shortcut-then = dann
dash-close = Schließen
dash-copied = Link in die Zwischenablage kopiert
dash-copy-manual = Kopiere diesen Link:
dash-confirm-again = Zum Bestätigen erneut klicken
dash-unsaved = Ungespeicherte Änderungen
dash-no-matches = Nichts passt zu dieser Suche.
dash-more-actions = Weitere Aktionen

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = Die Zeitzone deines Browsers ist { $detected }, deine Buchungszeitzone ist jedoch auf { $current } eingestellt.
tz-banner-update = Aktualisieren
tz-banner-dismiss = Ausblenden

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = URL eingeben:
editor-link-default-label = Linktext
editor-placeholder-text = Text
editor-nothing-to-preview = Keine Vorschau verfügbar

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Dashboard
overview-avail-banner-body = Deine Standardarbeitszeiten wurden auf Mo–Fr, 9:00–17:00 Uhr gesetzt. Sie gelten, wenn andere dich in dynamische Gruppentermine einbeziehen.
overview-avail-banner-cta = Verfügbarkeit prüfen
overview-dismiss = Ausblenden
overview-getting-started = Erste Schritte
overview-step-connect-calendar = Kalender verbinden
overview-step-first-event-type = Erste Terminart anlegen
overview-step-share-link = Buchungslink teilen
overview-pending-approval = Warten auf Bestätigung
overview-confirm = Bestätigen
overview-decline = Ablehnen
overview-copy-link = Buchungslink kopieren
overview-duration-h = { $hours } Std.
overview-duration-m = { $minutes } Min.
overview-duration-hm = { $hours } Std. { $minutes } Min.
overview-percent = { $value } %
overview-chart-title = Termine pro Woche
overview-chart-this-week = Diese Woche
overview-chart-week-of = Woche vom { $date }
overview-series-confirmed = Bestätigt
overview-series-pending = Ausstehend
overview-series-cancelled = Storniert oder abgelehnt
overview-view-data = Als Tabelle anzeigen
overview-table-week = Woche
overview-types-title = Nach Terminart
overview-types-other = Weitere Terminarten
overview-agenda-title = Als Nächstes
overview-agenda-today = Heute
overview-agenda-tomorrow = Morgen
overview-agenda-now = Läuft gerade
overview-agenda-join = Beitreten
overview-agenda-empty = Keine Termine in den nächsten 14 Tagen. Teile deinen Buchungslink, um welche zu bekommen.
overview-agenda-all = Alle Buchungen
nav-main-label = Hauptnavigation
dash-shortcuts-short = Tastenkürzel
dash-hint-to-decide = { $count } offen
dash-hint-failing = { $count } fehlerhaft
common-format-day-date = { $weekday }, { $day }. { $month }
common-format-short-date = { $day }. { $month }
bookings-summary = Anstehend: { $upcoming }. Warten auf deine Entscheidung: { $pending }. Zeiten in { $tz }.
sources-state-failing = Fehlerhaft
troubleshoot-previous-day = Vorheriger Tag
troubleshoot-next-day = Nächster Tag
overview-new-meeting-type = Neue Terminart
overview-step-connect = Verbinden
overview-step-create = Erstellen
overview-links-title = Deine Buchungslinks
overview-open-calendar = Deinen Kalender öffnen
overview-week-title = Diese Woche
overview-week-closed = geschlossen
overview-week-total = { $booked } gebucht
overview-week-number = KW { $week }
overview-week-short = KW { $week }
overview-time-range = { $from } bis { $to }
overview-until = bis { $time }
overview-expires = Läuft ab { $when }
overview-guest-and-more = { $name } und { $count } weitere
overview-mast-now = Jetzt: { $title } mit { $guest }, bis { $time }
overview-mast-next = Als Nächstes: { $title } mit { $guest } um { $time }
overview-mast-next-day = Als Nächstes: { $title } mit { $guest }, { $day } um { $time }
overview-ruler-booked = Gebucht
overview-ruler-pending = Wartet auf Entscheidung
overview-ruler-unavailable = Nicht verfügbar (dein Kalender)
overview-key-open = Offen für Buchungen
overview-ruler-aria = Heute, { $from } bis { $to }
overview-chart-now = jetzt
overview-figures-title = Letzte 30 Tage
overview-fig-booked = Gebuchte Termine
overview-fig-time = Zeit in Terminen
overview-fig-cancelled = Storniert
overview-fig-notice = Durchschnittlicher Vorlauf
overview-fig-before = { $value } in den 30 Tagen davor
overview-fig-prev-key = Die 30 Tage davor
overview-fig-days =
    { $count ->
        [one] 1 Tag
       *[other] { $count } Tage
    }

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Buchungen
bookings-pending-approval = Warten auf Bestätigung
bookings-available-to-claim = Zur Übernahme verfügbar
bookings-upcoming = Anstehende Buchungen
bookings-guest-booked = Vom Gast gebucht:
bookings-resource = Ressource:
bookings-confirm = Bestätigen
bookings-reschedule = Verschieben
bookings-decline = Ablehnen
bookings-claim = Übernehmen
bookings-badge-awaiting-reschedule = Verschiebung ausstehend
bookings-badge-writeback-failed = nicht in deinem Kalender
bookings-writeback-failed-detail = Schreiben in den Kalender fehlgeschlagen: { $error }
bookings-cancel = Stornieren
bookings-reason-placeholder = Grund (optional)
bookings-confirm-cancel = Stornierung bestätigen
bookings-back = Zurück
bookings-empty = Noch keine anstehenden Buchungen.<br>Teile deine { $link }, damit andere Zeit mit dir buchen können.
bookings-empty-link-label = Terminart-Links
bookings-search-placeholder = Gäste, E-Mails oder Termine suchen
bookings-recent = Letzte 30 Tage
bookings-cancelled = Abgesagt und abgelehnt

# Host notes on a booking (templates/booking_notes.html, bookings page, overview, host emails)

host-notes-title = Notizen der Gastgeber
host-notes-hint = Sichtbar für die Gastgeber dieser Buchung, ein Teammitglied, das sie übernommen hat, und einen Admin, der als einer ihrer Gastgeber handelt. Dem Gast werden sie nie angezeigt, und sie landen nie in der Einladung oder im Kalendertermin.
host-notes-placeholder = Zum Beispiel: was in einem Telefonat gesagt wurde, was vorzubereiten ist.
host-notes-save = Notizen speichern
host-notes-saved = Notizen gespeichert.
host-notes-too-long = Notizen der Gastgeber dürfen höchstens { $max } Zeichen lang sein.
host-notes-conflict = Diese Notizen wurden geändert, nachdem diese Seite geöffnet wurde. Die gespeicherte Fassung steht unten; erneutes Speichern ersetzt sie durch den Text im Feld.
host-notes-current = Gespeicherte Fassung:
host-notes-last-edit = Zuletzt bearbeitet von { $name }, { $when }
host-notes-last-edit-anon = Zuletzt bearbeitet am { $when }
host-notes-edit = Notiz bearbeiten
host-notes-marker = Notiz der Gastgeber
host-notes-cleared = (gelöscht)
host-notes-last-edit-as = Zuletzt bearbeitet von { $name } (Admin, als { $host }), { $when }
dash-not-found-title = Nicht gefunden
dash-not-found-body = Diese Seite existiert nicht, oder dieses Konto hat keinen Zugriff darauf.
host-notes-from-guest = Vom Gast:
bookings-host-notes = Notizen der Gastgeber:
email-detail-host-notes = Notizen der Gastgeber:

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Teams
teams-heading = Teams
teams-new = Neu
teams-badge-public = öffentlich
teams-badge-private = privat
teams-settings = Einstellungen
teams-view = Ansehen
teams-empty = Noch keine Teams.
teams-empty-admin = { $link }, um mit deinem Team zusammenzuarbeiten.
teams-empty-admin-link-label = Lege eines an
teams-empty-member = Teams werden von der Administration angelegt. Bitte sie, eines anzulegen und dich als Mitglied hinzuzufügen.
teams-subtitle = Personen, die Termine teilen, reihum oder alle gemeinsam gebucht.

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Einladungslinks
invite-links-heading = Einladungslinks
invite-links-new = Neuer interner Termin
invite-links-help = Erzeuge Einmal-Buchungslinks für interne Terminarten. Alle angemeldeten Kolleginnen und Kollegen können hier Links erstellen und teilen.
invite-links-duration = { $minutes } Min.
invite-links-hosted-by = Gastgeber: { $host }
invite-links-get-link = Link erzeugen
invite-links-invites = Einladungen
invite-links-empty = Noch keine internen Terminarten.<br>{ $link } mit der Sichtbarkeit „Intern“, damit alle Kolleginnen und Kollegen Buchungslinks erzeugen können.
invite-links-empty-link-label = Lege eine Terminart an
invite-links-js-generating = Wird erzeugt...
invite-links-js-copied = Kopiert!
invite-links-js-error = Fehler

teams-member-count =
    { $count ->
        [one] { $count } Mitglied
       *[other] { $count } Mitglieder
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Kalenderquellen
sources-heading = Kalenderquellen
sources-add = Hinzufügen
sources-last-sync = Letzte Synchronisierung:
sources-sync-failing = Synchronisierung schlägt seit { $since } UTC fehl: { $error }
sources-sync = Synchronisieren
sources-full-resync = Vollständige Neusynchronisierung
sources-full-resync-title = Cache leeren und alle Termine erneut vom Server laden
sources-test = Testen
sources-reconnect = Neu verbinden
sources-reconnect-title = Die Google-Zustimmung erneut durchlaufen
sources-edit = Bearbeiten
sources-remove = Entfernen
sources-remove-confirm = Quelle „{ $name }“ entfernen? Dabei werden alle von dieser Quelle synchronisierten Termine gelöscht.
sources-no-write-calendar = Kein Schreibkalender ausgewählt. Bestätigte Buchungen bleiben in calrs und werden nicht in diesen Kalender übertragen. Wähle unten einen aus, um das Rückschreiben zu aktivieren.
sources-write-calendar-read-only = Der Kalender, in den Buchungen geschrieben werden, ist jetzt schreibgeschützt, sie kommen dort nicht mehr an. Wähle unten einen beschreibbaren aus.
sources-write-calendar-refused = Dieser Kalender kann keine Buchungen aufnehmen: Er gehört nicht zu dieser Quelle oder das Konto darf nicht hineinschreiben. Wähle einen anderen.
sources-microsoft-not-ready = calrs hat diesen Kalender noch nicht fertig gelesen. Bis dahin zeigen deine Buchungsseiten keine freie Zeit. Das dauert nach dem Verbinden bis zu etwa einer Minute; dauert es länger, nennt der Synchronisierungsfehler oben den Grund.
sources-write-bookings-to = Buchungen schreiben nach:
sources-write-none = Keiner (nicht schreiben)
sources-empty = Keine Kalenderquellen verbunden. { $link }, um die Verfügbarkeit zu prüfen.
sources-empty-link-label = Füge eine hinzu
sources-subtitle = Kalender, die calrs liest, um zu wissen, wann du beschäftigt bist, und in die es neue Buchungen schreibt.
sources-never-synced = Noch nie synchronisiert
sources-calendars = Kalender
sources-calendars-synced = Synchronisierte Kalender: { $synced } von { $total }
source-calendars-page-title = Zu synchronisierende Kalender
source-calendars-heading = Zu synchronisierende Kalender
source-calendars-back = Zurück zu den Kalenderquellen
source-calendars-help = Wähle, welche Kalender von { $name } calrs synchronisiert. Nicht ausgewählte Kalender werden nie abgerufen und blockieren deine Verfügbarkeit nicht.
source-calendars-select-all = Alle auswählen
source-calendars-select-none = Keine auswählen
source-calendars-write-badge = Buchungen werden hier eingetragen
source-calendars-write-hint = Bestätigte Buchungen werden in diesen Kalender geschrieben, ob er synchronisiert wird oder nicht.
source-calendars-save = Speichern
source-calendars-saved = Gespeichert. Neu ausgewählte Kalender werden bei der nächsten Synchronisierung abgerufen; mit „Synchronisieren“ auf der Seite der Quellen holst du sie sofort.
source-calendars-empty = Für diese Quelle wurden noch keine Kalender gefunden. Starte eine Synchronisierung, um sie zu finden.

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Terminarten
event-types-heading = Terminarten
event-types-new = Neu
event-types-badge-disabled = deaktiviert
event-types-badge-internal = intern
event-types-badge-private = privat
event-types-badge-resources = Ressourcen
event-types-badge-active = aktiv
event-types-badge-expired = abgelaufen
event-types-badge-range-from = ab { $date }
event-types-badge-range-until = bis { $date }
event-types-send-invites = Einladungen senden
event-types-duration = { $minutes } Min.
event-types-mode-collective = kollektiv
event-types-mode-round-robin = Reihum
event-types-edit = Bearbeiten
event-types-disable = Deaktivieren
event-types-enable = Aktivieren
event-types-embed = Einbetten
event-types-overrides = Ausnahmen
event-types-team-settings = Teameinstellungen
event-types-invites = Einladungen
event-types-view-public = Öffentliche Seite ansehen
event-types-view-page = Seite ansehen
event-types-delete = Löschen
event-types-delete-confirm = Terminart „{ $title }“ löschen? Dies kann nicht rückgängig gemacht werden.
event-types-duplicate = Duplizieren
event-types-duplicate-title = { $title } (Kopie)
event-types-empty = Noch keine Terminarten. { $link }, um Buchungen anzunehmen.
event-types-empty-link-label = Lege eine an
event-types-subtitle = Die Termine, die andere bei dir buchen können. Kopiere einen Link oder bette ihn auf deiner Website ein.
event-types-search-placeholder = Nach Titel oder Link filtern
event-types-switch-label = Nimmt Buchungen an

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Fett (Strg+B)
editor-italic = Kursiv (Strg+I)
editor-strikethrough = Durchgestrichen
editor-code = Inline-Code
editor-link = Link einfügen (Strg+K)
editor-toggle-preview = Vorschau ein-/ausblenden
editor-preview = Vorschau

# Profile and settings (templates/settings.html)

settings-page-title = Einstellungen
settings-heading = Profil & Einstellungen
settings-subtitle = Dein öffentliches Profil, Sprache, Zeitzone und Standard-Arbeitszeiten.
settings-public-page-label = Deine öffentliche Buchungsseite
settings-copy = Kopieren
settings-copied = Kopiert!
settings-open = Öffnen
settings-avatar = Avatar
settings-upload = Hochladen
settings-remove = Entfernen
settings-display-name = Anzeigename
settings-display-name-placeholder = Dein Name
settings-username = Benutzername
settings-username-hint = (wird in deiner Buchungs-URL verwendet)
settings-username-pattern-title = Nur Kleinbuchstaben, Ziffern und Bindestriche
settings-username-help = Deine öffentliche Buchungsseite:
settings-title = Funktion
settings-title-placeholder = z. B. Softwareentwicklerin, Produktmanager
settings-title-help = Wird auf deinem öffentlichen Profil und in der Seitenleiste angezeigt.
settings-bio = Kurzprofil
settings-bio-placeholder = Erzähle ein wenig über dich...
settings-bio-help = Wird auf deiner öffentlichen Buchungsseite angezeigt. Unterstützt **fett**, *kursiv*, ~~durchgestrichen~~, `Code` und [Links](url).
settings-booking-email = Buchungs-E-Mail
settings-booking-email-help = Diese Adresse erscheint auf deinen öffentlichen Buchungsseiten und in E-Mail-Benachrichtigungen. Leer lassen, um deine Anmeldeadresse zu verwenden.
settings-booking-email-warning = Stelle sicher, dass diese Adresse bei deinem Mailanbieter existiert. Andernfalls werden Benachrichtigungen nicht zugestellt.
settings-timezone = Zeitzone
settings-timezone-help = Deine Verfügbarkeitsregeln und Buchungszeiten werden in dieser Zeitzone berechnet.
settings-language = Sprache
settings-language-auto = Automatisch (Browsereinstellung)
settings-language-help = Wähle eine Oberflächensprache oder belasse es bei „Automatisch“, um der Browsereinstellung zu folgen.
settings-public-page-language = Sprache der öffentlichen Buchungsseiten
settings-public-page-language-visitor = Sprache des jeweiligen Besuchers
settings-public-page-language-host = Immer meine Sprache
settings-public-page-language-help = Sprache deines öffentlichen Profils, deiner Buchungsseiten und der E-Mails an deine Gäste. „Immer meine Sprache“ verwendet die oben gewählte Sprache.
settings-error-public-page-language = Wähle eine gültige Sprache für die öffentlichen Buchungsseiten.
settings-dynamic-group = Anderen erlauben, mich in dynamische Gruppenlinks einzubeziehen
settings-dynamic-group-help = Wenn aktiviert, können andere Benutzer spontane Gruppentermin-URLs erstellen, die dich einschließen (z. B. { $example }).
settings-lend-resource = Meinen Kalenderzugang für Ressourcenreservierungen bereitstellen
settings-lend-resource-help = Wenn eine Buchung eine geteilte Ressource (Demolabor, Besprechungsraum) reservieren muss, in die dein Kalenderkonto schreiben darf, erlaube calrs, deine gespeicherten Kalenderzugangsdaten dafür zu verwenden.
settings-default-availability = Standardverfügbarkeit
settings-default-availability-help = Deine Standardarbeitszeiten. Werden für dynamische Gruppenlinks verwendet, wenn andere dich zu einem Termin hinzufügen.
settings-copy-to-all = Auf alle Tage übertragen
settings-copy-to-all-title = Die Zeitfenster des ersten aktivierten Tages auf alle anderen aktivierten Tage übertragen
settings-add-window = Zeitfenster hinzufügen
settings-remove-window = Zeitfenster entfernen
settings-save = Einstellungen speichern
settings-appearance = Darstellung
settings-theme-system = System
settings-theme-light = Hell
settings-theme-dark = Dunkel

# Sign in (templates/auth/login.html)

login-page-title = Anmelden
login-heading = Anmelden
login-subtitle = Melde dich bei deinem calrs-Konto an
login-sso = Mit SSO anmelden
login-or = oder
login-email = E-Mail
login-password = Passwort
login-submit = Mit E-Mail anmelden
login-no-account = Noch kein Konto? { $link }
login-register-link = Registrieren

# Registration (templates/auth/register.html)

register-page-title = Registrieren
register-heading = Konto erstellen
register-subtitle = Registriere ein neues calrs-Konto
register-domains-limited = Die Registrierung ist beschränkt auf: { $domains }
register-name = Name
register-name-placeholder = Dein Name
register-email = E-Mail
register-password = Passwort
register-password-hint = (mind. 12 Zeichen)
register-submit = Konto erstellen
register-have-account = Du hast bereits ein Konto? { $link }
register-signin-link = Anmelden

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Zu viele Anmeldeversuche. Bitte versuche es später erneut.
auth-error-invalid-credentials = Ungültige E-Mail-Adresse oder ungültiges Passwort
auth-error-internal = Interner Fehler
auth-error-registration-disabled = Die Registrierung ist deaktiviert.
auth-error-name-length = Der Name muss zwischen 1 und 255 Zeichen lang sein
auth-error-email-length = Die E-Mail-Adresse muss zwischen 1 und 255 Zeichen lang sein
auth-error-email-invalid = Bitte gib eine gültige E-Mail-Adresse ein
auth-error-email-domain = E-Mail-Domain nicht zugelassen
auth-error-password-length = Das Passwort muss mindestens 12 Zeichen lang sein
auth-error-email-taken = Diese E-Mail-Adresse ist bereits registriert
auth-error-create-failed = Konto konnte nicht erstellt werden

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Kalenderquelle
source-test-sync-heading = Synchronisierung: { $name }
source-test-heading = Verbindungstest
source-write-page-title = Kalender-Rückschreiben einrichten
source-write-back = Zurück zum Dashboard
source-write-heading = Wohin sollen Buchungen geschrieben werden?
source-write-help = Wenn jemand einen Termin mit dir bucht, kann calrs den Eintrag automatisch in deinem Kalender anlegen. Wähle, in welchen Kalender Buchungen für { $name } geschrieben werden.
source-write-save = Speichern
source-write-skip = Vorerst überspringen
source-write-sync-results = Ergebnisse der Synchronisierung

source-write-event-count =
    { $count ->
        [one] { $count } Termin
       *[other] { $count } Termine
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Datumsausnahmen
overrides-heading = Datumsausnahmen
overrides-back-teams = Zurück zu den Teams
overrides-back-event-types = Zurück zu den Terminarten
overrides-intro = Datumsbezogene Ausnahmen für { $title } hinzufügen
overrides-add-heading = Neue Ausnahme hinzufügen
overrides-date = Datum
overrides-type = Art der Ausnahme
overrides-type-blocked = Ganzen Tag sperren
overrides-type-custom = Abweichende Zeiten
overrides-start-time = Startzeit
overrides-end-time = Endzeit
overrides-add-submit = Ausnahme hinzufügen
overrides-existing = Bestehende Ausnahmen
overrides-badge-blocked = gesperrt
overrides-badge-custom = abweichende Zeiten
overrides-delete = Löschen
overrides-delete-confirm = Diese Ausnahme löschen?
overrides-empty = Noch keine Datumsausnahmen.<br>Nutze das Formular oben, um bestimmte Tage zu sperren (Feiertage, freie Tage) oder abweichende Zeiten festzulegen.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Wähle eine Terminart, um einen Termin zu buchen.
team-profile-empty = Noch keine Terminarten verfügbar.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Diagnose
troubleshoot-empty = Keine Terminarten gefunden. { $link }, um mit der Verfügbarkeitsdiagnose zu beginnen.
troubleshoot-empty-link-label = Lege eine an
troubleshoot-subtitle = Sieh nach, warum Zeitfenster für { $title } verfügbar oder blockiert sind
troubleshoot-duration = { $minutes } Min.
troubleshoot-buffer-before = { $minutes } Min. Puffer davor
troubleshoot-buffer-after = { $minutes } Min. Puffer danach
troubleshoot-min-notice = { $minutes } Min. Vorlaufzeit
troubleshoot-blocked-override = Durch Datumsausnahme gesperrt (freier Tag)
troubleshoot-custom-hours-active = Ausnahme mit abweichenden Zeiten aktiv (ersetzt die Wochenregeln)
troubleshoot-microsoft-not-ready = Dein Microsoft-Kalender ist noch nicht zum ersten Mal synchronisiert: Bis dahin ist jede Zeit belegt. Siehe Kalenderquellen.
troubleshoot-legend-available = Verfügbar
troubleshoot-legend-calendar-event = Kalendertermin
troubleshoot-legend-booking = Buchung
troubleshoot-legend-resource = Ressource belegt
troubleshoot-legend-outside = Außerhalb der Zeiten
troubleshoot-legend-buffer = Puffer / Mindestvorlaufzeit
troubleshoot-blocked-slots = Blockierte Zeitfenster
troubleshoot-none-date-blocked = Dieses Datum ist durch eine Verfügbarkeitsausnahme gesperrt (freier Tag). Keine Zeitfenster verfügbar.
troubleshoot-none-custom-hours = Ausnahme mit abweichenden Zeiten aktiv, aber keine passenden Zeitfenster. Prüfe die Einstellungen der Ausnahme.
troubleshoot-none-no-rules = Keine Verfügbarkeitsregeln für diesen Wochentag. Diese Terminart ist am { $date } nicht buchbar.
troubleshoot-none-all-bookable = Keine blockierten Zeitfenster innerhalb der Verfügbarkeitszeiten. Alle Zeiten sind buchbar.
troubleshoot-label-outside = Außerhalb der Verfügbarkeit
troubleshoot-label-available = Verfügbar
troubleshoot-label-min-notice = Mindestvorlaufzeit ({ $minutes } Min.)
troubleshoot-label-beyond-horizon = Jenseits des Buchungshorizonts ({ $days } Tage)
troubleshoot-label-outside-date-range = Außerhalb des buchbaren Zeitraums
troubleshoot-label-buffer = Puffer ({ $minutes } Min.)
troubleshoot-label-resource-busy = Ressource belegt: { $names }
troubleshoot-detail-around = Rund um: { $label }
troubleshoot-detail-around-booking = Rund um die Buchung von { $guest }
troubleshoot-reason-calendar-event = Kalendertermin: { $label }
troubleshoot-reason-booking = Buchung: { $label }

# Invite management (templates/invite_form.html)

invites-heading = Einladungen
invites-back-teams = Zurück zu den Teams
invites-back-event-types = Zurück zu den Terminarten
invites-intro = Einladungslinks für { $title } versenden
invites-capped = <strong>Die Eingabe wurde auf { $max } Empfänger pro Absendung begrenzt.</strong> Sende den Rest in einem weiteren Durchgang.
invites-failed-hint = — Einzelheiten findest du in den Serverprotokollen.
invites-quick-link = Schnelllink
invites-quick-link-help = Erzeuge einen Einmal-Link und kopiere ihn in die Zwischenablage.
invites-get-link = Link erzeugen
invites-or-email = Oder per E-Mail senden
invites-recipients = Empfänger
invites-recipients-hint = (eine Adresse pro Zeile, höchstens { $max })
invites-message = Persönliche Nachricht
invites-message-hint = (optional, geht an alle Empfänger)
invites-message-placeholder = Ich freue mich darauf, dir eine Demo zu zeigen...
invites-expires-in = Läuft ab in
invites-expires-days = { $days } Tagen
invites-expires-never = Nie
invites-allow-multiple = Mehrere Buchungen pro Empfänger erlauben
invites-send = Einladungen senden
invites-sent-heading = Gesendete Einladungen
invites-badge-expired = abgelaufen
invites-badge-used = verwendet
invites-badge-active = aktiv
invites-sent-by = Gesendet von { $name }
invites-uses = { $used }/{ $max } Verwendungen
invites-expires-at = Läuft ab am { $date }
invites-copy-link = Link kopieren
invites-delete = Löschen
invites-delete-confirm = Diese Einladung löschen?
invites-empty = Noch keine Einladungen gesendet. Nutze das Formular oben, um jemandem einen Buchungslink zu schicken.
invites-js-generating = Wird erzeugt...
invites-js-copied = Kopiert!
invites-js-error = Fehler

invites-sent-count =
    { $count ->
        [one] { $count } Einladung gesendet.
       *[other] { $count } Einladungen gesendet.
    }

invites-skipped-invalid =
    { $count ->
        [one] { $count } ungültige Zeile übersprungen:
       *[other] { $count } ungültige Zeilen übersprungen:
    }

invites-skipped-duplicate =
    { $count ->
        [one] { $count } doppelte Zeile übersprungen:
       *[other] { $count } doppelte Zeilen übersprungen:
    }

invites-failed =
    { $count ->
        [one] { $count } Einladung fehlgeschlagen (DB oder SMTP):
       *[other] { $count } Einladungen fehlgeschlagen (DB oder SMTP):
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Kalenderquelle bearbeiten
source-form-title-add = Kalender hinzufügen
source-form-heading-edit = Kalenderquelle bearbeiten
source-form-heading-add = Kalender verbinden
source-form-subtitle-edit = Aktualisiere die Verbindung. Lass das Passwort leer, um das bestehende beizubehalten. Führe nach einer Änderung von URL oder Benutzername eine Synchronisierung durch, um die Kalenderliste zu aktualisieren.
source-form-subtitle-add = Verbinde einen CalDAV-Server oder Microsoft Exchange (EWS), damit calrs bei Buchungen die Verfügbarkeit prüfen kann.
source-form-backend = Backend
source-form-preset = Voreinstellung
source-form-connect-google = Mit Google verbinden
source-form-google-unavailable = Google Kalender ist nicht verfügbar. Wende dich an deine Administration.
source-form-name = Anzeigename
source-form-name-placeholder = Mein Kalender
source-form-url-caldav = CalDAV-URL
source-form-url-ews = EWS-Endpunkt-URL
source-form-username = Benutzername
source-form-password = Passwort
source-form-password-keep = Leer lassen, um das bestehende beizubehalten
source-form-password-placeholder = App-Passwort oder Kontopasswort
source-form-skip-test = Verbindungstest überspringen
source-form-skip-test-help = Nutze dies, wenn der Test hängt (kommt bei manchen BlueMind-/Zimbra-Installationen vor). Du kannst die Verbindung später testen.
source-form-save = Änderungen speichern
source-form-add = Kalenderquelle hinzufügen
source-form-help-google-configured = Klicke auf die Schaltfläche unten, um calrs den Zugriff auf deinen Google Kalender zu erlauben.
source-form-help-google-unconfigured = Die Google-Kalender-Integration ist noch nicht eingerichtet. Bitte deine Administration, im Administrationsbereich OAuth2-Zugangsdaten für Google zu hinterlegen.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — Verwende den DAV-Endpunkt deines BlueMind-Servers.<br> Üblicherweise: <code>https://mail.yourcompany.com/dav/</code><br> Der Benutzername ist deine <strong>E-Mail-Adresse</strong> (z. B. <code>alice@yourcompany.com</code>), nicht nur der Anmeldename.<br> Wenn der Verbindungstest hängt, aktiviere „Verbindungstest überspringen“ und synchronisiere direkt.
source-form-help-nextcloud = <strong>Nextcloud</strong> — Verwende die WebDAV-Wurzel, nicht die URL eines einzelnen Kalenders.<br> Üblicherweise: <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — Verwende deine vollständige Adresse im URL-Pfad.<br> Beispiel: <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Verwende ein App-Passwort (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — Verwende <code>https://caldav.icloud.com/</code><br> Du benötigst ein App-Passwort von <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Sicherheit &rarr; App-spezifische Passwörter).
source-form-help-zimbra = <strong>Zimbra</strong> — Verwende den DAV-Endpunkt deines Zimbra-Servers.<br> Üblicherweise: <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — Verwende den DAV-Endpunkt von SOGo.<br> Üblicherweise: <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — Verwende die Wurzel-URL des Servers.<br> Üblicherweise: <code>https://cal.example.com/</code>
source-form-help-baikal = <strong>Baikal</strong>: Verwende den DAV-Endpunkt deines Baikal-Servers.<br> Üblicherweise: <code>https://cal.example.com/dav.php/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong>. Verwende den SOAP-Endpunkt:<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> Der Benutzername ist die Postfachadresse; das Passwort muss HTTP Basic über TLS zulassen (bei deaktiviertem Basic im Tenant an einem Dienstpostfach aktivieren).<br> Wähle außerdem oben im Backend-Menü <strong>Microsoft Exchange (EWS)</strong>.
source-form-help-google = <strong>Google Kalender</strong>: Verbindung über OAuth2. Kein Passwort nötig.<br>
source-form-help-other = Gib die <strong>DAV-Wurzel-URL</strong> deines CalDAV-Servers an — nicht die eines einzelnen Kalenders oder einen öffentlichen Link.<br> calrs findet deine Kalender automatisch per PROPFIND (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Fett
editor-italic-short = Kursiv
editor-link-short = Link einfügen

# Team creation (templates/team_form.html)

team-form-heading = Neues Team
team-form-name = Teamname
team-form-name-placeholder = Entwicklung
team-form-slug = Kürzel
team-form-slug-hint = (URL-tauglicher Bezeichner)
team-form-slug-pattern-title = Nur Kleinbuchstaben, Ziffern und Bindestriche
team-form-description = Beschreibung
team-form-optional = (optional)
team-form-description-placeholder = Worum es in diesem Team geht...
team-form-description-help = Wird auf der Teamseite angezeigt. Unterstützt **fett**, *kursiv* und [Links](url).
team-form-visibility = Sichtbarkeit
team-form-public = Öffentlich
team-form-private = Privat
team-form-visibility-help = Private Teams erhalten ein Einladungstoken zum Teilen. Öffentliche Teams erscheinen auf der Teamprofilseite.
team-form-members = Mitglieder
team-form-members-help = Du wirst automatisch als Teamadministrator hinzugefügt. Füge einzelne Benutzer hinzu oder verknüpfe OIDC-Gruppen.
team-form-search-placeholder = Benutzer oder Gruppen suchen...
team-form-search-users = Benutzer
team-form-search-groups = OIDC-Gruppen
team-form-you = (du)
team-form-submit = Team erstellen

# Team settings (templates/team_settings.html)

team-settings-page-title = Einstellungen
team-settings-subtitle = Teameinstellungen — Teamadministratoren können sie bearbeiten.
team-settings-public-url = Öffentliche URL
team-settings-public-url-help = Über diesen Link kann jeder buchen.
team-settings-invite-link = Einladungslink
team-settings-invite-link-help = Teile diesen Link, um Zugang zur Buchungsseite dieses privaten Teams zu geben.
team-settings-avatar = Team-Avatar
team-settings-profile = Profil
team-settings-description-placeholder = Beschreibe dieses Team...
team-settings-description-help = Wird auf der öffentlichen Buchungsseite des Teams angezeigt. Unterstützt **fett**, *kursiv* und [Links](url).
team-settings-visibility-help = Öffentliche Teams erscheinen auf der Teamprofilseite. Für private Teams ist ein Einladungslink erforderlich.
team-settings-members-help = Verwalte die Mitglieder dieses Teams. Füge einzelne Benutzer hinzu oder verknüpfe OIDC-Gruppen für die automatische Synchronisierung.
team-settings-time-off-help = Geplante Abwesenheiten der Mitglieder, in deiner Zeitzone, { $timezone }. Die Notizen bleiben privat.
team-settings-role-member = Mitglied
team-settings-role-admin = Administrator
team-settings-oidc-group = OIDC-Gruppe
team-settings-remove = Entfernen
team-settings-save = Änderungen speichern
team-settings-danger-zone = Gefahrenbereich
team-settings-danger-help = Dieses Team endgültig löschen. Terminarten werden nur getrennt, nicht gelöscht. Dies kann nicht rückgängig gemacht werden.
team-settings-delete = Dieses Team löschen
team-settings-delete-confirm = Team „{ $name }“ löschen? Dies kann nicht rückgängig gemacht werden.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Terminart bearbeiten
etf-heading-new = Neue Terminart
etf-disabled-banner = Diese Terminart ist deaktiviert, daher kann sie noch niemand buchen.
etf-team = Team
etf-team-hint = (optional — leer lassen für eine persönliche Terminart)
etf-team-personal = Persönlich
etf-scheduling-mode = Planungsmodus
etf-mode-round-robin = Reihum — einem verfügbaren Mitglied zuweisen
etf-mode-collective = Kollektiv — alle Mitglieder müssen verfügbar sein
etf-scheduling-mode-help = „Reihum“ weist die Buchung einem verfügbaren Mitglied zu (dem am wenigsten ausgelasteten zuerst). „Kollektiv“ verlangt, dass alle Mitglieder gleichzeitig frei sind.
etf-title = Titel
etf-title-placeholder = 30-minütiges Kennenlerngespräch
etf-slug = Kürzel
etf-slug-placeholder = wird aus dem Titel erzeugt
etf-description-placeholder = Ein kurzes Kennenlerngespräch, um zu besprechen...
etf-description-help = Wird auf der Buchungsseite angezeigt. Unterstützt **fett**, *kursiv* und [Links](url).
etf-location = Ort
etf-location-link = Videoanruf (feste URL)
etf-location-jitsi = Jitsi (automatisch erzeugter Raum)
etf-location-webhook = Webhook (eigener Anbieter)
etf-location-phone = Telefon
etf-location-in-person = Vor Ort
etf-location-custom = Benutzerdefiniert
etf-location-details = Details
etf-location-details-placeholder = https://meet.example.com/mein-raum
etf-pattern-placeholder = Leer lassen, um das Standardmuster der Organisation zu verwenden
etf-duration = Dauer (Minuten)
etf-slot-interval = Zeitfensterabstand (Minuten)
etf-slot-interval-placeholder = Wie die Dauer
etf-slot-interval-help = Wie oft Zeitfenster beginnen. Leer lassen, um der Dauer zu folgen.
etf-required-members = Erforderliche Mitglieder
etf-required-members-help = Alle angehakten Mitglieder müssen frei sein, damit ein Zeitfenster angeboten wird. Nimm den Haken bei Mitgliedern weg, die ausgeschlossen werden sollen (ihre Verfügbarkeit wird dann ignoriert).
etf-member-priority = Mitgliederpriorität
etf-member-priority-help = Mitglieder mit höherer Priorität erhalten Buchungen zuerst, sofern verfügbar. Bei gleicher Priorität entscheidet die Zahl der jüngsten Buchungen.
etf-member-timezone-title = Zeitzone des Mitglieds. Seine persönlichen Arbeitszeiten werden in dieser Zeitzone ausgelegt.
etf-priority-high = Hoch
etf-priority-medium = Mittel
etf-priority-low = Niedrig
etf-section-availability = Verfügbarkeit
etf-timezone-help = Die Zeiten unten werden in dieser Zeitzone ausgelegt. Wähle bei Team-Terminarten die Arbeitszeitzone des Teams (nicht zwingend die der erstellenden Person).
etf-reset-default = Auf meine Standardwerte zurücksetzen
etf-reset-default-title = Diese Zeiten durch die Standardverfügbarkeit aus deinem Profil ersetzen
etf-availability-prefilled = Vorbelegt aus deiner { $link }. Du kannst sie hier für diese Terminart überschreiben.
etf-availability-prefilled-link = Standardverfügbarkeit
etf-section-buffers = Puffer & Vorlaufzeit
etf-buffer-before = Puffer davor (Min.)
etf-buffer-after = Puffer danach (Min.)
etf-min-notice = Mindestvorlaufzeit
etf-min-notice-help = Wie lange im Voraus gebucht werden muss.
etf-section-limits = Buchungslimits
etf-first-slot-only = Ein Zeitfenster pro Tag
etf-first-slot-only-help = Nur die früheste verfügbare Zeit je Tag anzeigen.
etf-freq-limit = Buchungshäufigkeit begrenzen
etf-freq-limit-help = Begrenzen, wie oft dieser Termin pro Zeitraum gebucht werden kann.
etf-add-limit = Limit hinzufügen
etf-section-options = Buchungsoptionen
etf-requires-confirmation = Bestätigung erforderlich
etf-requires-confirmation-help = Buchungen bleiben ausstehend, bis du sie im Dashboard bestätigst.
etf-sms = SMS-Benachrichtigungen
etf-sms-off = Aus, keine Telefonnummer abfragen
etf-sms-optional = Optional, Gäste können eine Nummer angeben
etf-sms-required = Pflicht, Gäste müssen eine Nummer angeben
etf-sms-help = Schickt dem Gast zusätzlich zur E-Mail eine SMS, wenn seine Buchung bestätigt, verschoben oder storniert wird oder kurz bevorsteht. Gäste, die das Feld leer lassen, erhalten einfach keine SMS. Erfordert ein SMS-Gateway im { $link }.
etf-admin-panel-link = Administrationsbereich
etf-additional-guests = Weitere Teilnehmende
etf-guests-none = Gäste können niemanden hinzufügen
etf-additional-guests-help = Der buchenden Person erlauben, weitere Teilnehmende einzuladen, die die Kalendereinladung erhalten.
etf-default-view = Standard-Kalenderansicht
etf-view-month = Monat — Kalenderraster mit Zeitfensterliste
etf-view-week = Woche — Spalten für 7 Tage mit Zeitfenstern
etf-view-column = Spalte — Tage mit direkt aufgeführten Zeitfenstern
etf-view-week-short = Wochen-
etf-view-column-short = Spalten-
etf-default-view-help = Die Ansicht, die Gäste zuerst sehen. Gäste können jederzeit wechseln.
etf-conflict-calendars = Kalender für Konfliktprüfung
etf-conflict-calendars-help = Wähle, welche Kalender auf Konflikte geprüft werden. Ohne Auswahl werden deine Kalender geprüft, die als belegt zählen, nicht aber von Kollegen geteilte: Wähle alle Kalender aus, die blockieren sollen, auch geteilte.
etf-no-resources = Noch keine geteilten Ressourcen eingerichtet. Lege im { $link } eine an (Demolabor, Besprechungsraum), um sie hier zu verlangen.
etf-section-access = Zugriff & Benachrichtigungen
etf-visibility-public = Öffentlich — auf deinem Profil sichtbar
etf-visibility-internal = Intern — alle Kolleginnen und Kollegen können Einladungslinks erzeugen
etf-visibility-private = Privat — nur per Einladungslink
etf-visibility-help = Legt fest, wer diese Terminart sehen und buchen kann.
etf-vis-internal = Intern
etf-reminder = Buchungserinnerung
etf-reminder-none = Keine Erinnerung
etf-reminder-help = Vor dem Termin eine Erinnerungs-E-Mail an dich und deinen Gast senden.
etf-guest-footer = E-Mail-Fußzeile
etf-guest-footer-placeholder = z. B. Fragen? Telefon: +49 30 1234567
etf-guest-footer-help = Wird am Ende jeder E-Mail eingefügt, die der Gast zu dieser Buchung erhält, in jeder Sprache. Ersetzt die vom Administrator festgelegte Standard-Fußzeile. Nur fett, kursiv und Links.
etf-dynamic-group = Dynamischer Gruppenlink
etf-dynamic-group-help = Erstelle einen spontanen Terminlink, der die Verfügbarkeit von dir und anderen Benutzern prüft.
etf-dynamic-group-search = Nach einem Benutzer zum Hinzufügen suchen...
etf-dynamic-group-note = Es werden nur Benutzer angezeigt, die dynamische Gruppenlinks erlauben.
etf-dynamic-group-url = URL des Gruppenlinks
etf-watcher-teams = Beobachtende Teams
etf-watcher-teams-help = Ausgewählte Teams werden bei neuen Buchungen benachrichtigt. Mitglieder können eine Buchung übernehmen, um daran teilzunehmen.
etf-save = Änderungen speichern
etf-create = Terminart erstellen
etf-js-loading = Wird geladen...
etf-js-no-default = Kein Standard gesetzt
etf-js-reset-done = Zurückgesetzt!
etf-js-error = Fehler
etf-js-remove-limit = Limit entfernen
etf-period-day = Pro Tag
etf-period-week = Pro Woche
etf-period-month = Pro Monat
etf-period-year = Pro Jahr

# Event type form: runtime summary hints (templates/event_type_form.html)

# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = Keine Tage festgelegt
etf-hint-every-day = Täglich
etf-fmt-day-one = %1 Tag
etf-fmt-day-other = %1 Tage
etf-fmt-hours = %1 Std.
etf-fmt-minutes = %1 Min.
etf-hint-buffer-both = %1 Min. davor, %2 Min. danach
etf-hint-buffer-before = %1 Min. Puffer davor
etf-hint-buffer-after = %1 Min. Puffer danach
etf-hint-notice = %1 Vorlaufzeit
etf-hint-no-buffers = Keine Puffer, jederzeit buchbar
etf-hint-max = Max. %1
etf-hint-period-day = /Tag
etf-hint-period-week = /Woche
etf-hint-period-month = /Monat
etf-hint-period-year = /Jahr
etf-hint-no-limits = Keine Limits
etf-hint-confirmation-required = Bestätigung erforderlich
etf-hint-auto-confirmed = Automatisch bestätigt
etf-hint-extra-guests-one = bis zu %1 weitere Person
etf-hint-extra-guests-other = bis zu %1 weitere Personen
etf-hint-view = %1Ansicht
etf-hint-reminder = Erinnerung %1 vorher
etf-hint-no-reminder = keine Erinnerung

etf-guests-up-to =
    { $count ->
        [one] Bis zu { $count } weitere Person
       *[other] Bis zu { $count } weitere Personen
    }

etf-reminder-hours =
    { $count ->
        [one] { $count } Stunde vorher
       *[other] { $count } Stunden vorher
    }

etf-reminder-days =
    { $count ->
        [one] { $count } Tag vorher
       *[other] { $count } Tage vorher
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Du erstellst eine <strong>öffentliche</strong> Terminart &mdash; jeder mit dem Link kann buchen.
etf-preset-private = Du erstellst eine <strong>private</strong> Terminart &mdash; nur eingeladene Personen können buchen.
etf-preset-internal = Du erstellst eine <strong>interne</strong> Terminart &mdash; alle Kolleginnen und Kollegen können den Buchungslink teilen.
etf-preset-team = Du erstellst eine <strong>Team-Terminart</strong> &mdash; Buchungen werden auf die Teammitglieder verteilt.
etf-pattern-hint = Optionales eigenes Muster. Platzhalter: <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Leer lassen, um den von der Administration gesetzten Organisationsstandard zu verwenden.
etf-pattern-random-warning = Dieses Muster enthält keinen <code>{"{"}random{"}"}</code>-Platzhalter. Zwei Buchungen dieser Terminart am selben Tag teilen sich denselben Raum, und der zweite Gast kann in das Gespräch des ersten geraten. Verwende feste Räume nur, wenn du genau das möchtest.
etf-webhook-hint = Die Termin-URL je Buchung wird von dem Webhook geholt, den die Administration unter Administration &rarr; Termin-Webhook eingerichtet hat. Hier ist keine URL nötig.

# Admin panel (templates/admin.html)

admin-page-title = Administration
admin-action-refused = Aktion abgelehnt:
admin-error-last-admin = Das ist der letzte aktive Admin. Stufe zuerst einen anderen Benutzer hoch oder aktiviere einen anderen Admin.
admin-logo = Firmenlogo
admin-logo-help = Wird auf öffentlichen Buchungsseiten angezeigt. PNG, JPEG, GIF oder WebP, max. 2 MB.
admin-logo-refused = Das Logo wurde nicht gespeichert. Lade ein PNG-, JPEG-, GIF- oder WebP-Bild mit höchstens 2 MB hoch.
admin-favicon = Favicon
admin-favicon-help = Erscheint im Browser-Tab und in Lesezeichen. Empfohlen: ein quadratisches PNG ab 32×32 Pixel, max. 512 KB. Ohne eigenes Favicon erscheint die calrs-Krabbe.
admin-favicon-refused = Das Favicon wurde nicht gespeichert. Lade ein PNG-, JPEG-, GIF- oder WebP-Bild mit höchstens 512 KB hoch.
admin-upload-save-failed = Die Datei konnte nicht im Datenverzeichnis des Servers gespeichert werden. Details stehen im Server-Log.

# User and team avatar uploads (#307)
avatar-upload-refused = Das Bild wurde nicht gespeichert. Lade ein PNG-, JPEG-, GIF- oder WebP-Bild mit höchstens 2 MB hoch.
avatar-upload-save-failed = Das Bild konnte auf dem Server nicht gespeichert werden. Versuche es später erneut oder bitte deinen Administrator, das Server-Log zu prüfen.
admin-company-link = Firmenlink
admin-company-link-help = Das Logo verlinkt auf öffentlichen Buchungsseiten auf diese URL. Leer lassen für keinen Link.
admin-theme = Design
admin-theme-help = Wähle ein Farbdesign für alle Seiten. Die Umschaltung zwischen Hell und Dunkel ist davon unabhängig; die Designs passen sich beiden Modi an.
admin-theme-default = Standard
admin-theme-default-desc = Klares Blau
admin-theme-nord-desc = Arktischer Frost
admin-theme-dracula-desc = Dunkles Violett
admin-theme-gruvbox-desc = Warmes Retro
admin-theme-solarized-desc = Ethans Klassiker
admin-theme-tokyo-desc = Neon-Stadtbild
admin-theme-custom = Benutzerdefiniert
admin-theme-custom-desc = Deine Farben
admin-custom-colors = Eigene Farben
admin-color-accent = Akzent
admin-color-accent-hover = Akzent beim Überfahren
admin-color-bg = Hintergrund
admin-color-surface = Oberfläche
admin-color-text = Text
admin-save-theme = Design speichern
admin-users = Benutzer
admin-user-filter = Nach Name oder E-Mail filtern…
admin-badge-admin = Administrator
admin-badge-disabled = deaktiviert
admin-impersonate = Identität annehmen
admin-demote = Herabstufen
admin-promote = Hochstufen
admin-disable = Deaktivieren
admin-enable = Aktivieren
admin-delete = Löschen
admin-no-users-match = Keine Benutzer entsprechen dem Filter.
admin-no-users = Noch keine Benutzer.
admin-groups = Gruppen
admin-group-filter = Nach Gruppenname filtern…
admin-group-name = Gruppenname
admin-weight = Gewicht:
admin-no-groups-match = Keine Gruppen entsprechen dem Filter.
admin-no-groups = Noch keine Gruppen synchronisiert. Gruppen werden automatisch von deinem OIDC-Anbieter übernommen.
admin-auth-settings = Anmeldeeinstellungen
admin-registration-enabled = Registrierung aktiviert
admin-allowed-domains = Zugelassene E-Mail-Domains
admin-allowed-domains-hint = (kommagetrennt, leer lassen für alle)
admin-save-auth = Anmeldeeinstellungen speichern
admin-system-settings = Systemeinstellungen
admin-base-url = Basis-URL
admin-base-url-help = Öffentliche URL dieser Instanz. Wird für OIDC-Weiterleitungen und Links in E-Mails verwendet (Bestätigen/Ablehnen, Stornieren, Erinnerungen).
admin-private-hosts = Positivliste privater Hosts
admin-private-hosts-help = Kommagetrennte Hostnamen, die für CalDAV-/EWS-Quellen auf private oder reservierte IP-Adressen zeigen dürfen (Ausnahme vom SSRF-Schutz). Trage nur Hosts ein, die du kontrollierst (etwa einen Kalenderserver im selben Docker-Netz). Leer lassen, um den Schutz für alle Hosts aktiv zu halten.
admin-german-formal = Anrede auf Deutsch
admin-german-formal-du = Informell (du)
admin-german-formal-sie = Formell (Sie)
admin-german-formal-help = Wie Seiten und E-Mails Besucher ansprechen, deren Browser Deutsch anfordert. Wer unter Profil & Einstellungen eine Sprache gewählt hat, behält diese.
admin-unset-env = Entferne die Umgebungsvariable, um dies hier zu bearbeiten.
admin-save-system = Systemeinstellungen speichern
admin-status = Status:
admin-status-enabled = aktiviert
admin-status-disabled = deaktiviert
admin-status-disabled-paren = (deaktiviert)
admin-status-configured = konfiguriert
admin-status-not-configured = nicht konfiguriert
admin-via-environment = (über die Umgebung)
admin-issuer = Aussteller:
admin-client-id = Client-ID:
admin-instance = Instanz:
admin-oidc-settings = OIDC-Einstellungen
admin-oidc-enabled = OIDC aktiviert
admin-issuer-url = Aussteller-URL
admin-client-id-label = Client-ID
admin-client-secret = Client-Secret
admin-keep-current-hint = (leer lassen, um den aktuellen Wert zu behalten)
admin-keep-current-set-hint = (leer lassen, um den aktuellen Wert zu behalten, derzeit gesetzt)
admin-keep-unchanged = Leer lassen, um nichts zu ändern
admin-oidc-auto-register = Neue Benutzer aus OIDC automatisch registrieren
admin-save-oidc = OIDC-Einstellungen speichern
admin-google = Google Kalender (OAuth2)
admin-save-google = Google-OAuth2-Einstellungen speichern
admin-captcha = Captcha
admin-instance-url = Instanz-URL
admin-site-key = Site-Key
admin-secret = Secret
admin-widget-url = URL des Widget-Skripts
admin-widget-url-help = Überschreiben, falls das CDN blockiert ist. Änderungen wirken sofort nach dem Speichern.
admin-wasm-url = URL der Widget-WASM-Datei
admin-wasm-url-help = Von dort lädt das Widget seinen Löser. Trage die WASM-Datei deines Cap-Servers ein, damit Buchungsseiten cdn.jsdelivr.net nicht mehr kontaktieren. Leer lassen für den Standardwert.
admin-captcha-wasm-url-invalid = Die URL der Widget-WASM-Datei muss eine absolute http(s)-URL sein.
admin-captcha-disable-help = Lass Instanz-URL, Site-Key und Secret leer, um das Captcha auf Buchungsseiten zu deaktivieren.
admin-save-captcha = Captcha-Einstellungen speichern
admin-resources = Ressourcen
admin-resources-help = Geteilte buchbare Ressourcen (Demolabor, Besprechungsräume) auf Basis eines Kalender-Feeds. An Terminarten gebunden blockiert eine belegte Ressource Buchungen.
admin-resource-stats = Termine im Cache: { $events } &middot; Gebunden an { $attached } Terminart(en)
admin-never = nie
admin-resource-sync-failed = (letzter Versuch fehlgeschlagen: { $error })
admin-writeback-enabled = Rückschreiben: aktiviert ({ $via })
admin-writeback-readonly = Rückschreiben: nur Lesen
admin-teams-allowed = Zugelassene Teams:
admin-teams-allowed-none = keine (nur globale Administratoren)
admin-sync-now = Jetzt synchronisieren
admin-test-write = Schreiben testen
admin-delete-resource-confirm = Diese Ressource löschen? Terminarten, die sie nutzen, prüfen sie dann nicht mehr.
admin-name = Name
admin-name-help = Leer lassen, um den Namen aus dem Feed zu übernehmen.
admin-feed-url = URL des ICS-Feeds (Veröffentlichungsadresse)
admin-feed-url-help = BlueMind: die öffentliche oder private Kalenderadresse des Ressourcenkalenders.
admin-caldav-url = URL der CalDAV-Sammlung (für das Rückschreiben)
admin-caldav-url-help = Optional. Bei BlueMind wird sie automatisch aus der Feed-URL abgeleitet.
admin-caldav-username = CalDAV-Benutzername
admin-caldav-password = CalDAV-Passwort
admin-resource-teams = Teams, die diese Ressource nutzen dürfen
admin-resource-teams-help = Teamadministratoren dieser Teams können die Ressource an ihre Team-Terminarten binden. Leer: nur globale Administratoren.
admin-no-teams = Noch keine Teams.
admin-save-resource = Ressource speichern
admin-add-resource = Ressource hinzufügen
admin-jitsi = Jitsi (automatisch erzeugte Terminlinks)
admin-jitsi-help = Wenn der Ort einer Terminart auf „Jitsi (automatisch erzeugter Raum)“ gesetzt ist, baut calrs für jede Buchung eine frische Raum-URL, indem es das Muster unten an deine Jitsi-Basis-URL anhängt. Ein externer API-Aufruf ist nicht nötig.
admin-display-name = Anzeigename
admin-jitsi-display-name-placeholder = z. B. Meet DYB
admin-jitsi-display-name-help = Wird Gästen in der Zeitfensterauswahl und im Buchungsformular angezeigt. Standard ist „Videoanruf“, wenn leer.
admin-room-pattern = Muster für Raumnamen
admin-jitsi-disable-help = Lass die Basis-URL leer, um die automatische Jitsi-Erzeugung zu deaktivieren.
admin-save-jitsi = Jitsi-Einstellungen speichern
admin-meeting-webhook = Termin-Webhook (eigener Anbieter)
admin-webhook-url = Webhook-URL
admin-webhook-display-name-placeholder = z. B. Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Wird Gästen statt des allgemeinen Hinweises „Videoanruf“ angezeigt.
admin-authentication = Authentifizierung
admin-auth-none = Keine
admin-auth-hmac = HMAC-SHA256 (Header X-Calrs-Signature)
admin-shared-secret = Gemeinsames Secret
admin-webhook-disable-help = Lass die URL leer, um den Termin-Webhook zu deaktivieren.
admin-save-webhook = Webhook-Einstellungen speichern
admin-smtp = SMTP-Einstellungen
admin-smtp-test-sent = Test-E-Mail gesendet.
admin-smtp-test-failed = Die Test-E-Mail konnte nicht gesendet werden. Prüfe die Serverprotokolle und deine SMTP-Einstellungen.
admin-smtp-env-error = Fehler in der SMTP-Konfiguration aus der Umgebung:
admin-smtp-host = Host:
admin-smtp-from = Absender:
admin-smtp-enabled = SMTP aktiviert
admin-host = Host
admin-port = Port
admin-tls-mode = TLS-Modus
admin-tls-starttls = STARTTLS (Port 587)
admin-tls-implicit = Implizites TLS (Port 465)
admin-tls-none = Keines, unverschlüsselt (nur lokaler MTA)
admin-smtp-username-hint = (leer lassen für ein Relay ohne Authentifizierung)
admin-from-email = Absenderadresse
admin-from-name = Absendername
admin-save-smtp = SMTP-Einstellungen speichern
admin-send-test-email = Test-E-Mail senden an
admin-send-test-email-hint = (standardmäßig deine Kontoadresse)
admin-send-test-email-btn = Test-E-Mail senden
admin-smtp-clear-confirm = Die in der Datenbank gespeicherte SMTP-Konfiguration löschen?
admin-clear-db-config = Datenbankkonfiguration löschen
admin-sms = SMS-Einstellungen
admin-sms-help = Optional. SMS werden nur für Buchungen von Terminarten gesendet, bei denen „SMS-Benachrichtigungen“ aktiviert ist, und nur wenn der Gast eine Telefonnummer hinterlassen hat.
admin-sms-test-sent = Testnachricht gesendet.
admin-sms-test-checked = Zugangsdaten akzeptiert.
admin-sms-test-error = Das SMS-Gateway hat die Anfrage abgelehnt.
admin-sms-captcha-warning = Das Buchungsformular ist öffentlich und die Empfängernummer kommt vom Gast. SMS ohne Captcha sind daher ein offenes Relay, das jemand anderes auf deine Kosten nutzen kann. Richte oben das Captcha ein und beschränke die Zielländer in den Einstellungen deines Gateways.
admin-sms-sent-today = Heute gesendet:
admin-sms-of-cap = von { $cap }
admin-sms-config-error = Fehler in der SMS-Konfiguration:
admin-sms-gateway = Gateway:
admin-sms-account = Konto:
admin-sms-sender = Absender:
admin-sms-enabled = SMS aktiviert
admin-sms-gateway-label = Gateway
admin-required-on-switch = Beim Wechsel des Gateways erforderlich
admin-sms-docs = API-Dokumentation von { $provider }
admin-sms-country = Standard-Ländervorwahl
admin-sms-country-hint = (wird verwendet, wenn Gäste eine lokale Telefonnummer eingeben)
admin-sms-daily-cap = Tageslimit
admin-sms-daily-cap-hint = (Nachrichten pro Tag für die gesamte Instanz, 0 für kein Limit)
admin-sms-daily-cap-help = Jenseits des Limits verschickt calrs keine SMS mehr und sendet weiter E-Mails, damit Buchungen nie am erschöpften SMS-Budget scheitern.
admin-save-sms = SMS-Einstellungen speichern
admin-send-test-sms = Testnachricht senden an
admin-send-test-sms-hint-check = (leer lassen, um nur die Zugangsdaten zu prüfen)
admin-send-test-sms-hint-e164 = (Format E.164)
admin-test-gateway = Gateway testen
admin-sms-clear-confirm = Die in der Datenbank gespeicherte SMS-Konfiguration löschen?
admin-sms-allow-all = Allen Benutzern erlauben, SMS für ihre Terminarten zu aktivieren
admin-sms-allow-all-help = Standardmäßig aus: SMS verbrauchen Guthaben des hier konfigurierten Kontos, daher dürfen nur Administratoren eine Terminart in einen SMS-Modus schalten.
admin-save-policy = Richtlinie speichern
admin-guest-footer = Fußzeile der E-Mails an Gäste
admin-guest-footer-help = Ein kurzer Text am Ende jeder E-Mail an Gäste und Teilnehmer: Bestätigungen, Erinnerungen, Absagen und so weiter. E-Mails an den Gastgeber und an das Teammitglied, das die Buchung übernommen hat, bleiben unverändert.
admin-guest-footer-default = Standard-Fußzeile
admin-guest-footer-default-help = Wird verwendet, wenn eine Terminart keine eigene Fußzeile hat. Derselbe Text geht in jeder Sprache hinaus. Nur fett, kursiv und Links.
admin-guest-footer-allow-all = Allen Benutzern erlauben, eine Fußzeile für ihre Terminarten festzulegen
admin-guest-footer-allow-all-help = Standardmäßig aus: Die Fußzeile geht im Namen dieser Instanz hinaus, daher dürfen nur Administratoren eine schreiben. Eine an einer Terminart festgelegte Fußzeile ersetzt die Standard-Fußzeile oben.
admin-save-guest-footer = Fußzeile speichern
admin-page-of = Seite %1 von %2
admin-show-more-js = %1 weitere anzeigen
admin-show-fewer = Weniger anzeigen

# Admin panel: overview, section tabs, users table (templates/admin.html)

admin-copy = Kopieren
admin-copied = Kopiert
admin-sections = Admin-Bereiche
admin-tab-overview = Übersicht
admin-tab-users = Benutzer
admin-tab-access = Anmeldung und Sicherheit
admin-tab-notifications = E-Mail und SMS
admin-tab-meetings = Meeting-Links
admin-tab-resources = Ressourcen
admin-tab-branding = Erscheinungsbild
admin-tab-system = System
admin-att-title = Handlungsbedarf
admin-mast-users = { $count ->
    [one] { $count } Benutzer
   *[other] { $count } Benutzer
}
admin-mast-bookings = { $count ->
    [one] { $count } Buchung
   *[other] { $count } Buchungen
}
admin-mast-attention = { $count ->
    [one] { $count } Punkt braucht Aufmerksamkeit.
   *[other] { $count } Punkte brauchen Aufmerksamkeit.
}
admin-mast-clear = Kein Handlungsbedarf.
admin-sev-failing = Fehler
admin-sev-missing = Fehlt
admin-sev-off = Aus
admin-sev-exposed = Offen
admin-sev-expiring = Läuft ab
admin-sev-stale = Veraltet
admin-fig-title = Kennzahlen
admin-chart-week-short = KW { $week }
admin-chart-now = jetzt
admin-chart-ended-key = Abgesagt oder abgelehnt, unter jeder Woche gezählt
admin-pct = { $value } %
admin-att-review = Prüfen
admin-att-smtp-error = Die SMTP-Konfiguration aus der Umgebung ist ungültig: Es werden keine E-Mails versendet.
admin-att-smtp-missing = E-Mail ist nicht konfiguriert: Gäste und Gastgeber erhalten keine Bestätigungen, Erinnerungen oder Absagen.
admin-att-smtp-disabled = Der E-Mail-Versand ist ausgeschaltet.
admin-att-sources-failing = Kalenderquellen mit fehlgeschlagener Synchronisierung
admin-att-writeback = Anstehende Buchungen, die im Kalender des Gastgebers fehlen
admin-att-sms-captcha = SMS ist ohne Captcha konfiguriert: Über das öffentliche Buchungsformular können kostenpflichtige Nachrichten verschickt werden.
admin-att-resources-failing = Ressourcen-Feeds mit fehlgeschlagener Synchronisierung
admin-att-pending-expiring = Ausstehende Anfragen, die innerhalb von 24 Stunden ablaufen
admin-att-sources-stale = Kalenderquellen, die seit über 24 Stunden nicht synchronisiert wurden
admin-att-base-url = Die öffentliche Basis-URL ist nicht gesetzt: Links in E-Mails und Single-Sign-on-Weiterleitungen funktionieren eventuell nicht.
admin-kpi-bookings = Buchungen, letzte 30 Tage
admin-kpi-prev = Vorherige 30 Tage: { $count }
admin-kpi-upcoming = Anstehende Termine
admin-kpi-upcoming-week = Nächste 7 Tage: { $count }
admin-kpi-pending = Ausstehende Anfragen
admin-kpi-pending-expiring = Laufen innerhalb von 24 Stunden ab: { $count }
admin-kpi-pending-none = Nichts wartet auf Freigabe
admin-kpi-pending-ok = Keine läuft innerhalb von 24 Stunden ab
admin-kpi-active = Aktive Benutzer
admin-kpi-active-sub = In den letzten 30 Tagen angemeldet, von allen aktivierten Konten
admin-chart-weekly = Buchungen pro Woche
admin-chart-weekly-help = Pro Woche getätigte Buchungen der letzten 12 Wochen, nach ihrem aktuellen Status.
admin-series-confirmed = Bestätigt
admin-series-pending = Ausstehend
admin-series-ended = Abgesagt oder abgelehnt
admin-chart-bar-label = Woche ab { $week }: { $confirmed } bestätigt, { $pending } ausstehend, { $ended } abgesagt oder abgelehnt
admin-chart-table = Als Tabelle anzeigen
admin-chart-week = Woche ab
admin-chart-total = Gesamt
admin-chart-empty = Keine Buchungen in den letzten 12 Wochen.
admin-status-title = Buchungen nach Status
admin-status-empty = Noch keine Buchungen.
admin-st-cancelled = Abgesagt
admin-st-declined = Abgelehnt
admin-svc-title = Dienste
admin-svc-email = E-Mail (SMTP)
admin-svc-sms = SMS
admin-svc-oidc = Single Sign-on (OIDC)
admin-svc-google = Google Kalender
admin-svc-microsoft = Microsoft 365 / Outlook.com
admin-svc-registration = Selbstregistrierung
admin-svc-jitsi = Jitsi-Meeting-Links
admin-svc-webhook = Meeting-Webhook
admin-svc-base-url = Öffentliche Basis-URL
admin-state-error = Fehler
admin-top-event-types = Meistgebuchte Terminarten
admin-top-hosts = Gefragteste Gastgeber
admin-top-help = Bestätigte Buchungen der letzten 90 Tage.
admin-top-empty = Keine bestätigten Buchungen in den letzten 90 Tagen.
admin-top-team = Team
admin-health-title = Kalenderzustand
admin-health-help = Synchronisierungsstand jeder Kalenderquelle und Buchungen, die nicht in den Kalender eines Gastgebers geschrieben werden konnten.
admin-health-healthy = Fehlerfrei
admin-health-failing = Fehlerhaft
admin-health-stale = Seit 24 h nicht synchronisiert
admin-health-disabled = Ausgeschaltet
admin-health-writeback-count = Fehlgeschlagene Rückschreibungen
admin-health-no-sources = Noch keine Kalenderquelle verbunden.
admin-health-failing-title = Quellen mit fehlgeschlagener Synchronisierung
admin-health-col-source = Quelle
admin-health-col-owner = Inhaber
admin-health-col-error = Letzter Fehler
admin-health-col-since = Fehlerhaft seit
admin-health-attempts = Fehlgeschlagene Versuche: { $count }
admin-health-owner-hint = Die Verfügbarkeit wird weiter aus der letzten erfolgreichen Synchronisierung berechnet. Der Inhaber sieht den Fehler auf seiner Seite der Kalenderquellen und erhält eine E-Mail, wenn er anhält.
admin-health-writeback-title = Buchungen, die im Kalender des Gastgebers fehlen
admin-health-col-booking = Buchung
admin-health-col-host = Gastgeber
admin-health-col-retry = Wiederholung
admin-health-retrying = wird wiederholt
admin-health-retry-exhausted = Wiederholungen ausgeschöpft
admin-health-resources-title = Ressourcen-Feeds mit fehlgeschlagener Synchronisierung
admin-inv-title = Bestand
admin-inv-users = Benutzerkonten
admin-inv-users-sub = Administratoren: { $admins }, deaktiviert: { $disabled }
admin-inv-signin-sub = Passwort: { $local }, SSO: { $sso }
admin-inv-new-users = Neue Konten, 30 Tage
admin-inv-mfa-sub = Lokale Konten mit eingerichtetem zweiten Faktor
admin-inv-teams = Teams
admin-inv-groups-sub = OIDC-Gruppen: { $count }
admin-inv-event-types = Terminarten
admin-inv-event-types-sub = Aktiviert: { $enabled }, Team: { $team }
admin-inv-bookings = Buchungen
admin-inv-bookings-sub = Bestätigt: { $count }
admin-inv-sources = Kalenderquellen
admin-inv-sources-sub = Aktiviert: { $count }
admin-inv-resources-failing = Fehlerhafte Feeds: { $count }
admin-inv-sms = SMS, letzte 30 Tage
admin-inv-sms-today = heute: { $count }
admin-users-filter-label = Benutzer filtern
admin-filter-all = Alle
admin-filter-admins = Administratoren
admin-filter-disabled = Deaktiviert
admin-filter-never = Nie angemeldet
admin-provider-local = Passwort
admin-provider-oidc = SSO
admin-col-user = Benutzer
admin-col-role = Rolle
admin-col-signin = Anmeldung
admin-col-last = Letzte Anmeldung
admin-col-created = Beigetreten
admin-col-actions = Aktionen
admin-user-actions = Aktionen für { $name }
admin-role-user = Mitglied
admin-badge-mfa = 2FA
admin-delete-confirm-btn = Endgültig löschen
admin-users-shown = %1 von %2 Benutzern

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Benutzer { $email } endgültig löschen?{"\u000A"}{"\u000A"}Damit werden sein Benutzerkonto, sein Planungsprofil, seine Kalenderquellen, seine Terminarten und alle Daten gelöscht, die ausschließlich ihm gehören. Vergangene Buchungen werden zusammen mit seinen Terminarten gelöscht.{"\u000A"}{"\u000A"}Bei OIDC-/SSO-Benutzern gilt: Ist die automatische Registrierung aktiv, wird diese Person bei der nächsten Anmeldung neu angelegt.{"\u000A"}{"\u000A"}Dies kann nicht rückgängig gemacht werden.
admin-system-settings-help = Öffentliche URL und Netzwerksicherheitseinstellungen. Sie lassen sich auch über die Umgebungsvariablen <code>CALRS_BASE_URL</code> und <code>CALRS_ALLOW_PRIVATE_HOSTS</code> setzen. Ist eine Umgebungsvariable gesetzt, hat sie <strong>Vorrang</strong> vor dem Wert unten.
admin-set-by-env = durch die Umgebung gesetzt ({ $var }), überschreibt den gespeicherten Wert
admin-google-help = Um die Google-Kalender-Integration zu aktivieren, erstelle OAuth2-Zugangsdaten in der <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Aktiviere die <strong>CalDAV API</strong> (Lesen belegter Zeiten und Zurückschreiben von Buchungen) und die <strong>Google Calendar API</strong> (Google-Meet-Links) und trage dann { $redirect_uri } als zugelassene Weiterleitungs-URI ein.
admin-room-pattern-help = Verfügbare Platzhalter: <code>{"{"}username{"}"}</code> (Gastgeber), <code>{"{"}event{"}"}</code> (Kürzel der Terminart), <code>{"{"}date{"}"}</code> (JJJJMMTT), <code>{"{"}random{"}"}</code> (8 Zeichen). Standard: { $default }.
admin-room-pattern-warning = Ohne <code>{"{"}random{"}"}</code> ist der Raumname vorhersehbar: Zwei Gäste, die dieselbe Terminart am selben Tag buchen, landen im selben Raum und sehen das Gespräch des jeweils anderen. Feste Räume sind zulässig (etwa ein persönlicher Raum je Gastgeber), aktiviere das aber nur, wenn du den Kompromiss verstehst.
admin-meeting-webhook-help = Wenn der Ort einer Terminart auf „Webhook (eigener Anbieter)“ gesetzt ist, sendet calrs die Buchungsdaten bei der Bestätigung per POST an diese URL und erwartet als Antwort einen JSON-Body <code>{"{"}"url": "https://..."{"}"}</code>.
admin-auth-hmac-help = Mit HMAC sendet calrs <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> über den unveränderten Anfragetext.
admin-tls-none-warning = Wähle <strong>Keines</strong> nur für ein Relay auf diesem Rechner, das kein STARTTLS anbietet oder dessen Zertifikat selbstsigniert ist. E-Mails und etwaige Zugangsdaten gehen dann unverschlüsselt über die Leitung.
admin-smtp-env-error-help = Korrigiere die Umgebungsvariablen <code>CALRS_SMTP_*</code> oder entferne sie, um SMTP hier über die Datenbank zu verwalten.
admin-smtp-env-managed = Verwaltet über <strong>Umgebungsvariablen</strong> (haben Vorrang vor der Datenbank). Ändere die Variablen <code>CALRS_SMTP_*</code> oder entferne sie, um SMTP hier zu verwalten.
admin-smtp-env-help = Alternativ über Umgebungsvariablen konfigurieren (sie haben Vorrang): <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> oder <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Nur <code>CALRS_SMTP_HOST</code> und <code>CALRS_SMTP_FROM_EMAIL</code> sind erforderlich; lass Benutzername und Passwort weg, um ohne Authentifizierung über einen lokalen MTA zu relayen.
admin-sms-env-error-help = Korrigiere die Umgebungsvariablen <code>CALRS_SMS_*</code> oder entferne sie, um SMS hier über die Datenbank zu verwalten.
admin-sms-env-managed = Verwaltet über <strong>Umgebungsvariablen</strong> (haben Vorrang vor der Datenbank). Ändere die Variablen <code>CALRS_SMS_*</code> oder entferne sie, um SMS hier zu verwalten.
admin-sms-env-help = Alternativ über Umgebungsvariablen konfigurieren (sie haben Vorrang): <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>Der Twilio-Testmodus ist aktiv</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Gäste erhalten Twilios vordefinierte Vorlage <code>sms_appointment_reminders</code> statt der echten Nachricht, und nur in deiner Twilio-Konsole verifizierte Nummern werden erreicht. Das ist eine Testhilfe für Testkonten. Entferne die Variable, bevor du Buchungen annimmst.

admin-show-more =
    { $count ->
        [one] { $count } weitere anzeigen
       *[other] { $count } weitere anzeigen
    }

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Wähle das Protokoll, das dein Server spricht. EWS richtet sich an lokal betriebene Exchange-Server 2019/2016/2013.

admin-sms-going-live = <strong>Vor dem Produktivbetrieb:</strong> Beschränke die Zielländer in deinem Gateway (bei Twilio heißt das Geo Permissions), halte das Konto auf Guthabenbasis ohne automatische Aufladung, und lass das Captcha aktiv. Diese drei Maßnahmen begrenzen zusammen, was ein SMS-Pumping-Versuch kosten kann.

troubleshoot-heading = Verfügbarkeitsdiagnose

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = Name und Kürzel sind erforderlich.
form-error-team-name-length = Der Name darf höchstens 255 Zeichen lang sein.
form-error-team-description-length = Die Beschreibung darf höchstens 5000 Zeichen lang sein.
form-error-slug-charset = Das Kürzel darf nur Kleinbuchstaben, Ziffern und Bindestriche enthalten.
form-error-slug-reserved = Dieses Kürzel ist reserviert. Bitte wähle ein anderes.
form-error-team-slug-taken = Ein Team mit diesem Kürzel existiert bereits.
form-error-title-required = Für die Erzeugung eines Kürzels ist ein Titel erforderlich.
form-error-event-type-slug-taken = Eine Terminart mit diesem Kürzel existiert bereits.
form-error-event-type-slug-taken-team = In diesem Team existiert bereits eine Terminart mit diesem Kürzel.
form-error-event-type-title-required = Ein Titel ist erforderlich.
form-error-event-type-title-length = Der Titel darf höchstens 255 Zeichen lang sein.
form-error-event-type-duration = Die Dauer muss eine ganze Zahl von Minuten zwischen 1 und 1440 (24 Stunden) sein.
form-error-event-type-number = Puffer, Mindestvorlaufzeit, Erinnerung und weitere Teilnehmende müssen ganze Zahlen ab 0 sein.
form-error-event-type-horizon = Der Buchungshorizont muss eine ganze Zahl von Tagen ab 0 sein. Lass das Feld leer für keine Begrenzung.
form-error-location-required = Angaben zum Ort sind erforderlich (etwa ein Videoanruf-Link, eine Telefonnummer oder eine Adresse).
form-error-bookable-range-order = Das Enddatum muss am oder nach dem Startdatum liegen.
form-error-bookable-range-invalid = Gib die Daten im Format JJJJ-MM-TT ein.
form-error-not-team-admin = Du bist kein Teamadministrator dieses Teams.
form-error-no-account = Kein Planungsprofil gefunden. Bitte wende dich an die Administration.
form-error-all-fields-required = Alle Felder sind erforderlich.
form-error-encryption = Verschlüsselungsfehler.
form-error-connection-failed = Verbindung fehlgeschlagen: { $error }. Prüfe URL und Zugangsdaten, oder aktiviere „Verbindungstest überspringen“, um trotzdem zu speichern.

# Settings page flash (src/web/mod.rs)

settings-saved = Einstellungen gespeichert.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = Der Name muss zwischen 1 und 255 Zeichen lang sein.
settings-error-username-length = Der Benutzername muss mindestens 2 Zeichen lang sein.
settings-error-username-taken = Dieser Benutzername ist bereits vergeben.
settings-error-booking-email = Bitte gib eine gültige Buchungs-E-Mail-Adresse ein.
settings-error-save-failed = Die Einstellungen konnten nicht gespeichert werden.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = Team nicht gefunden, oder du bist kein Teamadministrator.
error-team-not-found = Team nicht gefunden.
error-event-type-not-found = Terminart nicht gefunden.
error-decrypt-failed = Die gespeicherten Zugangsdaten konnten nicht entschlüsselt werden.
error-source-not-found = Quelle nicht gefunden.
error-source-no-password = Für diese Quelle ist kein Passwort gespeichert.
error-oauth-invalid-state = Ungültiger State-Parameter. Bitte versuche es erneut.
error-oauth-no-code = Kein Autorisierungscode empfangen.
error-oauth-not-configured = Google OAuth2 ist nicht konfiguriert.
error-no-scheduling-account = Kein Planungsprofil gefunden.
error-private-event-type-not-found = Private Terminart nicht gefunden.
error-access-denied = Zugriff verweigert.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = Dieses Zeitfenster ist nicht mehr verfügbar.
error-slot-too-soon = Dieses Zeitfenster ist nicht mehr verfügbar (zu kurzfristig).
error-slot-beyond-horizon = Dieses Zeitfenster liegt außerhalb des Buchungszeitraums.
error-slot-not-yet-bookable = Dieser Termin ist noch nicht buchbar.
error-invite-required = Diese Terminart erfordert einen Einladungslink.
error-invite-invalid = Ungültiger Einladungslink.
error-invite-expired = Dieser Einladungslink ist abgelaufen.
error-invite-used = Dieser Einladungslink wurde bereits verwendet.
error-invalid-date = Ungültiges Datum.
error-invalid-time = Ungültige Uhrzeit.
error-invalid-date-format = Ungültiges Datumsformat.
error-invalid-time-format = Ungültiges Uhrzeitformat.
error-too-many-bookings = Zu viele Buchungsversuche. Bitte versuche es in ein paar Minuten erneut.
error-too-many-requests = Zu viele Anfragen. Bitte versuche es später erneut.
error-no-members-available = Für dieses Zeitfenster ist kein Teammitglied verfügbar.
error-dynamic-group-public-only = Dynamische Gruppenlinks gibt es nur für öffentliche Terminarten.
error-user-not-found = Benutzer nicht gefunden.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Captcha-Prüfung fehlgeschlagen
bae-title-invalid-booking = Ungültige Buchungsangaben
bae-title-unavailable = Derzeit nicht verfügbar
bae-title-cannot-approve = Diese Buchung kann nicht bestätigt werden
bae-title-invalid-link = Ungültiger Link
bae-title-invalid-or-expired = Ungültiger oder abgelaufener Link
bae-title-booking-not-found = Buchung nicht gefunden
bae-title-already-approved = Bereits bestätigt
bae-title-already-declined = Bereits abgelehnt
bae-title-already-cancelled = Bereits storniert
bae-title-booking-cancelled = Buchung storniert
bae-title-booking-declined = Buchung abgelehnt

# Booking action error page: bodies

bae-body-go-back = Bitte geh zurück und versuche es erneut.
bae-body-unavailable = Der Gastgeber nimmt für dieses Datum keine weiteren Buchungen an. Bitte wähle ein anderes Datum oder schau später noch einmal vorbei.
bae-body-resource-gone = Eine erforderliche Ressource ist zu dieser Zeit nicht mehr verfügbar. Bitte den Gast, einen anderen Termin zu wählen.
bae-body-no-claim-token = Kein Übernahme-Token angegeben.
bae-body-claim-invalid = Dieser Übernahme-Link ist nicht mehr gültig.
bae-body-booking-gone = Diese Buchung existiert nicht mehr.
bae-body-decline-link-invalid = Dieser Ablehnungslink ist ungültig, abgelaufen, oder die Buchung wurde bereits bearbeitet.
bae-body-cancel-link-invalid = Dieser Stornierungslink ist ungültig, abgelaufen, oder die Buchung wurde bereits storniert.
bae-body-cancel-link-invalid-short = Dieser Stornierungslink ist ungültig oder abgelaufen.
bae-body-reschedule-link-invalid = Dieser Verschiebungslink ist ungültig, abgelaufen, oder die Buchung wurde bereits bearbeitet.
bae-body-approval-link-invalid = Dieser Bestätigungslink ist ungültig oder abgelaufen.
bae-body-already-approved = Diese Buchung wurde bereits bestätigt.
bae-body-already-declined = Diese Buchung wurde bereits abgelehnt.
bae-body-already-cancelled = Diese Buchung wurde bereits storniert.
bae-body-was-cancelled = Diese Buchung wurde storniert.
bae-body-declined-by-host = Diese Buchung wurde vom Gastgeber abgelehnt.

# Booking form validation (src/web/mod.rs)

validate-name-length = Der Name muss zwischen 1 und 255 Zeichen lang sein.
validate-email-length = Die E-Mail-Adresse muss zwischen 1 und 255 Zeichen lang sein.
validate-email-invalid = Bitte gib eine gültige E-Mail-Adresse ein.
validate-notes-length = Die Notizen dürfen höchstens 5000 Zeichen lang sein.
validate-date-too-far = Es kann nicht mehr als ein Jahr im Voraus gebucht werden.
validate-guest-footer-too-long = Die E-Mail-Fußzeile darf höchstens 500 Zeichen lang sein.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = Zusätzliche Gäste sind für diese Terminart nicht erlaubt.
guests-too-many =
    { $max ->
        [one] Du kannst höchstens einen zusätzlichen Gast hinzufügen.
       *[other] Du kannst höchstens { $max } zusätzliche Gäste hinzufügen.
    }
guests-invalid-email = Ungültige E-Mail-Adresse eines zusätzlichen Gastes: { $email }
dynamic-group-min-usernames = Dynamische Gruppenlinks brauchen mindestens zwei Benutzernamen.
dynamic-group-user-not-found = Benutzer „{ $username }“ nicht gefunden.
dynamic-group-user-opted-out = Benutzer „{ $username }“ hat dynamische Gruppenlinks nicht aktiviert.

error-slot-unavailable-member = Dieses Zeitfenster ist nicht mehr verfügbar ({ $username } hat einen Konflikt).

# Pending requests: slot hold, expiry and approval re-check (#234)

sms-expired = Anfrage abgelaufen: { $event }, { $date } um { $time } ({ $tz }). Keine Antwort erhalten, bitte wähle einen anderen Termin.
email-pending-expired-status = Anfrage abgelaufen
email-pending-expired-headline = Deine Buchungsanfrage ist abgelaufen: Der Gastgeber hat nicht rechtzeitig geantwortet. Bitte wähle einen anderen Termin.
email-pending-expired-action = Anderen Termin wählen
bae-body-host-busy = Der Gastgeber (oder ein benötigtes Teammitglied) ist zu dieser Zeit nicht mehr frei. Bitte den Gast, einen anderen Termin zu wählen.
bae-body-host-not-synced = Der Microsoft-Kalender eines Gastgebers ist noch nicht zum ersten Mal synchronisiert, calrs weiß also nicht, ob diese Zeit frei ist. Versuche es in einer Minute erneut oder sieh dir den Synchronisierungsfehler der Quelle unter Kalenderquellen an.
bookings-error-generic = Etwas ist schiefgelaufen.
bookings-error-no-bookable-dates = Dieser Termintyp hat keine buchbaren Tage mehr, daher könnte der Gast keine neue Zeit wählen. Verlängere zuerst seinen Buchungszeitraum.
bookings-error-reschedule-pending = Der Gast wurde bereits gebeten, eine neue Zeit zu wählen, und hat noch nicht geantwortet.
bookings-error-reschedule-resend-too-soon = Die Anfrage wurde dem Gast vor weniger als einer Stunde gesendet. Versuche es später erneut.
bookings-resend-reschedule = Anfrage erneut senden
bookings-resend-reschedule-hint = Dem Gast denselben Link erneut per E-Mail senden (höchstens einmal pro Stunde)
troubleshoot-detail-pending-request = { $title } (offene Anfrage)
troubleshoot-detail-claimed-booking = { $title } (übernommene Buchung)
troubleshoot-detail-unsupported-recurrence = { $calendar } (Wiederholungsregel, die calrs nicht lesen kann, ab ihrem Beginn als belegt gewertet)
etf-pending-blocks-slot = Offene Anfragen blockieren den Termin
etf-pending-blocks-slot-help = Solange eine Anfrage auf deine Antwort wartet, kann niemand sonst diese Zeit buchen, weder bei dieser noch bei deinen anderen Terminarten.
etf-pending-expiry = Ablauf von Anfragen
etf-pending-expiry-help = Eine Anfrage, die du bis dahin nicht beantwortet hast, wird automatisch abgelehnt und der Gast wird gebeten, einen anderen Termin zu wählen. Sie endet früher, wenn der Termin zu nah ist, als dass jemand anderes ihn noch buchen könnte. Zur Halbzeit bekommst du eine Erinnerung.
etf-pending-expiry-never = Nie

etf-pending-expiry-hours =
    { $hours ->
        [one] Nach { $hours } Stunde
       *[other] Nach { $hours } Stunden
    }

# Host, watcher and remaining guest emails (src/email.rs, #209)

email-footer-sent-by = Gesendet von
email-sender-via = { $host } über { $instance }
email-date-at-time = { $date } um { $time }
email-detail-phone = Telefon:
email-detail-expires = Läuft ab:
email-detail-new-date = Neues Datum:
email-detail-new-time = Neue Uhrzeit:
email-detail-previous = Bisher:
email-detail-requested = Angefragt:
email-detail-originally = Ursprünglich:
email-detail-organizer = Organisator:
email-detail-booked-by = Gebucht von:
email-detail-invited-by = Eingeladen von:
email-detail-message = Nachricht:
email-action-approve = Genehmigen
email-action-decline = Ablehnen
email-action-pick-new-time = Neuen Termin wählen
email-host-approve-line = Genehmigen: { $url }
email-host-decline-line = Ablehnen: { $url }
email-host-approval-dashboard = Melde dich in deinem Dashboard an, um diese Buchung zu bestätigen oder abzulehnen.
email-host-new-subject = Neue Buchung: { $event }, { $guest } ({ $date })
email-host-new-heading = Neue Buchung!
email-host-new-message = { $guest } hat einen Termin bei dir gebucht.
email-host-confirmed-subject = Bestätigt: { $event }, { $guest } ({ $date })
email-host-confirmed-heading = Buchung bestätigt
email-host-confirmed-message = Du hast die Buchung mit { $guest } genehmigt.
email-host-confirmed-footer = Der Termin wurde deinem Kalender hinzugefügt.
email-host-sync-failure-subject = Aktion nötig: Kalender für { $event } nicht aktualisiert ({ $date })
email-host-sync-failure-heading = Dein Kalender wurde nicht aktualisiert
email-host-sync-failure-intro = Diese Buchung wurde verschoben, aber calrs konnte den Termin in deinem Google Kalender nicht aktualisieren. Der Gast hat die neue Uhrzeit, dein Kalender zeigt noch die alte.
email-host-sync-failure-action = Öffne den Termin in Google Kalender und verschiebe ihn auf die oben genannte Uhrzeit. Verschiebe den vorhandenen Termin, statt ihn neu anzulegen, sonst funktioniert der Google-Meet-Link nicht mehr, den der Gast bereits hat.
email-writeback-failure-subject = Handlungsbedarf: { $event } ({ $date }) fehlt in deinem Kalender
email-writeback-failure-heading = Eine Buchung fehlt in deinem Kalender
email-writeback-failure-intro = calrs konnte diese Buchung auch nach mehreren Versuchen nicht in deinen Kalender eintragen. Die Buchung ist bestätigt und der Gast hat die Einladung.
email-writeback-failure-action = Trage den Termin selbst in deinen Kalender ein, damit du ihn nicht verpasst, und prüfe die Kalenderquelle in calrs. calrs versucht es noch einige Stunden weiter; du bekommst diese E-Mail für diese Buchung nicht noch einmal.
email-detail-error = Fehler:
email-source-failure-subject = Handlungsbedarf: Kalenderquelle „{ $source }“ synchronisiert nicht
email-source-failure-heading = Eine Kalenderquelle synchronisiert nicht mehr
email-source-failure-intro = calrs kann diese Kalenderquelle seit über einer Stunde nicht synchronisieren. Deine Buchungsseiten bieten weiter Termine an, die aber auf der letzten erfolgreichen Synchronisierung beruhen: Seitdem hinzugefügte oder verschobene Termine werden nicht berücksichtigt.
email-source-failure-action = Prüfe die Quelle auf der Seite Kalenderquellen: Meist ist ein abgelaufenes Passwort oder ein entzogener Zugriff die Ursache. Von dieser Quelle hörst du erst wieder, wenn sie sich erholt und danach erneut ausfällt.
email-detail-source = Kalenderquelle:
email-detail-failing-since = Fehlerhaft seit:
email-host-reminder-subject = Erinnerung: { $event }, { $guest } ({ $date })
email-host-reminder-heading = Anstehende Buchung
email-host-reminder-message = Erinnerung: Du hast bald eine Buchung mit { $guest }.
email-host-cancel-subject = Storniert: { $event }, { $guest } ({ $date })
email-host-cancel-message-by-host = Du hast diese Buchung storniert.
email-host-cancel-message-by-guest = { $guest } hat die Buchung storniert.
email-host-approval-subject = Aktion erforderlich: { $event }, { $guest } ({ $date })
email-host-approval-heading = Aktion erforderlich
email-host-approval-message = { $guest } möchte einen Termin bei dir buchen.
email-host-approval-reminder-subject = Erinnerung: { $guest } wartet auf deine Antwort ({ $event }, { $date })
email-host-approval-reminder-heading = Erinnerung: Aktion erforderlich
email-host-approval-reminder-message = { $guest } wartet noch auf deine Antwort. Wenn du nicht antwortest, wird die Anfrage am { $deadline } automatisch abgelehnt.
email-host-approval-footer = Du kannst dies auch in deinem Dashboard verwalten.
email-host-expired-subject = Abgelaufen: { $event } mit { $guest } ({ $date })
email-host-expired-heading = Buchungsanfrage abgelaufen
email-host-expired-message = Niemand hat die Anfrage von { $guest } vor Ablauf der Frist beantwortet. Sie wurde daher automatisch abgelehnt, und der Gast wurde gebeten, einen anderen Termin zu wählen.
email-host-expired-footer = Wie lange Anfragen warten, kannst du in den Einstellungen des Termintyps ändern.
email-host-reschedule-request-subject = Verschiebungsanfrage: { $event }, { $guest } ({ $email })
email-host-reschedule-request-message = { $guest } möchte die Buchung verschieben.
email-watcher-claim-subject = Zum Übernehmen: { $event }, { $guest } ({ $date })
email-watcher-claim-message = Eine neue Buchung kann übernommen werden. Klicke unten, um als Teilnehmer dazuzukommen.
email-watcher-claim-link = Diese Buchung übernehmen: { $url }
email-watcher-claim-footer = Du kannst sie auch in deinem Dashboard übernehmen.
email-claim-confirmed-subject = Buchung übernommen: { $event }, { $guest } ({ $date })
email-claim-confirmed-footer = Du wirst als Teilnehmer zu diesem Termin hinzugefügt.
email-attendee-greeting = Hallo,
email-attendee-message = Du wurdest als Teilnehmer zu einer Buchung hinzugefügt.
email-attendee-rescheduled-message = Eine Buchung, an der du teilnimmst, wurde auf einen neuen Termin verschoben.
email-attendee-cancelled-message = Eine Buchung, an der du teilnimmst, wurde storniert.
email-attendee-cancelled-subject = Storniert: { $event }
email-original-booking-cancelled = Deine ursprüngliche Buchung ist ebenfalls storniert.
email-pick-new-time-subject = Verschiebung: { $event }, bitte wähle einen neuen Termin
email-pick-new-time-message = { $host } muss deine Buchung verschieben. Bitte wähle einen neuen Termin.
email-pick-new-time-link = Neuen Termin wählen: { $url }
email-rescheduled-message = Deine Buchung wurde von { $host } verschoben.
email-reschedule-approved-message = { $host } hat den neuen Termin deiner Buchung bestätigt.
email-rescheduled-ics-attached-plain = Eine aktualisierte Kalendereinladung ist beigefügt.
email-rescheduled-ics-attached-html = Eine aktualisierte Kalendereinladung ist dieser E-Mail beigefügt.
email-need-to-reschedule = Verschieben? { $url }
email-invite-subject = { $host } hat dich eingeladen, einen Termin zu buchen: { $event }
email-invite-message = { $host } hat dich eingeladen, einen Termin zu buchen: { $event }
email-invite-link = Klicke auf den Link unten, um einen Termin zu wählen:
email-invite-action = Termin wählen
email-cancel-reason-calendar-deleted = Der Kalendertermin wurde vom Gastgeber gelöscht.

# Embed snippet generator (templates/embed.html)
embed-page-title = Einbetten — { $title }
embed-heading = Einbetten: { $title }
embed-subtitle = Erzeuge ein Code-Snippet, um diese Terminart auf deiner eigenen Website einzubetten.
embed-warn-internal = Achtung: Diese Terminart ist <strong>intern</strong>. Besucher der eingebetteten Seite brauchen einen gültigen Einladungslink, um zu buchen — das erzeugte Snippet enthält keinen. Stell die Sichtbarkeit auf öffentlich, wenn du direkt einbetten willst.
embed-warn-private = Achtung: Diese Terminart ist <strong>privat</strong>. Besucher der eingebetteten Seite brauchen einen gültigen Einladungslink, um zu buchen — das erzeugte Snippet enthält keinen. Stell die Sichtbarkeit auf öffentlich, wenn du direkt einbetten willst.
embed-warn-private-team = Achtung: Dieses Team ist <strong>privat</strong>. Besucher der eingebetteten Seite können ohne den Einladungslink des Teams nicht buchen — das erzeugte Snippet enthält keinen. Stell das Team auf öffentlich, wenn du direkt einbetten willst.
embed-need-username = Du musst einen <strong>Benutzernamen</strong> festlegen, bevor du diese Terminart einbetten kannst — der Einbettungslink lautet { $path }. { $link } und komm dann zurück.
embed-need-username-link = Leg in den Einstellungen einen Benutzernamen fest
embed-username-placeholder = benutzername
embed-tab-inline = Inline
embed-tab-floating = Schwebender Button
embed-tab-element = Klick auf Element
embed-layout = Layout
embed-theme = Design
embed-theme-auto = Automatisch (wie beim Besucher)
embed-brand-color = Markenfarbe
embed-use-org-color = Farbe der Organisation verwenden
embed-fix-time-format = Festes Zeitformat (blendet den 12h/24h-Schalter aus)
embed-fixed-tz = Zeiten in der Zeitzone des Termins anzeigen (blendet die Zeitzonenauswahl aus)
embed-fixed-tz-hint = Ideal für Termine vor Ort. Besucher sehen die Ortszeit des Termins, die Zeitzone wird genannt.
embed-button-text = Button-Text
embed-button-default = Termin buchen
embed-button-position = Button-Position
embed-pos-bottom-right = Unten rechts
embed-pos-bottom-left = Unten links
embed-pos-top-right = Oben rechts
embed-pos-top-left = Oben links
embed-button-colors = Button-Farben
embed-button-fg-placeholder = Text
embed-button-colors-hint = Hintergrund / Text
embed-show-icon = Kalendersymbol anzeigen
embed-element-help = Füge das Snippet einmal in deine Website ein und ergänze dann die angezeigten data-Attribute an jedem klickbaren Element (Button, Link, div), das das Buchungsfenster öffnen soll.
embed-snippet = Snippet
embed-loading = Wird geladen…
embed-preview = Live-Vorschau
embed-preview-back = zurück zur Vorschau
embed-preview-floating-hint = Klick auf den Button, um das Buchungsfenster zu simulieren
embed-preview-element-hint = Demo-Auslöser — füge auf deiner Website die data-Attribute aus dem Snippet einem beliebigen Element hinzu. Klick auf einen davon, um das Fenster auszuprobieren:
embed-preview-text-link = oder klick auf diesen Textlink
embed-snippet-attr-comment = Füge diese Attribute einem beliebigen klickbaren Element deiner Seite hinzu:

# Leftovers in partly translated pages (host reschedule, impersonation banner, slots, team settings)
host-resched-when = { $date } um { $start } – { $end }
base-impersonating = Du handelst als { $name }
slots-rescheduling-banner = Verschieben: { $title } (derzeit { $date } um { $time })
slots-invite-required-title = Für die Buchung ist ein Einladungslink nötig
team-settings-slug-url = Wird in der Team-URL verwendet: { $url }
team-event-type-no-teams = Es gibt noch keine Teams.
team-event-type-not-team-admin = Du bist in keinem Team Admin.

# Google Calendar connection errors (src/web/sources.rs)
google-error-not-configured = Die Google-Kalender-Integration ist nicht eingerichtet. Bitte deine Administratorin oder deinen Administrator, die Google-OAuth2-Zugangsdaten im Admin-Bereich einzutragen.
google-error-no-base-url = Die öffentliche Basis-URL ist nicht konfiguriert. Leg sie über die Umgebungsvariable CALRS_BASE_URL oder im Admin-Bereich unter Systemeinstellungen fest, bevor du OAuth2-Abläufe nutzt.
google-error-authorization = Google-Autorisierung fehlgeschlagen: { $error }
google-error-token-exchange = Token-Austausch fehlgeschlagen: { $error }
google-error-account-email = E-Mail-Adresse des Google-Kontos konnte nicht abgerufen werden: { $error }

# Admin panel notices (src/web/admin.rs)
admin-resource-feed-invalid = Ungültige Feed-URL: { $error }
admin-resource-feed-check-failed = Feed-Prüfung fehlgeschlagen: { $error }
admin-resource-caldav-invalid = Ungültige CalDAV-URL: { $error }
admin-resource-credentials-failed = Zugangsdaten konnten nicht gespeichert werden: { $error }
admin-resource-added = Ressource „{ $name }“ hinzugefügt. Termine im Cache: { $count }.
admin-resource-updated = Ressource aktualisiert. Termine im Cache: { $count }.
admin-resource-deleted = Ressource gelöscht.
admin-resource-not-found = Ressource nicht gefunden.
admin-resource-synced = Feed synchronisiert. Termine im Cache: { $count }.
admin-resource-sync-error = Synchronisierung fehlgeschlagen: { $error }
admin-resource-no-caldav-url = Keine CalDAV-URL konfiguriert, und aus der Feed-URL ließ sich keine ableiten.
admin-resource-no-write-credentials = Keine Schreib-Zugangsdaten verfügbar (kein Dienstkonto, und kein zustimmendes Mitglied hat eine passende Kalenderquelle).
admin-resource-write-passed = Schreibtest mit { $user } erfolgreich (angelegt: { $created }, aufgeräumt: { $cleaned }).
admin-resource-write-verified = bestätigt
admin-resource-write-unverified = nicht bestätigt
admin-resource-write-check-calendar = prüf den Ressourcenkalender
admin-resource-write-failed = Schreibtest fehlgeschlagen. Letzter Fehler: { $error }
admin-base-url-invalid = Die Basis-URL muss eine absolute http(s)-URL sein, z. B. https://cal.example.com
admin-company-link-invalid = Der Firmenlink muss mit http:// oder https:// beginnen
admin-smtp-host-required = Der SMTP-Host ist erforderlich.
admin-smtp-from-invalid = Gib eine gültige Absender-E-Mail-Adresse ein.
admin-smtp-port-invalid = Der SMTP-Port muss eine Zahl zwischen 1 und 65535 sein.
admin-smtp-tls-invalid = Der TLS-Modus muss „starttls“, „tls“ oder „none“ sein.
admin-smtp-password-required = Bei der ersten SMTP-Einrichtung ist ein Passwort erforderlich.
admin-sms-switch-needs-secret = Beim Wechsel des SMS-Gateways musst du die Zugangsdaten dieses Gateways eingeben.
admin-sms-settings-invalid = SMS-Einstellungen: { $error }.
admin-sms-sender-format = Ein numerischer SMS-Absender muss im internationalen Format angegeben werden, z. B. +4915112345678.
admin-sms-not-configured = SMS ist nicht eingerichtet.
admin-sms-load-failed = Die SMS-Konfiguration konnte nicht geladen werden.
admin-sms-check-accepted = Zugangsdaten vom Gateway akzeptiert.
admin-sms-bad-number = Das ist keine verwendbare Telefonnummer.
admin-sms-test-body = calrs-Testnachricht. Dein SMS-Gateway funktioniert.
admin-sms-test-accepted = Testnachricht vom Gateway angenommen.

## Local multi-factor authentication

mfa-title = Zwei-Faktor-Authentifizierung
mfa-invalid = Die Überprüfung ist fehlgeschlagen. Prüfe dein Passwort oder deinen Code. Jeder Code kann nur einmal verwendet werden; warte bei Bedarf auf einen neuen. Warte nach zu vielen Versuchen 15 Minuten. Starte neu, wenn die Einrichtung abgelaufen ist.
mfa-restart = Zurück zur Anmeldung
mfa-manage = Zwei-Faktor-Authentifizierung verwalten
mfa-recovery-help = Bewahre diese Wiederherstellungscodes sicher auf. Sie werden nur einmal angezeigt. Jeder Code ersetzt einmalig einen Authenticator-Code. Neue Wiederherstellungscodes ersetzen alle bisherigen Codes.
mfa-continue = Weiter zum Dashboard
mfa-continue-consent = Weiter zur Freigabeseite
mfa-enabled = Die Zwei-Faktor-Authentifizierung ist aktiviert.
mfa-disabled = Die Zwei-Faktor-Authentifizierung ist deaktiviert.
mfa-required-help = Dein Administrator verlangt Zwei-Faktor-Authentifizierung für lokale Konten. Du kannst sie nicht deaktivieren.
mfa-code = Authenticator-Code oder Wiederherstellungscode
mfa-regenerate = Neue Wiederherstellungscodes erstellen
mfa-disable = Zwei-Faktor-Authentifizierung deaktivieren
mfa-enable = Zwei-Faktor-Authentifizierung einrichten
mfa-changes-help = Änderungen erfordern dein Passwort. Zum Deaktivieren oder Ersetzen der Wiederherstellungscodes brauchst du auch einen Code; deine anderen Sitzungen werden dabei abgemeldet.
mfa-back = Zurück zu den Einstellungen
mfa-setup-help = Scanne diesen QR-Code mit deiner Authenticator-App oder gib den Schlüssel unten manuell ein. Gib einen sechsstelligen Code ein, um die Einrichtung abzuschließen. Die Einrichtung läuft nach 10 Minuten ab.
mfa-qr-alt = QR-Code zur Einrichtung der Authenticator-App
mfa-login-help = Gib einen Code aus deiner Authenticator-App oder einen gespeicherten Wiederherstellungscode ein. Diese Anmeldung läuft nach 10 Minuten ab.
mfa-totp-code = Sechsstelliger Authenticator-Code
mfa-verify = Überprüfen
mfa-policy-help = Diese Richtlinie gilt für lokale Konten. SSO verwendet die MFA des Identitätsanbieters. Lokale Administratoren müssen vor einer Änderung MFA einrichten. Die MFA-Pflicht meldet lokale Sitzungen ohne MFA-Prüfung ab und verlangt die Einrichtung bei der nächsten Anmeldung.
mfa-policy = MFA-Richtlinie für lokale Konten
mfa-optional = Optional
mfa-required = Erforderlich
mfa-save-policy = MFA-Richtlinie speichern

# First day of the week (#330)
week-start-label = Erster Tag der Woche
week-start-inherit-site = Standard der Website verwenden
week-start-inherit-owner = Standard des Eigentümers verwenden

## Booking directory and personal favorites

directory-title = Terminverzeichnis
directory-help = Finde den Terminlink eines Kollegen oder Teams. Markiere häufig geteilte Termine als Favoriten, um sie auf deinem Dashboard zu sehen.
directory-search = Nach Person, Team oder Termintitel suchen
directory-find = Suchen
directory-favorites-only = Nur Favoriten
directory-favorites = Favorisierte Termine
directory-browse = Verzeichnis durchsuchen
directory-generate = Einladungslink erstellen
directory-invite-help = Einmalige Einladung, 7 Tage gültig.
directory-add = { $title } zu Favoriten hinzufügen
directory-remove = { $title } aus Favoriten entfernen
directory-added = Zu Favoriten hinzugefügt.
directory-removed = Aus Favoriten entfernt.
directory-error = Diese Aktion konnte nicht abgeschlossen werden. Lade die Seite neu und versuche es erneut.
directory-manual-copy = Kopiere diesen Link manuell:
directory-empty = Keine Termine gefunden. Versuche eine andere Suche.
directory-favorites-empty = Keine Favoriten gefunden. Markiere Termine im Verzeichnis als Favoriten, um sie hier zu sehen.
directory-with-hosts = Mit { $names }

microsoft-calendar-title = Microsoft 365 / Outlook.com-Kalender
microsoft-calendar-help = Verbinde deinen Microsoft-Kalender mit delegierter Zustimmung. Dein Microsoft-Konto kann vom calrs-Anmeldekonto abweichen. Für lokales Exchange verwende unten EWS.
microsoft-calendar-connect = Microsoft-Kalender verbinden
microsoft-calendar-unconfigured = Ein Administrator muss zuerst die Microsoft-Anwendung konfigurieren.
microsoft-admin-help = Registriere eine Webanwendung mit genau dieser Umleitungs-URI. Fordere delegierte Berechtigungen Calendars.ReadWrite, User.Read und offline_access an, keine Anwendungsberechtigungen. Jeder Benutzer verbindet sein eigenes Konto. Implizite Freigabe und öffentliche Clientflows sind nicht erforderlich.
microsoft-tenant-label = Microsoft-Mandant
microsoft-tenant-help = common erlaubt bewusst Geschäfts-, Schul- und Privatkonten, sofern die App-Registrierung beide unterstützt. organizations erlaubt nur Geschäfts-/Schulkonten, consumers nur Privatkonten. Eine Mandanten-UUID oder verifizierte Domäne beschränkt den Zugriff.
microsoft-admin-save = Microsoft-Einstellungen speichern
microsoft-error-impersonation = Du kannst den Microsoft-Kalender nicht verbinden, während du als ein anderer Benutzer angemeldet bist.
microsoft-error-start = Die Microsoft-Autorisierung konnte nicht gestartet werden. Prüfe die App-Konfiguration, die öffentliche HTTPS-Basis-URL und ob diese Quelle zu deinem Konto gehört.
microsoft-error-state = Der Microsoft-Autorisierungsstatus ist ungültig, abgelaufen oder bereits verwendet, oder deine Sitzung hat sich geändert. Starte die Verbindung erneut.
microsoft-error-consent = Die Microsoft-Autorisierung wurde nicht abgeschlossen. Starte erneut und erlaube Kalenderzugriff zum Lesen und Schreiben, Profilzugriff und Offlinezugriff.
microsoft-error-connect = Der Microsoft-Kalender konnte nicht verbunden werden. Prüfe Einstellungen und Berechtigungen. Wähle zum erneuten Verbinden das ursprüngliche Microsoft-Konto; füge für ein anderes Konto eine neue Quelle hinzu.
microsoft-error-sync = Microsoft ist verbunden, aber die Synchronisierung ist fehlgeschlagen. Prüfe, ob das Konto einen Exchange-Online- oder Outlook.com-Kalender hat. Kehre zu den Kalenderquellen zurück und synchronisiere oder verbinde erneut.
microsoft-error-selection = Microsoft ist verbunden, aber die Kalenderauswahl konnte nicht geladen werden. Kehre zu den Kalenderquellen zurück.
microsoft-error-settings = Die Microsoft-Einstellungen wurden nicht gespeichert. Prüfe Client-UUID, Mandant und Datenbankzugriff. Ein leeres Geheimnis behält das vorhandene bei.
microsoft-error-heading = Microsoft-Kalender nicht verbunden
microsoft-error-settings-heading = Microsoft-Einstellungen nicht gespeichert
microsoft-error-back-admin = Zurück zum Administrationsbereich
source-test-microsoft-ok = Verbindung zum Microsoft-Kalender OK.
source-test-microsoft-failed = Verbindung zum Microsoft-Kalender fehlgeschlagen. Prüfe den Kalenderzugriff oder verbinde neu.
source-test-microsoft-credentials = Die Zugangsdaten für den Microsoft-Kalender konnten nicht geladen werden. Verbinde dein Konto neu.
microsoft-callback-needs-https = Setze die öffentliche Basis-URL auf einen HTTPS-Ursprung ohne Pfad, um die zu registrierende Callback-URL zu sehen.
source-form-backend-ews = Microsoft Exchange lokal (EWS)

# Global Exchange (EWS) impersonation: admin card and managed sources
sources-admin-managed = Vom Admin verwaltet
sources-admin-managed-title = Vom Administrator bereitgestellt
sources-locked-notice = Dein Administrator verwaltet Exchange zentral. Das Hinzufügen weiterer Kalenderquellen ist deaktiviert.
sources-locked-by-admin = Das Hinzufügen persönlicher Kalenderquellen wurde vom Administrator deaktiviert. Dein Exchange-Kalender wird zentral verwaltet.
sources-managed-no-edit = Diese Kalenderquelle wurde vom Administrator bereitgestellt und kann nicht bearbeitet werden.
sources-managed-no-remove = Diese Kalenderquelle wurde vom Administrator bereitgestellt und kann nicht entfernt werden.
sources-managed-global-disabled = Diese Quelle wird vom Administrator verwaltet, aber das globale Exchange ist deaktiviert.
admin-ews-title = Exchange (EWS): Globale Impersonation
admin-ews-help = Richte einen einzelnen Exchange-Web-Services-Endpunkt und ein Dienstkonto mit der RBAC-Rolle <code>ApplicationImpersonation</code> ein. Der Kalender jedes Benutzers wird dann per Impersonation gelesen, ohne dass Zugangsdaten eingegeben werden müssen. PowerShell auf Exchange: <code>New-ManagementRoleAssignment -Name CalrsImpersonation -Role ApplicationImpersonation -User svc-calrs@deinedomain</code>.
admin-ews-endpoint = Endpunkt
admin-ews-managed-count = Verwaltete Quellen:
admin-ews-enable = Globales Exchange per Impersonation aktivieren
admin-ews-url = EWS-Endpunkt-URL
admin-ews-username = Benutzername des Dienstkontos
admin-ews-password = Passwort des Dienstkontos
admin-ews-password-keep = (leer lassen, um das aktuelle zu behalten)
admin-ews-password-unchanged = (unverändert)
admin-ews-password-placeholder = Passwort des Dienstkontos
admin-ews-domain = Postfach-Domain
admin-ews-optional = (optional)
admin-ews-domain-help = Fülle beide aus, wenn die Adressen, die calrs kennt (AD-UPN, OIDC-E-Mail), auf einer anderen Domain liegen als die Exchange-Postfächer. Nur eine Adresse in genau der Domain der Benutzer wird umgeschrieben, der lokale Teil bleibt erhalten: <code>alice@example.local</code> → <code>alice@example.com</code>. Adressen jeder anderen Domain werden unverändert verwendet. Lass beide leer, um die calrs-Adresse jedes Benutzers unverändert zu verwenden.
admin-ews-auto = Für alle Benutzer automatisch bereitstellen (bestehende + künftige)
admin-ews-auto-help = Wenn aktiviert, erhält jeder aktive Benutzer mit vertrauenswürdiger Adresse eine verwaltete Exchange-Quelle: mit calrs user create angelegte Konten, SSO-Konten, deren E-Mail der Identitätsanbieter verifiziert hat, und unten bestätigte Adressen. Beim Speichern läuft der Vorgang sofort.
admin-ews-lock = Normale Benutzer daran hindern, eigene Kalenderquellen hinzuzufügen
admin-ews-lock-help = Administratoren sind ausgenommen. Bereits von Benutzern hinzugefügte Quellen bleiben bestehen, bis du sie löschst.
admin-ews-disable-note = Solange die Funktion aus ist, bleiben die verwalteten Quellen erhalten, werden aber nicht synchronisiert, und keine Buchung wird in sie geschrieben. Beim erneuten Aktivieren laufen sie weiter.
admin-ews-remove = Verwaltete Quellen entfernen und das Passwort des Dienstkontos löschen
admin-ews-remove-confirm = Alle verwalteten Exchange-Quellen löschen, mit den Kalendern und belegten Zeiten, die calrs von ihnen gespeichert hat, und das gespeicherte Passwort des Dienstkontos? Globales Exchange wird ausgeschaltet. Bereits in die Postfächer geschriebene Buchungen bleiben dort.
admin-ews-notice-removed = Entfernte verwaltete Quellen: { $count }. Das Passwort des Dienstkontos ist gelöscht und globales Exchange ist aus. Bereits in die Postfächer geschriebene Buchungen bleiben dort.
admin-ews-notice-https-required = Der EWS-Endpunkt muss https:// verwenden: Das Passwort des Dienstkontos wird mit jeder Anfrage gesendet. Es wurde nichts gespeichert.
admin-ews-notice-password-cleared = Einstellungen gespeichert. Endpunkt oder Dienstkonto haben sich geändert, daher wurde das gespeicherte Passwort gelöscht: Es muss erneut eingegeben werden, bevor globales Exchange funktioniert.
admin-ews-notice-source-removed = Verwaltete Quelle entfernt, mit ihren gespeicherten Kalendern und belegten Zeiten.
admin-ews-row-remove = Entfernen
admin-ews-row-remove-confirm = Die verwaltete Exchange-Quelle von { $email } mit ihren gespeicherten Kalendern und belegten Zeiten entfernen? Die automatische Bereitstellung erstellt sie beim nächsten Speichern, bei der nächsten Bestätigung oder SSO-Anmeldung neu; ein gesperrtes Konto bleibt ohne.
admin-ews-trust-no-mailbox = keins (die Adresse bezeichnet kein Postfach)
admin-ews-notice-selections-reset = Das Postfach einiger verwalteter Quellen hat sich geändert, ihre Kalender werden daher neu erkannt. Ereignistypen mit zurückgesetzter Kalenderauswahl: { $count } (im Serverprotokoll aufgeführt).
admin-ews-notice-reconfirm = Die Domain-Ersetzung ordnet einige bestätigte Adressen jetzt einem anderen Postfach zu. Erneut zu bestätigende Adressen: { $count } (siehe Liste unten).
admin-ews-row-taken-over = Übernommen von { $user } am { $when }
admin-ews-notice-trust-changed = Nichts bestätigt: Die Adresse dieses Kontos oder das Postfach, das sie öffnet, hat sich seit der Anzeige der Seite geändert. Die Liste muss erneut geprüft werden.
sources-managed-user-untrusted = Diese Quelle wird vom Administrator verwaltet, aber die Adresse des Kontos ist noch nicht bestätigt, daher wird sie nicht synchronisiert.
sources-managed-user-suspended = Diese Quelle wird vom Administrator verwaltet, aber das Konto ist deaktiviert, daher wird sie nicht synchronisiert.
admin-ews-save = EWS-Einstellungen speichern
admin-ews-save-provision = Speichern &amp; jetzt für alle Benutzer bereitstellen
admin-ews-status-title = Synchronisierungsstatus pro Benutzer
admin-ews-status-summary = { $ok } von { $total } verwalteten Postfächern werden synchronisiert. Ein Benutzer ohne Exchange-Postfach (z. B. ein lokales Admin-Konto) zeigt immer einen Fehler an. Das ist zu erwarten.
admin-ews-col-user = Benutzer
admin-ews-col-mailbox = Postfach
admin-ews-col-last-sync = Letzte Synchronisierung
admin-ews-col-status = Status
admin-ews-row-disabled = deaktiviert
admin-ews-row-synced = Synchronisiert
admin-ews-row-never = Nie synchronisiert
admin-ews-row-no-mailbox = Kein Exchange-Postfach
admin-ews-notice-saved = Exchange-Einstellungen gespeichert.
admin-ews-notice-provisioned = Exchange-Einstellungen gespeichert. Erstellte verwaltete Quellen: { $count }.
admin-ews-notice-invalid-url = Die EWS-Endpunkt-URL ist ungültig (nur http(s), keine private Adresse außer auf der Freigabeliste). Es wurde nichts gespeichert.
admin-ews-notice-incomplete = Um globales Exchange zu aktivieren, gib die Endpunkt-URL, das Dienstkonto und dessen Passwort an. Es wurde nichts gespeichert.
admin-ews-notice-provision-failed = Einstellungen gespeichert, aber die Bereitstellung der verwalteten Quellen ist fehlgeschlagen. Siehe Serverprotokoll.
admin-ews-registration-warning = Die Selbstregistrierung ist offen. So erstellte Konten erhalten nie eine verwaltete Exchange-Quelle, auch nicht über „jetzt für alle Benutzer bereitstellen“: Nichts beweist, dass ihnen ihre Adresse gehört. Um einem davon ein Postfach zu geben, bestätige die Adresse unten oder führe calrs user trust-email aus, sobald du weißt, dass sie dieser Person gehört.
admin-ews-from-domain = Domain der Benutzer
admin-ews-notice-override-domains = Die Domain-Ersetzung braucht beide Domains, jeweils eine einfache Domain wie example.com: die, mit der sich deine Benutzer anmelden, und die ihrer Postfächer. Es wurde nichts gespeichert.
admin-ews-notice-trusted = Adresse bestätigt. Solange globales Exchange aktiv ist, wird die verwaltete Quelle des Benutzers jetzt erstellt: siehe Statustabelle.
admin-ews-row-suspended = Konto deaktiviert: nicht synchronisiert
admin-ews-row-untrusted = Adresse nicht bestätigt: nicht synchronisiert
admin-ews-row-conflict = Nicht bereitgestellt: Die Quelle eines anderen Benutzers liest dieses Postfach bereits
admin-ews-trust-title = Zu bestätigende Adressen
admin-ews-trust-help = Diese aktiven Konten in den erlaubten Domains haben eine Adresse, für die niemand bürgt (selbst registriert oder vor dieser Prüfung angelegt), und erhalten daher keine verwaltete Exchange-Quelle. Bestätige eine Adresse nur, wenn du weißt, dass sie dieser Person gehört: Damit liest calrs für sie den Kalender dieses Postfachs.
admin-ews-trust-confirm = Bestätigen
admin-ews-search = Nach Benutzer oder Postfach suchen
admin-ews-no-match = Kein verwaltetes Postfach und keine zu bestätigende Adresse passt zu dieser Suche.
admin-ews-conflicts-more = Postfach bereits von einem anderen Benutzer verwendet: { $shown } von { $total } angezeigt.
admin-ews-http-warning = Der Endpunkt verwendet http://: Das Passwort des Dienstkontos wird unverschlüsselt gesendet. Verwende besser https://.
# Time off (templates/time_off.html, src/web/time_off.rs, troubleshoot view)

nav-time-off = Abwesenheit
time-off-page-title = Abwesenheit
time-off-heading = Abwesenheit
time-off-subtitle = Zeiträume, in denen du keine Buchungen annimmst. Alle Terminarten, die du anbietest, persönliche wie die deiner Teams, richten sich danach. Gäste sehen nur, dass diese Zeiten nicht verfügbar sind.
time-off-upcoming = Geplant
time-off-empty = Keine Abwesenheit geplant.
time-off-kept-notice = Termine, die in einem Zeitraum bereits gebucht sind, bleiben bestehen: Eine Abwesenheit sagt nichts ab. Sage sie bei Bedarf unter Buchungen ab oder verschiebe sie.
time-off-pending-notice = Anfragen, die in einem Zeitraum auf deine Bestätigung warten, lassen sich nicht mehr annehmen: Lehne sie ab oder lösche zuerst die Abwesenheit.
time-off-in-progress = Läuft gerade
time-off-entered-in = Eingegeben in { $timezone }
time-off-delete = Löschen
time-off-delete-confirm = Diese Abwesenheit löschen? Der Zeitraum wird wieder buchbar.
time-off-add-heading = Abwesenheit hinzufügen
time-off-from = Von
time-off-to = Bis
time-off-time-optional = Uhrzeit (optional)
time-off-times-help = Lass die Uhrzeiten leer, um ganze Tage zu sperren, beide Daten eingeschlossen. Die Uhrzeiten gelten in deiner Zeitzone, { $timezone }.
time-off-label = Notiz (optional)
time-off-label-help = Nur du siehst sie, hier und unter Diagnose. Gäste und Teammitglieder sehen sie nie.
time-off-team-admins-notice = Die Administratoren deiner Teams und die der Instanz sehen Datum und Uhrzeit deiner Abwesenheiten, nie die Notiz.
time-off-add-submit = Abwesenheit hinzufügen
time-off-added = Abwesenheit hinzugefügt.
time-off-deleted = Abwesenheit gelöscht.
time-off-range = { $start } bis { $end }
time-off-at = { $date }, { $time }
time-off-same-day = { $date }, { $start } bis { $end }
time-off-from-time = { $date }, ab { $time }
time-off-until-time = { $date }, bis { $time }
time-off-impersonating = Du siehst diese Seite als { $name }. Die Abwesenheiten sind für dich hier nur lesbar, die Notizen sind ausgeblendet.
time-off-error-date = Gib die Daten als JJJJ-MM-TT und die Uhrzeiten als HH:MM ein.
time-off-error-order = Das Ende muss nach dem Beginn liegen.
time-off-error-too-long = Ein Zeitraum darf höchstens ein Jahr lang sein.
time-off-error-past = Dieser Zeitraum ist bereits vorbei.
time-off-error-label = Die Notiz darf höchstens 200 Zeichen lang sein.
time-off-error-too-many = Du hast bereits 100 Zeiträume geplant. Lösche einige, bevor du weitere hinzufügst.
time-off-hint = Um Daten für alle Terminarten auf einmal zu sperren, nutze { $link }.
troubleshoot-label-time-off = Abwesenheit
troubleshoot-legend-time-off = Abwesenheit

directory-all-favorites = Alle Favoriten
pager-label = Seiten
pager-range = { $from } bis { $to } von { $total }
pager-previous = Zurück
pager-next = Weiter
admin-users-search = Suchen
admin-figures-cached = Buchungszahlen vor { $seconds } s berechnet.

# OAuth connection consent (templates/auth/consent.html)
oauth-title = Eine Anwendung verbinden
oauth-intro = Dieser Anwendung Zugriff auf calrs als { $email } erlauben? Der Name wird vom Entwickler angegeben. Prüfe die Rückrufadresse, bevor du zustimmst.
oauth-events = Terminarten lesen, auf die du Zugriff hast
oauth-blocks = Deine belegten Kalenderzeiten lesen
oauth-invites = Einladungslinks mit deinen Berechtigungen erstellen
oauth-allow = Zugriff erlauben
oauth-deny = Ablehnen

# Connected apps (templates/dashboard_apps.html, admin user menu)

apps-page-title = Verbundene Anwendungen
apps-heading = Verbundene Anwendungen
apps-subtitle = Anwendungen, denen du erlaubt hast, in deinem Namen zu handeln, etwa KI-Assistenten, die über MCP verbunden sind. Ein Widerruf beendet ihren Zugriff sofort.
apps-approved = Genehmigt am { $date }
apps-last-used = Zuletzt verwendet am { $date }
apps-never-used = Noch nicht verwendet
apps-revoke = Widerrufen
apps-revoke-confirm = Zugriff für { $name } widerrufen? Die Anwendung muss erneut genehmigt werden, um sich wieder zu verbinden.
apps-revoked = Zugriff widerrufen.
apps-empty = Keine Anwendung verbunden. Ein MCP-Client bittet beim ersten Verbinden um deine Zustimmung.
oauth-slots = Buchbare Zeiten der Terminarten lesen, die du verwaltest
oauth-bookings-read = Buchungen lesen, bei denen du Gastgeber bist, mit den Angaben der Gäste
oauth-bookings-write = Auf Terminarten, die du verwaltest, buchen, stornieren und verschieben, mit E-Mails an die Gäste
oauth-api-closed = Diese Anwendung möchte die REST-API nutzen, die auf dieser Instanz nur Administratoren verwenden dürfen. Sie kann nicht genehmigt werden.
api-tokens-page-title = API-Tokens
api-tokens-heading = API-Tokens
api-tokens-subtitle = Tokens, mit denen deine eigenen Skripte die REST-API von calrs in deinem Namen aufrufen. Ein Token gilt, bis es abläuft oder du es widerrufst.
api-tokens-not-allowed = Auf dieser Instanz können nur Administratoren API-Tokens erstellen.
api-tokens-new = Neues Token
api-tokens-name = Name
api-tokens-name-hint = Wofür es ist, zum Beispiel CRM-Abgleich
api-tokens-scopes = Berechtigungen
api-tokens-expiry = Läuft ab
api-tokens-expiry-7 = In 7 Tagen
api-tokens-expiry-30 = In 30 Tagen
api-tokens-expiry-90 = In 90 Tagen
api-tokens-expiry-365 = In einem Jahr
api-tokens-expiry-never = Nie
api-tokens-create = Token erstellen
api-tokens-created = Kopiere dieses Token jetzt: Es wird nur einmal angezeigt.
api-tokens-created-on = Erstellt am { $date }
api-tokens-expires-on = Läuft ab am { $date }
api-tokens-never-expires = Läuft nie ab
api-tokens-revoke-confirm = { $name } widerrufen? Skripte, die es verwenden, funktionieren sofort nicht mehr.
api-tokens-revoked = Token widerrufen.
api-tokens-empty = Noch kein API-Token.
api-tokens-error-name = Gib dem Token einen Namen mit höchstens 100 Zeichen.
api-tokens-error-scope = Wähle mindestens eine Berechtigung.
api-tokens-error-limit = Du hast bereits 20 Tokens. Widerrufe zuerst eines.
api-tokens-docs = Wie ein Token verwendet wird, steht im Kapitel REST-API der calrs-Dokumentation.
api-docs-title = API-Referenz
api-docs-subtitle = Alle Endpunkte der REST-API von calrs, beschrieben von dieser Instanz selbst.
api-docs-closed = Auf dieser Instanz können nur Administratoren die REST-API nutzen.
api-docs-try = Um eine Anfrage auszuprobieren, wähle Authorize und füge ein persönliches Zugriffstoken ein. Diese Seite sendet deine Sitzung nie an die API.
api-docs-tokens = API-Tokens
api-docs-spec = OpenAPI-Dokument (JSON)
api-docs-guide = Anleitung zur REST-API
api-docs-noscript = Die interaktive Referenz benötigt JavaScript. Das OpenAPI-Dokument beschreibt dieselbe API.
admin-api-tokens = API-Tokens
admin-api-tokens-allow-all = Allen Benutzern die REST-API erlauben
admin-api-tokens-allow-all-help = Standardmäßig aus: Nur Administratoren können persönliche Zugriffstokens erstellen oder Anwendungen über OAuth mit der REST-API verbinden, da beides Gastdaten liest. Ausschalten setzt die Tokens und API-Anwendungen anderer Benutzer aus, bis es wieder eingeschaltet wird. MCP-Clients sind nicht betroffen.
admin-revoke-apps = Verbundene Anwendungen widerrufen ({ $count })

# Write destination per event type (#47)
etf-write-destination = Bestätigte Buchungen schreiben nach
etf-write-destination-default = Standard (Einstellung der Quelle)
etf-write-destination-help = Nur für diese Terminart. Standardmäßig landen sie im Schreibkalender jeder deiner Quellen. Bei einer Team-Terminart gilt das nur für die Buchungen, die du übernimmst.
etf-write-destination-team = Jedes Mitglied wählt unter { $link }, wohin die Buchungen geschrieben werden, die es übernimmt.
sources-destinations-heading = Buchungen nach Terminart
sources-destinations-help = Wähle für jede Terminart den Kalender, in den ihre bestätigten Buchungen geschrieben werden. Bei einer Team-Terminart gilt das für die Buchungen, die du übernimmst; die anderen Mitglieder wählen ihren eigenen. Bereits geschriebene Buchungen ziehen beim nächsten Schreiben um: bei einer Verschiebung, Freigabe, Wiederholung oder Übernahme.
sources-destinations-saved = Gespeichert. Neue Buchungen landen in den hier gewählten Kalendern.
write-destination-not-yours = Nichts wurde gespeichert: Wähle einen deiner eigenen Kalender, für eine Terminart, die du übernimmst.
write-destination-unavailable = { $name } (nicht verfügbar)
write-destination-refused = Dieser Kalender hat die letzte Buchung abgelehnt ({ $error }); sie wurde stattdessen nach der Einstellung der Quelle geschrieben. Die nächste Buchung versucht es erneut.
# Booking webhooks (templates/webhooks.html, admin card, #168)
webhooks-title = Webhooks
webhooks-back = Zurück zu den Terminarten
webhooks-subtitle = calrs sendet eine signierte JSON-Nachricht an diese Adressen, wenn eine Buchung angelegt, angefragt, bestätigt, abgelehnt, abgelaufen, storniert oder verschoben wird.
webhooks-not-allowed = Webhooks sind auf dieser Instanz Administratoren vorbehalten.
webhooks-endpoints = Endpunkte
webhooks-empty = Noch keine Webhooks. Füge unten einen hinzu, damit calrs ein anderes System über deine Buchungen informiert.
webhooks-scope = Ereignisse senden für
webhooks-scope-personal = Alle meine persönlichen Terminarten
webhooks-scope-event-type = Terminart: { $title }
webhooks-scope-team-event-type = Terminart: { $title } ({ $team })
webhooks-added-by = Hinzugefügt von { $name }
webhooks-state-off = Ausgeschaltet
webhooks-never-sent = Noch nichts gesendet
webhooks-state-ok = Letzte Zustellung erfolgreich
webhooks-state-failing = Letzte Zustellung fehlgeschlagen
webhooks-queued =
    { $count ->
        [one] 1 Zustellung wartet auf einen neuen Versuch
       *[other] { $count } Zustellungen warten auf einen neuen Versuch
    }
webhooks-gave-up =
    { $count ->
        [one] 1 Zustellung nach allen Versuchen aufgegeben
       *[other] { $count } Zustellungen nach allen Versuchen aufgegeben
    }
webhooks-send-test = Test senden
webhooks-enable = Einschalten
webhooks-disable = Ausschalten
webhooks-delete = Entfernen
webhooks-delete-confirm = Diesen Webhook entfernen? Noch wartende Zustellungen werden verworfen.
webhooks-edit = Bearbeiten
webhooks-url = URL
webhooks-url-hint = (https)
webhooks-secret = Signaturgeheimnis
webhooks-secret-hint = (mindestens { $min } Zeichen, signiert jede Anfrage, wird nie wieder angezeigt)
webhooks-save = Speichern
webhooks-add = Webhook hinzufügen
webhooks-add-submit = Webhook hinzufügen
webhooks-privacy-note = Jede Nachricht enthält Name, E-Mail, Telefonnummer und Notizen des Gastes. Sende sie nur an ein System, dem du diese Daten anvertraust.
webhooks-notice-created = Webhook hinzugefügt. Prüfe mit „Test senden“, ob er ankommt.
webhooks-notice-updated = Webhook gespeichert.
webhooks-notice-deleted = Webhook entfernt.
webhooks-notice-enabled = Webhook eingeschaltet.
webhooks-notice-disabled = Webhook ausgeschaltet. Noch wartende Zustellungen wurden verworfen.
webhooks-notice-test-ok = Test zugestellt: Der Endpunkt hat mit einem 2xx-Status geantwortet.
webhooks-error-url = Gib eine vollständige URL ein, zum Beispiel https://hooks.example.com/calrs.
webhooks-error-https = Die URL muss mit https:// beginnen. Einfaches http geht nur für einen Host auf der Liste erlaubter privater Hosts.
webhooks-error-credentials = Entferne Benutzername und Passwort aus der URL: Das Signaturgeheimnis authentifiziert calrs.
webhooks-error-secret = Das Signaturgeheimnis muss 16 bis 256 Zeichen lang sein.
webhooks-error-scope = Du kannst dieser Terminart keinen Webhook hinzufügen.
webhooks-error-limit = Für diesen Bereich gibt es bereits die maximal 10 Webhooks.
webhooks-error-test = Der Test wurde nicht zugestellt. Der Grund steht beim Webhook.
admin-webhooks = Buchungs-Webhooks
admin-webhooks-help =
    { $count ->
        [one] Auf dieser Instanz ist 1 Webhook eingerichtet.
       *[other] Auf dieser Instanz sind { $count } Webhooks eingerichtet.
    }
admin-webhooks-allow-all = Allen Benutzern erlauben, Webhooks hinzuzufügen
admin-webhooks-allow-all-help = Standardmäßig aus: Ein Webhook sendet Namen, E-Mail-Adressen und Telefonnummern von Gästen an eine vom Benutzer gewählte Adresse, daher dürfen nur Administratoren einen hinzufügen. Ausschalten stoppt die Webhooks, die andere Benutzer hinzugefügt haben.
admin-webhooks-manage = Deine Webhooks verwalten
webhooks-events = Ereignisse
webhooks-events-all = Alle Ereignisse
webhooks-event-created = Gebucht (bestätigt)
webhooks-event-requested = Angefragt (wartet auf Bestätigung)
webhooks-event-confirmed = Bestätigt
webhooks-event-declined = Abgelehnt
webhooks-event-expired = Unbeantwortet abgelaufen
webhooks-event-cancelled = Storniert
webhooks-event-rescheduled = Verschoben
webhooks-error-events = Wähle mindestens ein Ereignis aus.
webhooks-error-test-limit = Zu viele Tests in kurzer Zeit. Warte ein paar Minuten und versuche es erneut.
webhooks-added-by-former = Hinzugefügt von einem ehemaligen Mitglied
webhooks-signature-help = Jede Anfrage ist signiert: <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> ist ein HMAC-SHA256 des unveränderten Inhalts mit dem Signaturgeheimnis als Schlüssel. Inhalt und die Prüfungen auf Empfängerseite beschreibt die Seite Webhooks der calrs-Dokumentation.
webhooks-state-suspended = Ausgesetzt: Die Person, die ihn hinzugefügt hat, kann diese Terminart nicht mehr verwalten. Übernimm ihn, um ihn fortzusetzen.
webhooks-adopt = Übernehmen
webhooks-notice-adopted = Webhook übernommen und fortgesetzt.
webhooks-event-claimed = Von einem Beobachter übernommen
webhooks-error-address = Diese Adresse kann nicht verwendet werden: calrs sendet nur an öffentliche Internetadressen, außer der Host steht auf der Liste erlaubter privater Hosts.
webhooks-adopt-help = Wenn du ihn übernimmst, gehört er dir und läuft wieder. Bestätige die Adresse, an die er sendet, und lege ein neues Signaturgeheimnis fest: Die Person, die ihn hinzugefügt hat, kennt das alte womöglich. Noch wartende Zustellungen werden verworfen.
webhooks-error-not-suspended = Nur ein ausgesetzter Webhook kann übernommen werden.
webhooks-error-new-secret = Die URL zeigt jetzt auf einen anderen Host: Gib auch ein neues Signaturgeheimnis ein.
webhooks-url-masked-hint = (nur die Person, die ihn hinzugefügt hat, sieht die vollständige URL: gib sie zum Speichern erneut ein)

# Booking questions (#210): booking form, answers, event type editor

book-question-choose = Bitte auswählen
book-question-pick-one = Kreuze mindestens ein Kästchen an.
book-question-read-policy = Richtlinie lesen
book-question-new-tab = (öffnet in einem neuen Tab)
answer-yes = Ja
answer-no = Nein
answer-agreed = Zugestimmt
answer-not-agreed = Nicht zugestimmt
answer-you-agreed = Du hast zugestimmt
answer-you-did-not-agree = Du hast nicht zugestimmt
bookings-answers = Antworten
validate-notes-required = Bitte schreib eine Notiz für die Gastgeber.
validate-question-required = Bitte beantworte „{ $question }“.
validate-question-too-long = Die Antwort auf „{ $question }“ ist zu lang.
validate-question-unknown-option = Die Antwort auf „{ $question }“ gehört nicht zu den angebotenen Optionen.
validate-questions-invalid = Die Buchungsfragen konnten nicht gelesen werden. Lade die Seite neu und versuche es noch einmal.
validate-questions-too-many = Ein Termintyp kann höchstens { $max } Buchungsfragen stellen.
validate-question-kind = Bei Frage { $position }: Diese Fragenart ist nicht verfügbar.
validate-question-kind-change = Bei Frage { $position }: Eine gespeicherte Frage behält ihre Art. Entferne sie und füge eine neue hinzu.
validate-question-label-empty = Bei Frage { $position }: Die Frage ist leer.
validate-question-label-too-long = Bei Frage { $position }: Die Frage ist zu lang (200 Zeichen, 1000 für ein Kontrollkästchen).
validate-question-help-too-long = Bei Frage { $position }: Der Hilfetext ist zu lang (höchstens 500 Zeichen).
validate-question-options-few = Bei Frage { $position }: Mindestens zwei Optionen sind nötig.
validate-question-options-many = Bei Frage { $position }: Höchstens 30 Optionen.
validate-question-option-empty = Bei Frage { $position }: Eine Option ist leer.
validate-question-option-too-long = Bei Frage { $position }: Eine Option ist zu lang (höchstens 200 Zeichen).
validate-question-option-duplicate = Bei Frage { $position }: Zwei Optionen haben denselben Text.
validate-question-link = Bei Frage { $position }: Der Link zur Richtlinie muss eine vollständige Adresse sein, die mit https:// oder http:// beginnt.
etf-section-booking-form = Buchungsformular
etf-booking-form-help = Gäste geben immer ihren Namen und ihre E-Mail-Adresse an. Füge die Fragen hinzu, die dieser Termintyp braucht: Gäste beantworten sie beim Buchen, und die Antworten kommen mit der Buchung bei dir an.
etf-notes-mode = Notizfeld
etf-notes-mode-off = Ausgeblendet
etf-notes-mode-optional = Optional
etf-notes-mode-required = Pflicht
etf-q-empty = Noch keine Fragen.
etf-q-add = Neue Frage
etf-q-kind = Art
etf-q-kind-short-text = Kurzer Text
etf-q-kind-long-text = Langer Text
etf-q-kind-single-choice = Eine Option
etf-q-kind-multiple-choice = Mehrere Optionen
etf-q-kind-checkbox = Kontrollkästchen
etf-q-label = Fragetext
etf-q-help = Hilfetext
etf-q-required = Pflicht
etf-q-options = Optionen
etf-q-option = Option %1
etf-q-add-option = Option hinzufügen
etf-q-remove-option = Diese Option entfernen
etf-q-consent = Dieses Kästchen hält eine Einwilligung fest (Datenschutz, Aufzeichnung)
etf-q-link = Link zur Richtlinie
etf-q-link-help = Optional. Wird unter dem Kästchen angezeigt und öffnet sich in einem neuen Tab.
etf-q-in-calendar = Antwort in den Kalendereintrag schreiben
etf-q-in-calendar-help = Bei sensiblen Antworten ausschalten: Wer deinen Kalender lesen kann, würde sie sehen.
etf-q-not-in-calendar = Nicht im Kalendereintrag
etf-q-consent-word = Einwilligung
etf-q-edit = Bearbeiten
etf-q-done = Fertig
etf-q-move-up = Nach oben
etf-q-move-down = Nach unten
etf-q-remove = Entfernen
etf-q-untitled = Unbenannte Frage
etf-q-kind-locked = Eine gespeicherte Frage behält ihre Art.
etf-q-limit = Höchstens %1 Fragen.
etf-q-preview = Buchungsseite ansehen
etf-q-key = Schlüssel für Webhooks: %1
etf-hint-questions-one = 1 Frage
etf-hint-questions-other = %1 Fragen
etf-hint-no-questions = Keine Fragen
