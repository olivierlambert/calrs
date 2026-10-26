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
overview-welcome = Willkommen, { $name }
overview-public-page = Öffentliche Seite:
overview-avail-banner-title = Standardverfügbarkeit
overview-avail-banner-body = Deine Standardarbeitszeiten wurden auf Mo–Fr, 9:00–17:00 Uhr gesetzt. Sie gelten, wenn andere dich in dynamische Gruppentermine einbeziehen.
overview-avail-banner-cta = Verfügbarkeit prüfen
overview-dismiss = Ausblenden
overview-getting-started = Erste Schritte
overview-getting-started-help = Folge diesen Schritten, um Buchungen anzunehmen.
overview-step-connect-calendar = Kalender verbinden
overview-step-first-event-type = Erste Terminart anlegen
overview-step-share-link = Buchungslink teilen
overview-pending-approval = Warten auf Bestätigung
overview-booking-with = { $title } mit { $guest }
overview-badge-pending = ausstehend
overview-guest-booked = Vom Gast gebucht:
overview-confirm = Bestätigen
overview-decline = Ablehnen
overview-stat-event-types = Terminarten
overview-stat-upcoming = Anstehende Buchungen
overview-stat-pending = Warten auf Bestätigung
overview-stat-sources = Kalenderquellen
overview-quick-actions = Neue Terminart anlegen
overview-action-public-title = Öffentliche Buchungsseite
overview-action-public-desc = Teile einen Link — jeder kann ein Zeitfenster wählen und Zeit mit dir buchen.
overview-action-team-title = Teamplanung
overview-action-team-desc = Verteile Buchungen auf Teammitglieder oder finde einen Termin, an dem alle frei sind.
overview-action-team-desc-empty = Lege zuerst ein Team an und richte dann gemeinsame Terminarten ein.
overview-action-private-title = Privat, nur auf Einladung
overview-action-private-desc = Erzeuge Einmal-Links für bestimmte Kontakte. Niemand sonst kann buchen.
overview-action-shared-title = Geteilte Einladungslinks
overview-action-shared-desc = Alle Kolleginnen und Kollegen im Team können Buchungslinks erzeugen und extern teilen.
overview-action-reason-calendar = Verbinde zuerst einen Kalender
overview-action-reason-ask-admin = Bitte die Administration, ein Team anzulegen
overview-action-reason-team-admin = Erfordert ein Team — lege zuerst eines an
overview-action-reason-team-member = Erfordert ein Team — frage die Administration

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Buchungen
bookings-pending-approval = Warten auf Bestätigung
bookings-available-to-claim = Zur Übernahme verfügbar
bookings-upcoming = Anstehende Buchungen
bookings-with = { $title } mit { $guest }
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
sources-write-bookings-to = Buchungen schreiben nach:
sources-write-none = Keiner (nicht schreiben)
sources-empty = Keine Kalenderquellen verbunden. { $link }, um die Verfügbarkeit zu prüfen.
sources-empty-link-label = Füge eine hinzu

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Terminarten
event-types-heading = Terminarten
event-types-new = Neu
event-types-badge-disabled = deaktiviert
event-types-badge-internal = intern
event-types-badge-private = privat
event-types-badge-resources = Ressourcen
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
event-types-empty = Noch keine Terminarten. { $link }, um Buchungen anzunehmen.
event-types-empty-link-label = Lege eine an

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
etf-conflict-calendars-help = Wähle, welche Kalender auf Konflikte geprüft werden. Ohne Auswahl werden alle Kalender verwendet.
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
admin-heading = Administrations-Dashboard
admin-action-refused = Aktion abgelehnt:
admin-error-last-admin = Das ist der letzte aktive Admin. Stufe zuerst einen anderen Benutzer hoch oder aktiviere einen anderen Admin.
admin-logo = Firmenlogo
admin-logo-help = Wird auf öffentlichen Buchungsseiten angezeigt. PNG, JPEG, GIF oder WebP, max. 2 MB.
admin-logo-refused = Das Logo wurde nicht gespeichert. Lade ein PNG-, JPEG-, GIF- oder WebP-Bild mit höchstens 2 MB hoch.
admin-favicon = Favicon
admin-favicon-help = Erscheint im Browser-Tab und in Lesezeichen. Empfohlen: ein quadratisches PNG ab 32×32 Pixel, max. 512 KB. Ohne eigenes Favicon erscheint die calrs-Krabbe.
admin-favicon-refused = Das Favicon wurde nicht gespeichert. Lade ein PNG-, JPEG-, GIF- oder WebP-Bild mit höchstens 512 KB hoch.
admin-upload-save-failed = Die Datei konnte nicht im Datenverzeichnis des Servers gespeichert werden. Details stehen im Server-Log.
admin-company-link = Firmenlink
admin-company-link-help = Das Logo verlinkt auf öffentlichen Buchungsseiten auf diese URL. Leer lassen für keinen Link.
admin-theme = Design
admin-theme-help = Wähle ein Farbdesign für alle Seiten. Die Umschaltung zwischen Hell und Dunkel ist davon unabhängig — die Designs passen sich beiden Modi an.
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
admin-users = Benutzer ({ $count })
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
admin-groups = Gruppen ({ $count })
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
admin-keep-current-set-hint = (leer lassen, um den aktuellen Wert zu behalten — derzeit gesetzt)
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
admin-page-of = Seite %1 von %2
admin-show-more-js = %1 weitere anzeigen
admin-show-fewer = Weniger anzeigen

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Benutzer { $email } endgültig löschen?{"\u000A"}{"\u000A"}Damit werden sein Benutzerkonto, sein Planungsprofil, seine Kalenderquellen, seine Terminarten und alle Daten gelöscht, die ausschließlich ihm gehören. Vergangene Buchungen werden zusammen mit seinen Terminarten gelöscht.{"\u000A"}{"\u000A"}Bei OIDC-/SSO-Benutzern gilt: Ist die automatische Registrierung aktiv, wird diese Person bei der nächsten Anmeldung neu angelegt.{"\u000A"}{"\u000A"}Dies kann nicht rückgängig gemacht werden.
admin-system-settings-help = Öffentliche URL und Netzwerksicherheitseinstellungen. Sie lassen sich auch über die Umgebungsvariablen <code>CALRS_BASE_URL</code> und <code>CALRS_ALLOW_PRIVATE_HOSTS</code> setzen. Ist eine Umgebungsvariable gesetzt, hat sie <strong>Vorrang</strong> vor dem Wert unten.
admin-set-by-env = — durch die Umgebung gesetzt ({ $var }), überschreibt den gespeicherten Wert
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
bookings-error-generic = Etwas ist schiefgelaufen.
troubleshoot-detail-pending-request = { $title } (offene Anfrage)
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
source-test-microsoft-ok = Verbindung zum Microsoft-Kalender OK.
source-test-microsoft-failed = Verbindung zum Microsoft-Kalender fehlgeschlagen. Prüfe den Kalenderzugriff oder verbinde neu.
source-test-microsoft-credentials = Die Zugangsdaten für den Microsoft-Kalender konnten nicht geladen werden. Verbinde dein Konto neu.
microsoft-callback-needs-https = Setze die öffentliche Basis-URL auf einen HTTPS-Ursprung ohne Pfad, um die zu registrierende Callback-URL zu sehen.
source-form-backend-ews = Microsoft Exchange lokal (EWS)
