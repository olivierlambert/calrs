# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Boeking in afwachting
confirmed-page-title-booked = Boeking bevestigd

confirmed-heading-reschedule-requested = Verzetten aangevraagd
confirmed-heading-rescheduled = Verzet!
confirmed-heading-pending = In afwachting van bevestiging
confirmed-heading-booked = Je afspraak staat!

confirmed-subtitle-reschedule-requested = Je verzoek om te verzetten is naar { $host } gestuurd. Je krijgt een e-mail op { $email } zodra het is goedgekeurd.
confirmed-subtitle-rescheduled = Je boeking is verzet. Er is een bevestiging gestuurd naar { $email }.
confirmed-subtitle-pending = Je boekingsaanvraag is naar { $host } gestuurd. Je krijgt een e-mail op { $email } zodra die is bevestigd.
confirmed-subtitle-booked = Er is een bevestiging gestuurd naar { $email }.

confirmed-detail-event = Afspraak:
confirmed-detail-date = Datum:
confirmed-detail-time = Tijd:
confirmed-detail-with = Met:
confirmed-detail-location = Locatie:
confirmed-detail-notes = Opmerkingen:
confirmed-detail-additional-guests = Extra gasten:

confirmed-book-another = Nog een tijd boeken

confirmed-add-to-calendar = Toevoegen aan agenda

# Slot picker (templates/slots.html)

slots-location-video = Videogesprek
slots-location-phone = Telefoongesprek
slots-location-google-meet = Google Meet

slots-tz-label = Jouw tijdzone
slots-tz-fixed = Tijden worden getoond in { $tz }
slots-time-format-label = Tijdnotatie
tz-group-common = Veelgebruikte tijdzones
tz-region-africa = Afrika
tz-region-america = Amerika
tz-region-antarctica = Antarctica
tz-region-arctic = Arctis
tz-region-asia = Azië
tz-region-atlantic = Atlantische Oceaan
tz-region-australia = Australië
tz-region-europe = Europa
tz-region-indian = Indische Oceaan
tz-region-pacific = Stille Oceaan
slots-offset-note-day = Tijden op %1 zijn in %2 (de klok wordt verzet op %3).
slots-offset-note-range = De klok wordt verzet op %1: vanaf dan zijn de tijden in %2.

slots-view-month = Maandweergave
slots-view-week = Weekweergave
slots-view-column = Kolomweergave

slots-weekday-mon = ma
slots-weekday-tue = di
slots-weekday-wed = wo
slots-weekday-thu = do
slots-weekday-fri = vr
slots-weekday-sat = za
slots-weekday-sun = zo

slots-weekday-mon-short = M
slots-weekday-tue-short = D
slots-weekday-wed-short = W
slots-weekday-thu-short = D
slots-weekday-fri-short = V
slots-weekday-sat-short = Z
slots-weekday-sun-short = Z

slots-select-date = Kies een datum
slots-loading-availability = Beschikbaarheid laden...
slots-click-highlighted = Klik op een gemarkeerde datum om de beschikbare tijden te zien
slots-bookable-range-both = Boekbaar van { $from } tot en met { $until }
slots-bookable-range-from = Boekbaar vanaf { $from }
slots-bookable-range-until = Boekbaar tot en met { $until }
slots-no-longer-bookable = Deze afspraak is niet meer boekbaar.
slots-no-times-month = Geen beschikbare tijden deze maand
slots-no-times-day = Geen beschikbare tijden op deze dag
slots-no-availability-participants = Deze maand is er geen tijd waarop alle deelnemers beschikbaar zijn
slots-week-more = meer

# Booking form (templates/book.html)

book-page-title = { $title } boeken
book-back-to-times = Terug naar tijden
book-name-label = Je naam
book-name-placeholder = Sanne de Vries
book-email-label = E-mail
book-email-placeholder = sanne@example.com
book-email-invalid = Vul een volledig e-mailadres in, inclusief het domein (bijv. sanne@example.com).
book-notes-label = Opmerkingen
book-notes-optional = (optioneel)
book-notes-placeholder = Is er iets wat je wilt bespreken?
book-additional-guests-label = Extra gasten
book-additional-guests-hint = (optioneel, maximaal { $max })
book-add-guest-btn = + E-mailadres van gast toevoegen
book-guest-email-placeholder = collega@example.com
book-phone-label = Telefoonnummer
book-phone-placeholder = 06 12345678
book-phone-help = Een lokaal nummer is prima; we gaan uit van { $country }, tenzij je begint met +.
book-phone-optional-consequence = Laat dit leeg als je liever geen sms'jes over deze boeking ontvangt.
book-phone-required = Voor deze boeking is een telefoonnummer verplicht.
book-phone-invalid-title = Ongeldig telefoonnummer
book-phone-invalid = Vul een telefoonnummer in waar we een sms naartoe kunnen sturen, of laat het veld leeg.
book-phone-country-search = Zoeken
book-phone-country-label = Kies een land
book-phone-country-none = Geen land gekozen
book-phone-country-no-results = Geen landen gevonden
captcha-label = Beveiligingscontrole
captcha-initial-state = Bevestig dat je een mens bent
captcha-verifying = Controleren...
captcha-solved = Je bent een mens
captcha-error = Fout
captcha-troubleshooting = Probleemoplossing
captcha-wasm-disabled = Schakel WASM in om dit veel sneller op te lossen
captcha-verify-aria = Klik om te bevestigen dat je een mens bent
captcha-verifying-aria = Bezig met controleren, even geduld
captcha-verified-aria = Gecontroleerd
captcha-required = Bevestig dat je een mens bent
captcha-error-aria = Er is een fout opgetreden, probeer het opnieuw
book-confirm-button = Boeking bevestigen

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Boeking bevestigd: { $event }, { $date } om { $time } ({ $tz }).
sms-cancelled = Boeking geannuleerd: { $event }, { $date } om { $time } ({ $tz }).
sms-rescheduled = Boeking verzet: { $event } is nu { $date } om { $time } ({ $tz }).
sms-reminder = Herinnering: { $event } begint { $date } om { $time } ({ $tz }).

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Gast:
common-detail-reason = Reden:
common-reason-optional = (optioneel)
common-close-page = Je kunt deze pagina sluiten.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Boeking annuleren
cancel-heading = Boeking annuleren
cancel-subtitle = Je staat op het punt je boeking te annuleren.
cancel-reason-label = Reden
cancel-reason-placeholder-host = Laat de organisator weten waarom...
cancel-button = Boeking annuleren
cancelled-heading = Boeking geannuleerd
cancelled-subtitle = Je boeking is geannuleerd en de organisator is op de hoogte gebracht.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Boeking afwijzen
decline-heading = Boeking afwijzen
decline-subtitle = Je staat op het punt deze boekingsaanvraag af te wijzen.
decline-reason-placeholder-guest = Laat de gast weten waarom...
decline-button = Boeking afwijzen
declined-heading = Boeking afgewezen
declined-subtitle = De boeking is afgewezen en de gast is op de hoogte gebracht.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Boeking goedkeuren
approve-heading = Boeking goedkeuren
approve-subtitle = Je staat op het punt deze boekingsaanvraag goed te keuren.
approve-button = Boeking goedkeuren
approved-heading = Boeking goedgekeurd
approved-subtitle = De boeking is bevestigd en er is een bevestiging gestuurd naar { $email }.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Boeking overnemen
claim-heading = Boeking overnemen
claim-subtitle = Je staat op het punt deze boeking over te nemen. Je wordt toegevoegd als deelnemer.
claim-assigned-to = Toegewezen aan:
claim-button = Deze boeking overnemen
claimed-page-title = Boeking overgenomen
claimed-heading = Boeking overgenomen
claimed-subtitle = Je hebt deze boeking overgenomen. Er is een agenda-uitnodiging naar je e-mailadres gestuurd.
already-claimed-page-title = Al overgenomen
already-claimed-heading = Al overgenomen
already-claimed-subtitle = Deze boeking is al overgenomen door { $name }.

# Generic error page (booking_action_error.html)

action-error-page-title = Fout bij boekingsactie

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Boeking verzetten — calrs
host-resched-heading = Boeking verzetten
host-resched-subtitle = { $guest } krijgt dan een e-mail met het verzoek een nieuw tijdstip te kiezen.
host-resched-currently = Huidig tijdstip:
host-resched-button = Verzoek om te verzetten sturen
host-resched-cancel-link = Annuleren

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Verzetten bevestigen
resched-confirm-heading = Verzetten bevestigen
resched-confirm-subtitle = Je staat op het punt je boeking naar een nieuw tijdstip te verzetten.
resched-was = Was:
resched-new = Nieuw:
resched-button = Verzetten bevestigen
resched-back-to-picker = Terug naar tijdkeuze

# Base layout chrome (templates/base.html)

base-loader-checking = Beschikbaarheid controleren
base-loader-please-wait = Even geduld, de nieuwste agendagegevens worden geladen...
base-stop-impersonating = Stoppen met meekijken als gebruiker
base-theme-toggle = Thema wisselen
base-powered-by = Mogelijk gemaakt door

# Profile (templates/profile.html)

profile-pick-event-type-invite = Kies een afspraaktype om een tijd te boeken.
profile-no-event-type = Nog geen afspraaktypes beschikbaar.

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

common-month-1 = januari
common-month-2 = februari
common-month-3 = maart
common-month-4 = april
common-month-5 = mei
common-month-6 = juni
common-month-7 = juli
common-month-8 = augustus
common-month-9 = september
common-month-10 = oktober
common-month-11 = november
common-month-12 = december
common-month-short-1 = jan
common-month-short-2 = feb
common-month-short-3 = mrt
common-month-short-4 = apr
common-month-short-5 = mei
common-month-short-6 = jun
common-month-short-7 = jul
common-month-short-8 = aug
common-month-short-9 = sep
common-month-short-10 = okt
common-month-short-11 = nov
common-month-short-12 = dec

common-weekday-long-mon = maandag
common-weekday-long-tue = dinsdag
common-weekday-long-wed = woensdag
common-weekday-long-thu = donderdag
common-weekday-long-fri = vrijdag
common-weekday-long-sat = zaterdag
common-weekday-long-sun = zondag

# Format patterns are parametric per locale to handle word order. Translators
# pick where each placeholder lands. Example outputs:
#   EN: April 2026  /  Tuesday, March 12, 2026
#   FR: avril 2026  /  mardi 12 mars 2026
#   ES: abril 2026  /  martes, 12 de marzo de 2026
common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday } { $day } { $month } { $year }
common-format-date = { $day } { $month } { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Verzetten
email-action-cancel-booking = Boeking annuleren

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
email-confirm-greeting = Hoi { $name },
email-confirm-headline = Je boeking is bevestigd!
email-confirm-ics-attached-plain = Er is een agenda-uitnodiging bijgevoegd.
email-confirm-ics-attached-html = Er is een agenda-uitnodiging bij deze e-mail gevoegd.
email-confirm-need-to-cancel = Wil je annuleren? { $url }

# Email: guest reminder

email-reminder-subject = Herinnering: { $event } om { $time }
email-reminder-headline = Je afspraak komt eraan.

# Email: guest cancellation

email-cancel-subject = Geannuleerd: { $event } — { $date }
email-cancel-headline-by-host = Je boeking is geannuleerd door { $host }.
email-cancel-headline-by-guest = Je boeking is geannuleerd.
email-cancel-ics-attached-plain = Er is een agenda-annulering bijgevoegd.
email-cancel-ics-attached-html = Er is een agenda-annulering bij deze e-mail gevoegd.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Let op: annuleren kan tot uiterlijk { $minutes } minuten van tevoren.
email-confirm-reschedule-notice = Let op: verzetten kan tot uiterlijk { $minutes } minuten van tevoren.

# Event type form: Google Meet location + cancel/reschedule minimum notice
# (templates/event_type_form.html, src/google_meet.rs)

event-type-form-location-google-meet = Google Meet (automatisch gegenereerde link)
event-type-form-location-google-meet-hint = Bij bevestiging wordt een unieke Google Meet-link aangemaakt, op naam van de toegewezen organisator. Elke organisator (jij, of elk geschikt teamlid) moet Google Calendar gekoppeld hebben met een agenda geselecteerd om naar terug te schrijven.

google-meet-prereq-no-host = Voor Google Meet is een organisator met een gekoppelde Google Calendar nodig.
google-meet-prereq-no-eligible = Voor Google Meet is minstens één geschikt teamlid met een gekoppelde Google Calendar nodig.
google-meet-prereq-missing = Voor Google Meet moet elke organisator Google Calendar gekoppeld hebben met een agenda geselecteerd om naar terug te schrijven. Ontbreekt nog bij: { $names }. Koppel ze via Dashboard → Agendabronnen.
google-meet-unavailable-title = Google Meet is niet beschikbaar
google-meet-dynamic-group-unavailable = De organisator moet Google Calendar gekoppeld hebben met een agenda geselecteerd om naar terug te schrijven.

event-type-form-cancel-notice-label = Minimale aankondigingstijd voor annuleren
event-type-form-reschedule-notice-label = Minimale aankondigingstijd voor verzetten
event-type-form-notice-help = Laat leeg voor geen beperking.
event-type-form-resources-label = Benodigde faciliteiten
event-type-form-resources-hint = Tijdsloten worden alleen aangeboden als de geselecteerde faciliteiten beschikbaar zijn, volgens de modus hieronder.
event-type-form-resources-mode-all = Alle geselecteerde faciliteiten moeten vrij zijn
event-type-form-resources-mode-round-robin = Eén vrije faciliteit is genoeg (die wordt aan de boeking toegewezen)
event-type-form-notice-unit-minutes = minuten
event-type-form-notice-unit-hours = uur
event-type-form-notice-unit-days = dagen
event-type-form-booking-horizon-label = Boekingshorizon
event-type-form-booking-horizon-help = Hoeveel dagen vooruit gasten kunnen boeken. Laat leeg voor geen limiet, 0 voor alleen vandaag.
event-type-form-bookable-range-label = Boekbare periode
event-type-form-bookable-from = van
event-type-form-bookable-until = tot en met
event-type-form-bookable-range-help = Alleen deze dagen zijn boekbaar. Laat leeg voor geen limiet. Gebruik uitzonderingen voor andere tijden op losse dagen.
event-type-form-bookable-range-past-warning = Deze periode is al voorbij. Het afspraaktype wordt niet meer openbaar getoond.
event-type-form-bookable-range-too-far-warning = Deze periode begint over meer dan een jaar. Gasten kunnen maximaal een jaar vooruit boeken, dus boeken kan vanaf { $date }.
event-type-form-bookable-range-horizon-warning = De boekingshorizon eindigt voordat deze periode begint. Gasten kunnen boeken vanaf { $date }.
event-type-form-bookable-range-bookings-outside =
    { $count ->
        [one] 1 bestaande boeking valt buiten de boekbare periode.
       *[other] { $count } bestaande boekingen vallen buiten de boekbare periode.
    }
event-type-form-bookable-range-bookings-link = Boekingen bekijken
event-type-form-saved = Opgeslagen.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = Annuleren kan tot uiterlijk { $minutes } minuten voor de afspraak.
confirmed-reschedule-notice-info = Verzetten kan tot uiterlijk { $minutes } minuten voor de afspraak.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Deze boeking kan niet meer online worden geannuleerd
booking-blocked-title-reschedule = Deze boeking kan niet meer online worden verzet
booking-blocked-body = De organisator vraagt minstens { $minutes } minuten van tevoren bericht. Kun je niet aanwezig zijn? Mail dan rechtstreeks naar <a href="mailto:{ $host_email }">{ $host_email }</a>.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Kopiëren
dashboard-event-types-copied = Gekopieerd!
dashboard-event-types-copy-title = Boekingslink kopiëren
dashboard-event-types-copy-failed = Kopiëren mislukt

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Planning
nav-overview = Overzicht
nav-event-types = Afspraaktypes
nav-bookings = Boekingen
nav-teams = Teams
nav-section-shared-links = Gedeelde links
nav-invite-links = Uitnodigingslinks
nav-section-calendars = Agenda's
nav-sources = Bronnen
nav-section-personal = Persoonlijk
nav-settings = Profiel en instellingen
nav-troubleshoot = Probleemoplossing
nav-section-admin = Beheer
nav-admin-panel = Beheerpaneel
nav-sign-out = Uitloggen
nav-release-notes = Release notes bekijken
dash-skip-to-content = Naar de inhoud
dash-open-menu = Menu openen
dash-shortcuts-title = Sneltoetsen
dash-shortcut-search = Zoeken op deze pagina
dash-shortcut-new = Nieuw maken
dash-shortcut-save = Formulier opslaan
dash-shortcut-close = Menu's en vensters sluiten
dash-shortcut-help = Deze hulp tonen
dash-shortcut-go = Naar { $page }
dash-shortcut-then = dan
dash-close = Sluiten
dash-copied = Link gekopieerd naar het klembord
dash-copy-manual = Kopieer deze link:
dash-confirm-again = Klik nogmaals om te bevestigen
dash-unsaved = Niet-opgeslagen wijzigingen
dash-no-matches = Niets komt overeen met je zoekopdracht.
dash-more-actions = Meer acties

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = De tijdzone van je browser is { $detected }, maar je boekingstijdzone staat ingesteld op { $current }.
tz-banner-update = Bijwerken
tz-banner-dismiss = Sluiten

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = Voer een URL in:
editor-link-default-label = linktekst
editor-placeholder-text = tekst
editor-nothing-to-preview = Niets om te bekijken

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Dashboard
overview-avail-banner-body = Je standaardwerktijden zijn ingesteld op ma–vr, 9:00–17:00. Deze worden gebruikt wanneer anderen je toevoegen aan dynamische groepsafspraken.
overview-avail-banner-cta = Controleer je beschikbaarheid
overview-dismiss = Sluiten
overview-getting-started = Aan de slag
overview-step-connect-calendar = Koppel een agenda
overview-step-first-event-type = Maak je eerste afspraaktype aan
overview-step-share-link = Deel je boekingslink
overview-pending-approval = In afwachting van goedkeuring
overview-confirm = Bevestigen
overview-decline = Afwijzen
overview-copy-link = Boekingslink kopiëren
overview-duration-h = { $hours } u
overview-duration-m = { $minutes } min
overview-duration-hm = { $hours } u { $minutes } min
overview-percent = { $value }%
overview-chart-title = Afspraken per week
overview-chart-this-week = Deze week
overview-chart-week-of = Week van { $date }
overview-series-confirmed = Bevestigd
overview-series-pending = In afwachting
overview-series-cancelled = Geannuleerd of afgewezen
overview-view-data = Als tabel bekijken
overview-table-week = Week
overview-types-title = Per type afspraak
overview-types-other = Overige types
overview-agenda-title = Hierna
overview-agenda-today = Vandaag
overview-agenda-tomorrow = Morgen
overview-agenda-now = Bezig
overview-agenda-join = Deelnemen
overview-agenda-empty = Geen afspraken in de komende 14 dagen. Deel je boekingslink om er te krijgen.
overview-agenda-all = Alle boekingen
nav-main-label = Hoofdnavigatie
dash-shortcuts-short = Sneltoetsen
dash-hint-to-decide = { $count } te beslissen
dash-hint-failing = { $count } mislukt
common-format-day-date = { $weekday } { $day } { $month }
common-format-short-date = { $day } { $month }
bookings-summary = Komend: { $upcoming }. Wacht op je beslissing: { $pending }. Tijden in { $tz }.
sources-state-failing = Mislukt
troubleshoot-previous-day = Vorige dag
troubleshoot-next-day = Volgende dag
overview-new-meeting-type = Nieuw afspraaktype
overview-step-connect = Koppelen
overview-step-create = Maken
overview-links-title = Je boekingslinks
overview-open-calendar = Je agenda openen
overview-week-title = Deze week
overview-week-closed = gesloten
overview-week-total = { $booked } geboekt
overview-week-number = Week { $week }
overview-week-short = W{ $week }
overview-time-range = { $from } tot { $to }
overview-until = tot { $time }
overview-expires = Verloopt { $when }
overview-guest-and-more = { $name } en { $count } anderen
overview-mast-now = Nu: { $title } met { $guest }, tot { $time }
overview-mast-next = Hierna: { $title } met { $guest } om { $time }
overview-mast-next-day = Hierna: { $title } met { $guest }, { $day } om { $time }
overview-ruler-booked = Geboekt
overview-ruler-pending = Wacht op een beslissing
overview-ruler-unavailable = Niet beschikbaar (je agenda)
overview-key-open = Open voor boekingen
overview-ruler-aria = Vandaag, { $from } tot { $to }
overview-chart-now = nu
overview-figures-title = Laatste 30 dagen
overview-fig-booked = Geboekte afspraken
overview-fig-time = Tijd in afspraken
overview-fig-cancelled = Geannuleerd
overview-fig-notice = Gemiddelde aanlooptijd
overview-fig-before = { $value } in de 30 dagen ervoor
overview-fig-prev-key = De 30 dagen ervoor
overview-fig-days =
    { $count ->
        [one] 1 dag
       *[other] { $count } dagen
    }

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Boekingen
bookings-pending-approval = In afwachting van goedkeuring
bookings-available-to-claim = Beschikbaar om over te nemen
bookings-upcoming = Komende boekingen
bookings-guest-booked = Geboekt door gast:
bookings-resource = Faciliteit:
bookings-confirm = Bevestigen
bookings-reschedule = Verzetten
bookings-decline = Afwijzen
bookings-claim = Overnemen
bookings-badge-awaiting-reschedule = wacht op nieuw tijdstip
bookings-badge-writeback-failed = niet in je agenda
bookings-writeback-failed-detail = Schrijven naar de agenda mislukt: { $error }
bookings-cancel = Annuleren
bookings-reason-placeholder = Reden (optioneel)
bookings-confirm-cancel = Annulering bevestigen
bookings-back = Terug
bookings-empty = Nog geen komende boekingen.<br>Deel je { $link } zodat mensen een afspraak met je kunnen boeken.
bookings-empty-link-label = links naar afspraaktypes
bookings-search-placeholder = Zoek gasten, e-mails of afspraken
bookings-recent = Afgelopen 30 dagen
bookings-cancelled = Geannuleerd en afgewezen

# Host notes on a booking (templates/booking_notes.html, bookings page, overview, host emails)

host-notes-title = Notities van de organisatoren
host-notes-hint = Zichtbaar voor de organisatoren van deze boeking, een teamlid dat de boeking heeft overgenomen en een beheerder die optreedt als een van de organisatoren. Ze worden nooit aan de gast getoond en nooit aan de uitnodiging of het agenda-item toegevoegd.
host-notes-placeholder = Bijvoorbeeld: wat er in een gesprek is gezegd, wat er voorbereid moet worden.
host-notes-save = Notities opslaan
host-notes-saved = Notities opgeslagen.
host-notes-too-long = Notities van de organisatoren mogen maximaal { $max } tekens lang zijn.
host-notes-conflict = Deze notities zijn gewijzigd nadat deze pagina werd geopend. De opgeslagen versie staat hieronder; opnieuw opslaan vervangt die door de tekst in het veld.
host-notes-current = Opgeslagen versie:
host-notes-last-edit = Laatst bewerkt door { $name }, { $when }
host-notes-last-edit-anon = Laatst bewerkt op { $when }
host-notes-edit = Notitie bewerken
host-notes-marker = Notitie van de organisatoren
host-notes-cleared = (gewist)
host-notes-last-edit-as = Laatst bewerkt door { $name } (beheerder, als { $host }), { $when }
dash-not-found-title = Niet gevonden
dash-not-found-body = Deze pagina bestaat niet, of dit account heeft er geen toegang toe.
host-notes-from-guest = Van de gast:
bookings-host-notes = Notities van de organisatoren:
email-detail-host-notes = Notities van de organisatoren:

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Teams
teams-heading = Teams
teams-new = Nieuw
teams-badge-public = openbaar
teams-badge-private = privé
teams-settings = Instellingen
teams-view = Bekijken
teams-empty = Nog geen teams.
teams-empty-admin = { $link } om samen te werken met je team.
teams-empty-admin-link-label = Maak er een aan
teams-empty-member = Teams worden aangemaakt door beheerders. Vraag je beheerder om er een aan te maken en jou als lid toe te voegen.
teams-subtitle = Mensen die afspraken delen, om de beurt of allemaal samen geboekt.

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Uitnodigingslinks
invite-links-heading = Uitnodigingslinks
invite-links-new = Nieuwe interne afspraak
invite-links-help = Genereer eenmalige boekingslinks voor interne afspraaktypes. Elke ingelogde collega kan hier links aanmaken en delen.
invite-links-duration = { $minutes } min
invite-links-hosted-by = Georganiseerd door { $host }
invite-links-get-link = Link ophalen
invite-links-invites = Uitnodigingen
invite-links-empty = Nog geen interne afspraaktypes.<br>{ $link } met zichtbaarheid "Intern" zodat elke collega boekingslinks kan genereren.
invite-links-empty-link-label = Maak een afspraaktype aan
invite-links-js-generating = Bezig met genereren...
invite-links-js-copied = Gekopieerd!
invite-links-js-error = Fout

teams-member-count =
    { $count ->
        [one] { $count } lid
       *[other] { $count } leden
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Agendabronnen
sources-heading = Agendabronnen
sources-add = Toevoegen
sources-last-sync = Laatst gesynchroniseerd:
sources-sync-failing = Synchronisatie mislukt sinds { $since } UTC: { $error }
sources-sync = Synchroniseren
sources-full-resync = Volledig opnieuw synchroniseren
sources-full-resync-title = Cache wissen en alle afspraken opnieuw ophalen van de server
sources-test = Testen
sources-reconnect = Opnieuw koppelen
sources-reconnect-title = De Google-toestemmingsprocedure opnieuw doorlopen
sources-edit = Bewerken
sources-remove = Verwijderen
sources-remove-confirm = Bron '{ $name }' verwijderen? Hiermee worden alle gesynchroniseerde afspraken van deze bron verwijderd.
sources-no-write-calendar = Geen agenda gekozen om naar te schrijven. Bevestigde boekingen blijven in calrs en worden niet naar deze agenda gezet. Kies hieronder een agenda om terugschrijven in te schakelen.
sources-write-calendar-read-only = De agenda waarin boekingen worden geschreven is nu alleen-lezen, dus ze komen er niet meer in. Kies hieronder een agenda waarin je kunt schrijven.
sources-write-calendar-refused = Die agenda kan geen boekingen ontvangen: het is geen agenda van deze bron, of het account kan er niet in schrijven. Kies een andere.
sources-microsoft-not-ready = calrs is nog niet klaar met het lezen van deze agenda. Tot dan tonen je boekingspagina's geen vrije tijd. Dat duurt na het koppelen tot ongeveer een minuut; duurt het langer, dan zegt de synchronisatiefout hierboven waarom.
sources-write-bookings-to = Boekingen schrijven naar:
sources-write-none = Geen (niet schrijven)
sources-empty = Geen agendabronnen gekoppeld. { $link } om beschikbaarheid te controleren.
sources-empty-link-label = Voeg er een toe
sources-subtitle = Agenda's die calrs leest om te weten wanneer je bezet bent, en waarin het nieuwe boekingen schrijft.
sources-never-synced = Nog nooit gesynchroniseerd
sources-calendars = Agenda's
sources-calendars-synced = Gesynchroniseerde agenda's: { $synced } van { $total }
source-calendars-page-title = Te synchroniseren agenda's
source-calendars-heading = Te synchroniseren agenda's
source-calendars-back = Terug naar agendabronnen
source-calendars-help = Kies welke agenda's van { $name } calrs synchroniseert. Niet-aangevinkte agenda's worden nooit opgehaald en blokkeren je beschikbaarheid niet.
source-calendars-select-all = Alles selecteren
source-calendars-select-none = Niets selecteren
source-calendars-write-badge = Boekingen worden hier geschreven
source-calendars-write-hint = Bevestigde boekingen worden in deze agenda geschreven, of die nu gesynchroniseerd wordt of niet.
source-calendars-save = Opslaan
source-calendars-saved = Opgeslagen. Nieuw aangevinkte agenda's worden bij de volgende synchronisatie opgehaald; gebruik Synchroniseren op de bronnenpagina om ze nu op te halen.
source-calendars-empty = Nog geen agenda's gevonden voor deze bron. Start een synchronisatie om ze te vinden.

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Afspraaktypes
event-types-heading = Afspraaktypes
event-types-new = Nieuw
event-types-badge-disabled = uitgeschakeld
event-types-badge-internal = intern
event-types-badge-private = privé
event-types-badge-resources = faciliteiten
event-types-badge-active = actief
event-types-badge-expired = verlopen
event-types-badge-range-from = vanaf { $date }
event-types-badge-range-until = t/m { $date }
event-types-send-invites = Uitnodigingen versturen
event-types-duration = { $minutes } min
event-types-mode-collective = collectief
event-types-mode-round-robin = round-robin
event-types-edit = Bewerken
event-types-disable = Uitschakelen
event-types-enable = Inschakelen
event-types-embed = Embed
event-types-overrides = Uitzonderingen
event-types-team-settings = Teaminstellingen
event-types-invites = Uitnodigingen
event-types-view-public = Openbare pagina bekijken
event-types-view-page = Pagina bekijken
event-types-delete = Verwijderen
event-types-delete-confirm = Afspraaktype '{ $title }' verwijderen? Dit kan niet ongedaan worden gemaakt.
event-types-duplicate = Dupliceren
event-types-duplicate-title = { $title } (kopie)
event-types-empty = Nog geen afspraaktypes. { $link } om boekingen te gaan ontvangen.
event-types-empty-link-label = Maak er een aan
event-types-subtitle = De afspraken die mensen met je kunnen boeken. Kopieer een link of sluit hem in op je site.
event-types-search-placeholder = Filteren op titel of link
event-types-switch-label = Neemt boekingen aan

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Vet (Ctrl+B)
editor-italic = Cursief (Ctrl+I)
editor-strikethrough = Doorhalen
editor-code = Inline code
editor-link = Link invoegen (Ctrl+K)
editor-toggle-preview = Voorbeeld aan/uit
editor-preview = Voorbeeld

# Profile and settings (templates/settings.html)

settings-page-title = Instellingen
settings-heading = Profiel en instellingen
settings-subtitle = Je openbare profiel, taal, tijdzone en standaard werktijden.
settings-public-page-label = Je openbare boekingspagina
settings-copy = Kopiëren
settings-copied = Gekopieerd!
settings-open = Openen
settings-avatar = Avatar
settings-upload = Uploaden
settings-remove = Verwijderen
settings-display-name = Weergavenaam
settings-display-name-placeholder = Je naam
settings-username = Gebruikersnaam
settings-username-hint = (gebruikt in je boekings-URL)
settings-username-pattern-title = Alleen kleine letters, cijfers en streepjes
settings-username-help = Je openbare boekingspagina:
settings-title = Functie
settings-title-placeholder = bijv. Software Engineer, Product Manager
settings-title-help = Getoond op je openbare profiel en in de zijbalk.
settings-bio = Bio
settings-bio-placeholder = Vertel iets over jezelf...
settings-bio-help = Getoond op je openbare boekingspagina. Ondersteunt **vet**, *cursief*, ~~doorhalen~~, `code` en [links](url).
settings-booking-email = E-mailadres voor boekingen
settings-booking-email-help = Dit e-mailadres verschijnt op je openbare boekingspagina's en in e-mailmeldingen. Laat leeg om je inlog-e-mailadres te gebruiken.
settings-booking-email-warning = Zorg dat dit e-mailadres bestaat bij je e-mailprovider. Anders worden meldingen niet afgeleverd.
settings-timezone = Tijdzone
settings-timezone-help = Je beschikbaarheidsregels en boekingstijden worden in deze tijdzone berekend.
settings-language = Taal
settings-language-auto = Automatisch (browserstandaard)
settings-language-help = Kies een taal voor de interface, of laat op Automatisch staan om de instelling van je browser te volgen.
settings-public-page-language = Taal van openbare boekingspagina's
settings-public-page-language-visitor = De taal van elke bezoeker
settings-public-page-language-host = Altijd mijn taal
settings-public-page-language-help = Taal van je openbare profiel, je boekingspagina's en de e-mails die je gasten ontvangen. "Altijd mijn taal" gebruikt de taal die je hierboven kiest.
settings-error-public-page-language = Kies een geldige taal voor de openbare boekingspagina's.
settings-dynamic-group = Anderen mogen mij toevoegen aan dynamische groepslinks
settings-dynamic-group-help = Als dit is ingeschakeld, kunnen andere gebruikers ad-hoc collectieve afspraak-URL's maken waar jij in zit (bijv. { $example }).
settings-lend-resource = Mijn agendatoegang uitlenen voor het reserveren van faciliteiten
settings-lend-resource-help = Als een boeking een gedeelde faciliteit moet reserveren (demolab, vergaderruimte) waar jouw agenda-account naar kan schrijven, mag calrs je opgeslagen agendagegevens daarvoor gebruiken.
settings-default-availability = Standaardbeschikbaarheid
settings-default-availability-help = Je standaardwerktijden. Worden gebruikt voor dynamische groepslinks wanneer anderen je aan een afspraak toevoegen.
settings-copy-to-all = Naar alle dagen kopiëren
settings-copy-to-all-title = Kopieer de tijdvakken van de eerste ingeschakelde dag naar alle andere ingeschakelde dagen
settings-add-window = Tijdvak toevoegen
settings-remove-window = Tijdvak verwijderen
settings-save = Instellingen opslaan
settings-appearance = Weergave
settings-theme-system = Systeem
settings-theme-light = Licht
settings-theme-dark = Donker

# Sign in (templates/auth/login.html)

login-page-title = Inloggen
login-heading = Inloggen
login-subtitle = Log in op je calrs-account
login-sso = Inloggen met SSO
login-or = of
login-email = E-mail
login-password = Wachtwoord
login-submit = Inloggen met e-mail
login-no-account = Nog geen account? { $link }
login-register-link = Registreren

# Registration (templates/auth/register.html)

register-page-title = Registreren
register-heading = Account aanmaken
register-subtitle = Registreer een nieuw calrs-account
register-domains-limited = Registratie is beperkt tot: { $domains }
register-name = Naam
register-name-placeholder = Je naam
register-email = E-mail
register-password = Wachtwoord
register-password-hint = (min. 12 tekens)
register-submit = Account aanmaken
register-have-account = Heb je al een account? { $link }
register-signin-link = Inloggen

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Te veel inlogpogingen. Probeer het later opnieuw.
auth-error-invalid-credentials = Ongeldig e-mailadres of wachtwoord
auth-error-internal = Interne fout
auth-error-registration-disabled = Registratie is uitgeschakeld.
auth-error-name-length = Naam moet tussen 1 en 255 tekens lang zijn
auth-error-email-length = E-mailadres moet tussen 1 en 255 tekens lang zijn
auth-error-email-invalid = Voer een geldig e-mailadres in
auth-error-email-domain = E-maildomein niet toegestaan
auth-error-password-length = Wachtwoord moet minimaal 12 tekens lang zijn
auth-error-email-taken = E-mailadres is al geregistreerd
auth-error-create-failed = Account aanmaken mislukt

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Agendabron
source-test-sync-heading = Synchronisatie: { $name }
source-test-heading = Verbindingstest
source-write-page-title = Terugschrijven naar agenda instellen
source-write-back = Terug naar dashboard
source-write-heading = Waar moeten boekingen naartoe?
source-write-help = Als iemand een afspraak met je boekt, kan calrs die automatisch in je agenda zetten. Kies naar welke agenda boekingen voor { $name } worden geschreven.
source-write-save = Opslaan
source-write-skip = Nu overslaan
source-write-sync-results = Synchronisatieresultaten

source-write-event-count =
    { $count ->
        [one] { $count } afspraak
       *[other] { $count } afspraken
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Datumuitzonderingen
overrides-heading = Datumuitzonderingen
overrides-back-teams = Terug naar teams
overrides-back-event-types = Terug naar afspraaktypes
overrides-intro = Voeg uitzonderingen voor specifieke datums toe voor { $title }
overrides-add-heading = Nieuwe uitzondering toevoegen
overrides-date = Datum
overrides-type = Soort uitzondering
overrides-type-blocked = Hele dag blokkeren
overrides-type-custom = Aangepaste tijden
overrides-start-time = Begintijd
overrides-end-time = Eindtijd
overrides-add-submit = Uitzondering toevoegen
overrides-existing = Bestaande uitzonderingen
overrides-badge-blocked = geblokkeerd
overrides-badge-custom = aangepaste tijden
overrides-delete = Verwijderen
overrides-delete-confirm = Deze uitzondering verwijderen?
overrides-empty = Nog geen datumuitzonderingen.<br>Gebruik het formulier hierboven om specifieke datums te blokkeren (feestdagen, vrije dagen) of aangepaste tijden in te stellen.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Kies een afspraaktype om een tijd te boeken.
team-profile-empty = Nog geen afspraaktypes beschikbaar.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Beschikbaarheid controleren
troubleshoot-empty = Geen afspraaktypes gevonden. { $link } om je beschikbaarheid te controleren.
troubleshoot-empty-link-label = Maak er een aan
troubleshoot-subtitle = Bekijk waarom tijdsloten beschikbaar of geblokkeerd zijn voor { $title }
troubleshoot-duration = { $minutes } min
troubleshoot-buffer-before = { $minutes } min buffertijd vooraf
troubleshoot-buffer-after = { $minutes } min buffertijd achteraf
troubleshoot-min-notice = { $minutes } min aankondigingstijd
troubleshoot-blocked-override = Geblokkeerd door datumuitzondering (vrije dag)
troubleshoot-custom-hours-active = Uitzondering met aangepaste tijden actief (vervangt wekelijkse regels)
troubleshoot-microsoft-not-ready = Je Microsoft-agenda is nog niet voor het eerst gesynchroniseerd: tot dan is elk tijdstip bezet. Zie Agendabronnen.
troubleshoot-legend-available = Beschikbaar
troubleshoot-legend-calendar-event = Agenda-afspraak
troubleshoot-legend-booking = Boeking
troubleshoot-legend-resource = Faciliteit bezet
troubleshoot-legend-outside = Buiten werktijden
troubleshoot-legend-buffer = Buffertijd / min. aankondigingstijd
troubleshoot-blocked-slots = Geblokkeerde tijdsloten
troubleshoot-none-date-blocked = Deze datum is geblokkeerd door een beschikbaarheidsuitzondering (vrije dag). Geen tijdsloten beschikbaar.
troubleshoot-none-custom-hours = Uitzondering met aangepaste tijden actief, maar zonder passende tijdvakken. Controleer je uitzonderingsinstellingen.
troubleshoot-none-no-rules = Geen beschikbaarheidsregels voor deze dag van de week. Dit afspraaktype is niet te boeken op { $date }.
troubleshoot-none-all-bookable = Geen geblokkeerde tijdsloten binnen de beschikbare uren. Alle tijden zijn te boeken.
troubleshoot-label-outside = Buiten beschikbaarheid
troubleshoot-label-available = Beschikbaar
troubleshoot-label-min-notice = Min. aankondigingstijd ({ $minutes } min)
troubleshoot-label-beyond-horizon = Buiten boekingshorizon ({ $days } dagen)
troubleshoot-label-outside-date-range = Buiten de boekbare periode
troubleshoot-label-buffer = Buffertijd ({ $minutes } min)
troubleshoot-label-resource-busy = Faciliteit bezet: { $names }
troubleshoot-detail-around = Rond: { $label }
troubleshoot-detail-around-booking = Rond boeking van { $guest }
troubleshoot-reason-calendar-event = Agenda-afspraak: { $label }
troubleshoot-reason-booking = Boeking: { $label }

# Invite management (templates/invite_form.html)

invites-heading = Uitnodigingen
invites-back-teams = Terug naar teams
invites-back-event-types = Terug naar afspraaktypes
invites-intro = Verstuur uitnodigingslinks voor { $title }
invites-capped = <strong>Invoer is beperkt tot { $max } ontvangers per keer.</strong> Verstuur de rest in een volgende ronde.
invites-failed-hint = — bekijk de serverlogs voor details.
invites-quick-link = Snelle link
invites-quick-link-help = Genereer een eenmalige link en kopieer die naar je klembord.
invites-get-link = Link ophalen
invites-or-email = Of verstuur per e-mail
invites-recipients = Ontvangers
invites-recipients-hint = (één e-mailadres per regel, max. { $max })
invites-message = Persoonlijk bericht
invites-message-hint = (optioneel, wordt naar elke ontvanger gestuurd)
invites-message-placeholder = Ik laat je graag een demo zien...
invites-expires-in = Verloopt over
invites-expires-days = { $days } dagen
invites-expires-never = Nooit
invites-allow-multiple = Meerdere boekingen per ontvanger toestaan
invites-send = Uitnodigingen versturen
invites-sent-heading = Verstuurde uitnodigingen
invites-badge-expired = verlopen
invites-badge-used = gebruikt
invites-badge-active = actief
invites-sent-by = Verstuurd door { $name }
invites-uses = { $used }/{ $max } keer gebruikt
invites-expires-at = Verloopt { $date }
invites-copy-link = Link kopiëren
invites-delete = Verwijderen
invites-delete-confirm = Deze uitnodiging verwijderen?
invites-empty = Nog geen uitnodigingen verstuurd. Gebruik het formulier hierboven om iemand een boekingslink te sturen.
invites-js-generating = Genereren...
invites-js-copied = Gekopieerd!
invites-js-error = Fout

invites-sent-count =
    { $count ->
        [one] { $count } uitnodiging verstuurd.
       *[other] { $count } uitnodigingen verstuurd.
    }

invites-skipped-invalid =
    { $count ->
        [one] { $count } ongeldige regel overgeslagen:
       *[other] { $count } ongeldige regels overgeslagen:
    }

invites-skipped-duplicate =
    { $count ->
        [one] { $count } dubbele regel overgeslagen:
       *[other] { $count } dubbele regels overgeslagen:
    }

invites-failed =
    { $count ->
        [one] { $count } uitnodiging mislukt (DB of SMTP):
       *[other] { $count } uitnodigingen mislukt (DB of SMTP):
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Agendabron bewerken
source-form-title-add = Agenda toevoegen
source-form-heading-edit = Agendabron bewerken
source-form-heading-add = Een agenda koppelen
source-form-subtitle-edit = Werk de verbinding bij. Laat het wachtwoord leeg om het huidige te behouden. Synchroniseer na het wijzigen van de URL of gebruikersnaam om de lijst met gevonden agenda's te vernieuwen.
source-form-subtitle-add = Koppel een CalDAV-server of Microsoft Exchange (EWS) zodat calrs je beschikbaarheid kan controleren wanneer gasten een afspraak boeken.
source-form-backend = Backend
source-form-preset = Voorinstelling
source-form-connect-google = Koppelen met Google
source-form-google-unavailable = Google Agenda is niet beschikbaar. Neem contact op met je beheerder.
source-form-name = Weergavenaam
source-form-name-placeholder = Mijn agenda
source-form-url-caldav = CalDAV-URL
source-form-url-ews = URL van EWS-endpoint
source-form-username = Gebruikersnaam
source-form-password = Wachtwoord
source-form-password-keep = Laat leeg om het huidige te behouden
source-form-password-placeholder = App-wachtwoord of accountwachtwoord
source-form-skip-test = Verbindingstest overslaan
source-form-skip-test-help = Gebruik dit als de test blijft hangen (komt vaak voor bij sommige BlueMind/Zimbra-installaties). Je kunt de verbinding later testen.
source-form-save = Wijzigingen opslaan
source-form-add = Agendabron toevoegen
source-form-help-google-configured = Klik op de knop hieronder om calrs toegang te geven tot je Google Agenda.
source-form-help-google-unconfigured = De koppeling met Google Agenda is nog niet ingesteld. Vraag je beheerder om Google OAuth2-gegevens in te stellen in het beheerpaneel.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — Gebruik het DAV-endpoint van je BlueMind-server.<br> Meestal: <code>https://mail.yourcompany.com/dav/</code><br> De gebruikersnaam is je <strong>e-mailadres</strong> (bijv. <code>alice@yourcompany.com</code>), niet alleen de inlognaam.<br> Blijft de verbindingstest hangen? Vink dan "Verbindingstest overslaan" aan en probeer direct te synchroniseren.
source-form-help-nextcloud = <strong>Nextcloud</strong> — Gebruik de WebDAV-root, niet de URL van een specifieke agenda.<br> Meestal: <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — Gebruik je volledige e-mailadres in het URL-pad.<br> Voorbeeld: <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Gebruik een app-specifiek wachtwoord (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — Gebruik <code>https://caldav.icloud.com/</code><br> Je hebt een app-specifiek wachtwoord nodig van <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Inloggen en beveiliging &rarr; App-specifieke wachtwoorden).
source-form-help-zimbra = <strong>Zimbra</strong> — Gebruik het DAV-endpoint van je Zimbra-server.<br> Meestal: <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — Gebruik het SOGo-DAV-endpoint.<br> Meestal: <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — Gebruik de root-URL van de server.<br> Meestal: <code>https://cal.example.com/</code>
source-form-help-baikal = <strong>Baikal</strong>: Gebruik het DAV-endpoint van je Baikal-server.<br> Meestal: <code>https://cal.example.com/dav.php/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong>. Gebruik het SOAP-endpoint:<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> De gebruikersnaam is het e-mailadres van de mailbox; voor dit account moet inloggen met HTTP Basic-authenticatie via TLS zijn toegestaan (schakel dit in op een servicemailbox als je tenant Basic heeft uitgeschakeld).<br> Kies ook <strong>Microsoft Exchange (EWS)</strong> in het Backend-menu hierboven.
source-form-help-google = <strong>Google Agenda</strong>: koppelen via OAuth2. Geen wachtwoord nodig.<br>
source-form-help-other = Voer de <strong>DAV-root-URL</strong> van je CalDAV-server in — niet een specifieke agenda of openbare link.<br> calrs vindt je agenda's automatisch via PROPFIND (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Vet
editor-italic-short = Cursief
editor-link-short = Link invoegen

# Team creation (templates/team_form.html)

team-form-heading = Nieuw team
team-form-name = Teamnaam
team-form-name-placeholder = Engineering
team-form-slug = Slug
team-form-slug-hint = (URL-vriendelijke naam)
team-form-slug-pattern-title = Alleen kleine letters, cijfers en streepjes
team-form-description = Beschrijving
team-form-optional = (optioneel)
team-form-description-placeholder = Waar dit team zich mee bezighoudt...
team-form-description-help = Wordt getoond op de teampagina. Ondersteunt **vet**, *cursief* en [links](url).
team-form-visibility = Zichtbaarheid
team-form-public = Openbaar
team-form-private = Privé
team-form-visibility-help = Privéteams krijgen een uitnodigingstoken om te delen. Openbare teams zijn zichtbaar op de teamprofielpagina.
team-form-members = Leden
team-form-members-help = Je wordt automatisch toegevoegd als teambeheerder. Voeg losse gebruikers toe of koppel OIDC-groepen.
team-form-search-placeholder = Zoek gebruikers of groepen...
team-form-search-users = Gebruikers
team-form-search-groups = OIDC-groepen
team-form-you = (jij)
team-form-submit = Team aanmaken

# Team settings (templates/team_settings.html)

team-settings-page-title = Instellingen
team-settings-subtitle = Teaminstellingen — teambeheerders kunnen deze bewerken.
team-settings-public-url = Openbare URL
team-settings-public-url-help = Iedereen kan via deze link boeken.
team-settings-invite-link = Uitnodigingslink
team-settings-invite-link-help = Deel deze link om mensen toegang te geven tot de boekingspagina van dit privéteam.
team-settings-avatar = Teamavatar
team-settings-profile = Profiel
team-settings-description-placeholder = Vertel mensen over dit team...
team-settings-description-help = Wordt getoond op de openbare boekingspagina van het team. Ondersteunt **vet**, *cursief* en [links](url).
team-settings-visibility-help = Openbare teams worden vermeld op de teamprofielpagina. Voor privéteams is een uitnodigingslink nodig.
team-settings-members-help = Beheer wie bij dit team hoort. Voeg losse gebruikers toe of koppel OIDC-groepen voor automatische synchronisatie.
team-settings-time-off-help = Geplande afwezigheid van de leden, in jouw tijdzone, { $timezone }. Hun notities blijven privé.
team-settings-role-member = Lid
team-settings-role-admin = Beheerder
team-settings-oidc-group = OIDC-groep
team-settings-remove = Verwijderen
team-settings-save = Wijzigingen opslaan
team-settings-danger-zone = Gevarenzone
team-settings-danger-help = Verwijder dit team permanent. Afspraaktypes worden losgekoppeld (niet verwijderd). Dit kan niet ongedaan worden gemaakt.
team-settings-delete = Dit team verwijderen
team-settings-delete-confirm = Team '{ $name }' verwijderen? Dit kan niet ongedaan worden gemaakt.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Afspraaktype bewerken
etf-heading-new = Nieuw afspraaktype
etf-disabled-banner = Dit afspraaktype is uitgeschakeld, dus niemand kan het nog boeken.
etf-team = Team
etf-team-hint = (optioneel — laat leeg voor een persoonlijk afspraaktype)
etf-team-personal = Persoonlijk
etf-scheduling-mode = Planningsmodus
etf-mode-round-robin = Round-robin — toewijzen aan één beschikbaar lid
etf-mode-collective = Collectief — alle leden moeten beschikbaar zijn
etf-scheduling-mode-help = Round-robin wijst toe aan één beschikbaar lid (minst drukke eerst). Collectief vereist dat alle leden tegelijk vrij zijn.
etf-title = Titel
etf-title-placeholder = Kennismakingsgesprek van 30 min
etf-slug = Slug
etf-slug-placeholder = automatisch gegenereerd uit titel
etf-description-placeholder = Een kort kennismakingsgesprek om te bespreken...
etf-description-help = Wordt getoond op de boekingspagina. Ondersteunt **vet**, *cursief* en [links](url).
etf-location = Locatie
etf-location-link = Videogesprek (vaste URL)
etf-location-jitsi = Jitsi (automatisch aangemaakte ruimte)
etf-location-webhook = Webhook (eigen provider)
etf-location-phone = Telefoon
etf-location-in-person = Op locatie
etf-location-custom = Aangepast
etf-location-details = Details
etf-location-details-placeholder = https://meet.example.com/my-room
etf-pattern-placeholder = Laat leeg om het standaardpatroon van de organisatie te gebruiken
etf-duration = Duur (minuten)
etf-slot-interval = Tijdslotinterval (minuten)
etf-slot-interval-placeholder = Gelijk aan duur
etf-slot-interval-help = Hoe vaak een tijdslot begint. Laat leeg om de duur aan te houden.
etf-required-members = Verplichte leden
etf-required-members-help = Alle aangevinkte leden moeten vrij zijn voordat een tijdslot wordt aangeboden. Vink leden uit die je wilt uitsluiten (hun beschikbaarheid wordt dan genegeerd).
etf-member-priority = Prioriteit van leden
etf-member-priority-help = Leden met een hogere prioriteit krijgen boekingen als eerste toegewezen als ze beschikbaar zijn. Gelijke prioriteit = verdeeld op basis van het aantal recente boekingen.
etf-member-timezone-title = Tijdzone van het lid. Diens persoonlijke werktijden worden in deze tijdzone geïnterpreteerd.
etf-priority-high = Hoog
etf-priority-medium = Gemiddeld
etf-priority-low = Laag
etf-section-availability = Beschikbaarheid
etf-timezone-help = De tijden hieronder gelden in deze tijdzone. Kies voor team-afspraaktypes de werktijdzone van het team (niet per se die van de maker).
etf-reset-default = Terugzetten naar mijn standaard
etf-reset-default-title = Vervang deze tijden door de standaardbeschikbaarheid uit je profiel
etf-availability-prefilled = Vooraf ingevuld vanuit je { $link }. Je kunt deze hier aanpassen voor dit afspraaktype.
etf-availability-prefilled-link = standaardbeschikbaarheid
etf-section-buffers = Buffertijden en aankondiging
etf-buffer-before = Buffertijd vooraf (min)
etf-buffer-after = Buffertijd achteraf (min)
etf-min-notice = Minimale aankondigingstijd
etf-min-notice-help = Hoe ver van tevoren iemand moet boeken.
etf-section-limits = Boekingslimieten
etf-first-slot-only = Eén tijdslot per dag
etf-first-slot-only-help = Toon alleen de vroegst beschikbare tijd per dag.
etf-freq-limit = Boekingsfrequentie beperken
etf-freq-limit-help = Beperk hoe vaak deze afspraak per periode geboekt kan worden.
etf-add-limit = Limiet toevoegen
etf-section-options = Boekingsopties
etf-requires-confirmation = Bevestiging vereist
etf-requires-confirmation-help = Boekingen blijven in afwachting totdat je ze goedkeurt via het dashboard.
etf-sms = Sms-meldingen
etf-sms-off = Uit, geen telefoonnummer gevraagd
etf-sms-optional = Optioneel, gasten mogen een nummer opgeven
etf-sms-required = Verplicht, gasten moeten een nummer opgeven
etf-sms-help = Stuurt de gast naast een e-mail ook een sms wanneer de boeking is bevestigd, verzet, geannuleerd of op het punt staat te beginnen. Gasten die het veld leeg laten, krijgen gewoon geen sms. Vereist een sms-gateway in het { $link }.
etf-admin-panel-link = beheerpaneel
etf-additional-guests = Extra gasten
etf-guests-none = Gasten kunnen niemand anders toevoegen
etf-additional-guests-help = Sta degene die boekt toe om extra deelnemers uit te nodigen die de agenda-uitnodiging ontvangen.
etf-default-view = Standaard agendaweergave
etf-view-month = Maand — kalenderraster met lijst van tijdsloten
etf-view-week = Week — 7 kolommen met tijdsloten
etf-view-column = Kolom — dagen onder elkaar met tijdsloten
etf-view-week-short = week
etf-view-column-short = kolom
etf-default-view-help = De weergave die gasten standaard zien. Ze kunnen altijd van weergave wisselen.
etf-conflict-calendars = Agenda's voor conflicten
etf-conflict-calendars-help = Kies welke agenda's worden gecontroleerd op conflicten. Als je niets selecteert, worden je bezette agenda's gecontroleerd, maar niet de agenda's die collega's met je hebben gedeeld: selecteer alle agenda's die moeten blokkeren, ook gedeelde.
etf-no-resources = Nog geen gedeelde faciliteiten ingesteld. Voeg er een toe (demolab, vergaderruimte) in het { $link } om die hier verplicht te maken.
etf-section-access = Toegang en meldingen
etf-visibility-public = Openbaar — zichtbaar op je profiel
etf-visibility-internal = Intern — elke collega kan uitnodigingslinks maken
etf-visibility-private = Privé — alleen via uitnodigingslink
etf-visibility-help = Bepaalt wie dit afspraaktype kan zien en boeken.
etf-vis-internal = Intern
etf-reminder = Boekingsherinnering
etf-reminder-none = Geen herinnering
etf-reminder-help = Stuur jou en je gast vóór de afspraak een herinnering per e-mail.
etf-guest-footer = E-mailvoettekst
etf-guest-footer-placeholder = bijv. Vragen? Telefoon: +31 20 123 4567
etf-guest-footer-help = Wordt onderaan elke e-mail geplaatst die de gast over deze boeking ontvangt, in elke taal. Vervangt de standaardvoettekst van de beheerder. Alleen vet, cursief en links.
etf-dynamic-group = Dynamische groepslink
etf-dynamic-group-help = Maak een ad-hoc afspraaklink die de beschikbaarheid van jou en andere gebruikers controleert.
etf-dynamic-group-search = Zoek een gebruiker om toe te voegen...
etf-dynamic-group-note = Alleen gebruikers die dynamische groepslinks toestaan, worden getoond.
etf-dynamic-group-url = URL van groepslink
etf-watcher-teams = Meekijkende teams
etf-watcher-teams-help = Geselecteerde teams krijgen een melding als er geboekt wordt. Leden kunnen boekingen overnemen om als deelnemer aan te sluiten.
etf-save = Wijzigingen opslaan
etf-create = Afspraaktype aanmaken
etf-js-loading = Laden...
etf-js-no-default = Geen standaard ingesteld
etf-js-reset-done = Teruggezet!
etf-js-error = Fout
etf-js-remove-limit = Limiet verwijderen
etf-period-day = Per dag
etf-period-week = Per week
etf-period-month = Per maand
etf-period-year = Per jaar

# Event type form: runtime summary hints (templates/event_type_form.html)


# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = Geen dagen ingesteld
etf-hint-every-day = Elke dag
etf-fmt-day-one = %1 dag
etf-fmt-day-other = %1 dagen
etf-fmt-hours = %1 u
etf-fmt-minutes = %1 min
etf-hint-buffer-both = %1 min vooraf, %2 min achteraf
etf-hint-buffer-before = %1 min buffertijd vooraf
etf-hint-buffer-after = %1 min buffertijd achteraf
etf-hint-notice = minimaal %1 vooraf
etf-hint-no-buffers = Geen buffertijden, altijd te boeken
etf-hint-max = Max. %1
etf-hint-period-day = /dag
etf-hint-period-week = /week
etf-hint-period-month = /maand
etf-hint-period-year = /jaar
etf-hint-no-limits = Geen limieten
etf-hint-confirmation-required = Bevestiging vereist
etf-hint-auto-confirmed = Automatisch bevestigd
etf-hint-extra-guests-one = maximaal %1 extra gast
etf-hint-extra-guests-other = maximaal %1 extra gasten
etf-hint-view = %1weergave
etf-hint-reminder = herinnering %1 van tevoren
etf-hint-no-reminder = geen herinnering

etf-guests-up-to =
    { $count ->
        [one] Maximaal { $count } extra gast
       *[other] Maximaal { $count } extra gasten
    }

etf-reminder-hours =
    { $count ->
        [one] { $count } uur van tevoren
       *[other] { $count } uur van tevoren
    }

etf-reminder-days =
    { $count ->
        [one] { $count } dag van tevoren
       *[other] { $count } dagen van tevoren
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Je maakt een <strong>openbaar</strong> afspraaktype aan &mdash; iedereen met de link kan boeken.
etf-preset-private = Je maakt een <strong>privé</strong> afspraaktype aan &mdash; alleen mensen die je uitnodigt kunnen boeken.
etf-preset-internal = Je maakt een <strong>intern</strong> afspraaktype aan &mdash; elke collega kan de boekingslink delen.
etf-preset-team = Je maakt een <strong>team</strong>-afspraaktype aan &mdash; boekingen worden over de teamleden verdeeld.
etf-pattern-hint = Optioneel afwijkend patroon. Tokens: <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Laat leeg om de standaard van de organisatie te gebruiken die een beheerder heeft ingesteld.
etf-pattern-random-warning = Dit patroon bevat geen <code>{"{"}random{"}"}</code>-token. Twee boekingen van dit afspraaktype op dezelfde dag delen dan dezelfde ruimte, en de tweede gast kan zo de afspraak van de eerste gast binnenlopen. Gebruik vaste ruimtes alleen als je dat echt wilt.
etf-webhook-hint = De afspraak-URL per boeking wordt opgehaald via de webhook die een beheerder heeft ingesteld onder Beheerpaneel &rarr; Vergaderwebhook. Hier is geen URL nodig.

# Admin panel (templates/admin.html)

admin-page-title = Beheer
admin-action-refused = Actie geweigerd:
admin-error-last-admin = Dit is de laatste actieve beheerder. Maak eerst iemand anders beheerder of schakel een andere beheerder in.
admin-logo = Bedrijfslogo
admin-logo-help = Wordt getoond op openbare boekingspagina's. PNG, JPEG, GIF of WebP, max. 2 MB.
admin-logo-refused = Het logo is niet opgeslagen. Upload een PNG-, JPEG-, GIF- of WebP-afbeelding van maximaal 2 MB.
admin-favicon = Favicon
admin-favicon-help = Verschijnt in browsertabbladen en bladwijzers. Aanbevolen: een vierkante PNG van minstens 32×32 pixels, max. 512 KB. Zonder eigen favicon wordt de calrs-krab getoond.
admin-favicon-refused = Het favicon is niet opgeslagen. Upload een PNG-, JPEG-, GIF- of WebP-afbeelding van maximaal 512 KB.
admin-upload-save-failed = Het bestand kon niet in de gegevensmap van de server worden geschreven. De serverlog bevat de details.

# User and team avatar uploads (#307)
avatar-upload-refused = De afbeelding is niet opgeslagen. Upload een PNG-, JPEG-, GIF- of WebP-afbeelding van maximaal 2 MB.
avatar-upload-save-failed = De afbeelding kon niet op de server worden opgeslagen. Probeer het later opnieuw, of vraag je beheerder het serverlogboek te bekijken.
admin-company-link = Bedrijfslink
admin-company-link-help = Het logo linkt naar deze URL op openbare boekingspagina's. Laat leeg voor geen link.
admin-theme = Thema
admin-theme-help = Kies een kleurthema voor alle pagina's. De schakelaar voor donker/licht staat hier los van: thema's passen zich aan beide modi aan.
admin-theme-default = Standaard
admin-theme-default-desc = Strak blauw
admin-theme-nord-desc = Arctische vorst
admin-theme-dracula-desc = Donkerpaars
admin-theme-gruvbox-desc = Warm retro
admin-theme-solarized-desc = De klassieker van Ethan
admin-theme-tokyo-desc = Neonstad
admin-theme-custom = Aangepast
admin-theme-custom-desc = Jouw kleuren
admin-custom-colors = Aangepaste kleuren
admin-color-accent = Accent
admin-color-accent-hover = Accent (hover)
admin-color-bg = Achtergrond
admin-color-surface = Oppervlak
admin-color-text = Tekst
admin-save-theme = Thema opslaan
admin-users = Gebruikers
admin-user-filter = Filter op naam of e-mail…
admin-badge-admin = beheerder
admin-badge-disabled = uitgeschakeld
admin-impersonate = Inloggen als
admin-demote = Degraderen
admin-promote = Promoveren
admin-disable = Uitschakelen
admin-enable = Inschakelen
admin-delete = Verwijderen
admin-no-users-match = Geen gebruikers gevonden met dit filter.
admin-no-users = Nog geen gebruikers.
admin-groups = Groepen
admin-group-filter = Filter op groepsnaam…
admin-group-name = Groepsnaam
admin-weight = gewicht:
admin-no-groups-match = Geen groepen gevonden met dit filter.
admin-no-groups = Nog geen groepen gesynchroniseerd. Groepen worden automatisch gesynchroniseerd vanuit je OIDC-provider.
admin-auth-settings = Authenticatie-instellingen
admin-registration-enabled = Registratie ingeschakeld
admin-allowed-domains = Toegestane e-maildomeinen
admin-allowed-domains-hint = (kommagescheiden, laat leeg om alles toe te staan)
admin-save-auth = Authenticatie-instellingen opslaan
admin-system-settings = Systeeminstellingen
admin-base-url = Basis-URL
admin-base-url-help = Openbare URL van deze installatie. Wordt gebruikt voor OIDC-redirects en links in e-mails (goedkeuren/afwijzen, annuleren, herinneringen).
admin-private-hosts = Toegestane privéhosts
admin-private-hosts-help = Kommagescheiden hostnamen die naar privé- of gereserveerde IP-adressen mogen resolven voor CalDAV/EWS-bronnen (uitzondering op de SSRF-beveiliging). Voeg alleen hosts toe die je zelf beheert (bijv. een agendaserver op hetzelfde Docker-netwerk). Laat leeg om de beveiliging voor alle hosts actief te houden.
admin-german-formal = Aanspreekvorm in het Duits
admin-german-formal-du = Informeel (du)
admin-german-formal-sie = Formeel (Sie)
admin-german-formal-help = Hoe pagina's en e-mails bezoekers aanspreken van wie de browser om Duits vraagt. Wie een taal koos in Profiel en instellingen, houdt die.
admin-unset-env = Verwijder de omgevingsvariabele om dit hier te kunnen bewerken.
admin-save-system = Systeeminstellingen opslaan
admin-status = Status:
admin-status-enabled = ingeschakeld
admin-status-disabled = uitgeschakeld
admin-status-disabled-paren = (uitgeschakeld)
admin-status-configured = geconfigureerd
admin-status-not-configured = niet geconfigureerd
admin-via-environment = (via omgevingsvariabelen)
admin-issuer = Issuer:
admin-client-id = Client-ID:
admin-instance = Instantie:
admin-oidc-settings = OIDC-instellingen
admin-oidc-enabled = OIDC ingeschakeld
admin-issuer-url = Issuer-URL
admin-client-id-label = Client-ID
admin-client-secret = Client-secret
admin-keep-current-hint = (laat leeg om de huidige waarde te behouden)
admin-keep-current-set-hint = (laat leeg om de huidige waarde te behouden, momenteel ingesteld)
admin-keep-unchanged = Laat leeg om ongewijzigd te laten
admin-oidc-auto-register = Nieuwe gebruikers automatisch registreren via OIDC
admin-save-oidc = OIDC-instellingen opslaan
admin-google = Google Agenda (OAuth2)
admin-save-google = Google OAuth2-instellingen opslaan
admin-captcha = Captcha
admin-instance-url = Instantie-URL
admin-site-key = Sitesleutel
admin-secret = Secret
admin-widget-url = URL van widgetscript
admin-widget-url-help = Overschrijf dit als het CDN geblokkeerd is. Wijzigingen gaan direct na opslaan in.
admin-wasm-url = URL van widget-WASM
admin-wasm-url-help = Waar de widget zijn oplosser vandaan laadt. Vul het WASM-bestand van je Cap-server in, zodat boekingspagina's geen contact meer maken met cdn.jsdelivr.net. Laat leeg voor de standaardwaarde.
admin-captcha-wasm-url-invalid = De URL van de widget-WASM moet een absolute http(s)-URL zijn.
admin-captcha-disable-help = Laat instantie-URL, sitesleutel en secret leeg om captcha op boekingspagina's uit te schakelen.
admin-save-captcha = Captcha-instellingen opslaan
admin-resources = Faciliteiten
admin-resources-help = Gedeelde boekbare faciliteiten (demolab, vergaderruimtes) op basis van een agendafeed. Gekoppeld aan afspraaktypes blokkeert een bezette faciliteit boekingen.
admin-resource-stats = Afspraken in cache: { $events } &middot; Gekoppeld aan { $attached } afspraaktype(s)
admin-never = nooit
admin-resource-sync-failed = (laatste poging mislukt: { $error })
admin-writeback-enabled = Terugschrijven: ingeschakeld ({ $via })
admin-writeback-readonly = Terugschrijven: alleen-lezen
admin-teams-allowed = Toegestane teams:
admin-teams-allowed-none = geen (alleen globale beheerders)
admin-sync-now = Nu synchroniseren
admin-test-write = Schrijven testen
admin-delete-resource-confirm = Deze faciliteit verwijderen? Afspraaktypes die haar gebruiken, controleren haar dan niet meer.
admin-name = Naam
admin-name-help = Laat leeg om de naam uit de feed op te halen.
admin-feed-url = ICS-feed-URL (publicatieadres)
admin-feed-url-help = BlueMind: het openbare of privé-agendaadres van de faciliteitenagenda.
admin-caldav-url = CalDAV-collectie-URL (voor terugschrijven)
admin-caldav-url-help = Optioneel. Voor BlueMind wordt deze automatisch afgeleid van de feed-URL.
admin-caldav-username = CalDAV-gebruikersnaam
admin-caldav-password = CalDAV-wachtwoord
admin-resource-teams = Teams die deze faciliteit mogen gebruiken
admin-resource-teams-help = Teambeheerders van deze teams kunnen deze faciliteit koppelen aan de afspraaktypes van hun team. Leeg: alleen globale beheerders.
admin-no-teams = Nog geen teams.
admin-save-resource = Faciliteit opslaan
admin-add-resource = Faciliteit toevoegen
admin-jitsi = Jitsi (automatisch gegenereerde vergaderlinks)
admin-jitsi-help = Als de locatie van een afspraaktype is ingesteld op "Jitsi (automatisch gegenereerde ruimte)", maakt calrs per boeking een nieuwe ruimte-URL door het onderstaande patroon aan je Jitsi-basis-URL toe te voegen. Er is geen externe API-aanroep nodig.
admin-display-name = Weergavenaam
admin-jitsi-display-name-placeholder = bijv. Meet DYB
admin-jitsi-display-name-help = Wordt aan gasten getoond bij de tijdslotkiezer en het boekingsformulier. Standaard "Videogesprek" als dit leeg is.
admin-room-pattern = Patroon voor ruimtenaam
admin-jitsi-disable-help = Laat de basis-URL leeg om het automatisch genereren van Jitsi-links uit te schakelen.
admin-save-jitsi = Jitsi-instellingen opslaan
admin-meeting-webhook = Vergaderwebhook (eigen provider)
admin-webhook-url = Webhook-URL
admin-webhook-display-name-placeholder = bijv. Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Wordt aan gasten getoond in plaats van het algemene label "Videogesprek".
admin-authentication = Authenticatie
admin-auth-none = Geen
admin-auth-hmac = HMAC-SHA256 (X-Calrs-Signature-header)
admin-shared-secret = Gedeeld secret
admin-webhook-disable-help = Laat de URL leeg om de vergaderwebhook uit te schakelen.
admin-save-webhook = Webhookinstellingen opslaan
admin-smtp = SMTP-instellingen
admin-smtp-test-sent = Testmail verzonden.
admin-smtp-test-failed = De testmail kon niet worden verzonden. Controleer de serverlogs en je SMTP-instellingen.
admin-smtp-env-error = Fout in SMTP-configuratie via omgevingsvariabelen:
admin-smtp-host = Host:
admin-smtp-from = Van:
admin-smtp-enabled = SMTP ingeschakeld
admin-host = Host
admin-port = Poort
admin-tls-mode = TLS-modus
admin-tls-starttls = STARTTLS (poort 587)
admin-tls-implicit = Impliciete TLS (poort 465)
admin-tls-none = Geen, onversleuteld (alleen lokale MTA)
admin-smtp-username-hint = (laat leeg voor een relay zonder authenticatie)
admin-from-email = Afzenderadres
admin-from-name = Afzendernaam
admin-save-smtp = SMTP-instellingen opslaan
admin-send-test-email = Stuur een testmail naar
admin-send-test-email-hint = (standaard het e-mailadres van je account)
admin-send-test-email-btn = Testmail versturen
admin-smtp-clear-confirm = De SMTP-configuratie in de database verwijderen?
admin-clear-db-config = Databaseconfiguratie wissen
admin-sms = SMS-instellingen
admin-sms-help = Optioneel. Er wordt alleen een SMS verstuurd voor boekingen van afspraaktypes waarbij "SMS-meldingen" is ingeschakeld, en alleen als de gast een telefoonnummer heeft opgegeven.
admin-sms-test-sent = Testbericht verzonden.
admin-sms-test-checked = Inloggegevens geaccepteerd.
admin-sms-test-error = De SMS-gateway heeft het verzoek geweigerd.
admin-sms-captcha-warning = Het boekingsformulier is openbaar en het ontvangende nummer komt van de gast, dus SMS zonder captcha is een open relay waarvoor iemand anders jou kosten kan laten maken. Stel hierboven de captcha in en beperk de bestemmingslanden in de instellingen van je gateway zelf.
admin-sms-sent-today = Vandaag verzonden:
admin-sms-segments-today = Vandaag gefactureerde segmenten:
admin-sms-unknown-today = Zonder antwoord van de gateway, meegeteld voor de limiet:
admin-sms-of-cap = van { $cap }
admin-sms-config-error = Fout in SMS-configuratie:
admin-sms-gateway = Gateway:
admin-sms-account = Account:
admin-sms-sender = Afzender:
admin-sms-enabled = SMS ingeschakeld
admin-sms-gateway-label = Gateway
admin-required-on-switch = Verplicht bij het wisselen van gateway
admin-sms-docs = API-documentatie van { $provider }
admin-sms-country = Standaard landcode
admin-sms-country-hint = (gebruikt als gasten een lokaal telefoonnummer invoeren)
admin-sms-daily-cap = Daglimiet
admin-sms-daily-cap-hint = (gefactureerde SMS-segmenten per UTC-dag voor de hele installatie, 0 voor geen limiet)
admin-sms-daily-cap-help = Een bericht tot 160 tekens is één segment, of tot 70 als het een teken buiten het GSM-alfabet bevat; een langer bericht kost één segment per 153 tekens (67), en testberichten tellen ook mee. Boven de limiet stopt calrs met SMS'en en blijft het e-mails versturen, zodat boekingen nooit mislukken omdat het SMS-budget op is.
admin-save-sms = SMS-instellingen opslaan
admin-send-test-sms = Stuur een testbericht naar
admin-send-test-sms-hint-check = (laat leeg om alleen de inloggegevens te controleren)
admin-send-test-sms-hint-e164 = (E.164-notatie)
admin-test-gateway = Gateway testen
admin-sms-clear-confirm = De SMS-configuratie in de database verwijderen?
admin-sms-allow-all = Alle gebruikers SMS laten inschakelen voor hun afspraaktypes
admin-sms-allow-all-help = Standaard uit: SMS kost tegoed op het hier ingestelde account, dus alleen beheerders mogen een afspraaktype in een SMS-modus zetten.
admin-save-policy = Beleid opslaan
admin-guest-footer = Voettekst van e-mails aan gasten
admin-guest-footer-help = Een korte tekst onderaan elke e-mail aan gasten en deelnemers: bevestigingen, herinneringen, annuleringen enzovoort. E-mails aan de host en aan het teamlid dat de boeking heeft overgenomen blijven ongewijzigd.
admin-guest-footer-default = Standaardvoettekst
admin-guest-footer-default-help = Wordt gebruikt als een afspraaktype geen eigen voettekst heeft. Dezelfde tekst wordt in elke taal verstuurd. Alleen vet, cursief en links.
admin-guest-footer-allow-all = Alle gebruikers een voettekst laten instellen voor hun afspraaktypes
admin-guest-footer-allow-all-help = Standaard uit: de voettekst gaat uit naam van deze installatie, dus alleen beheerders mogen er een schrijven. Een voettekst op een afspraaktype vervangt de standaard hierboven.
admin-save-guest-footer = Voettekst opslaan
admin-page-of = Pagina %1 van %2
admin-show-more-js = Nog %1 tonen
admin-show-fewer = Minder tonen

# Admin panel: overview, section tabs, users table (templates/admin.html)

admin-copy = Kopiëren
admin-copied = Gekopieerd
admin-sections = Beheeronderdelen
admin-tab-overview = Overzicht
admin-tab-users = Gebruikers
admin-tab-access = Inloggen en beveiliging
admin-tab-notifications = E-mail en sms
admin-tab-meetings = Vergaderlinks
admin-tab-resources = Faciliteiten
admin-tab-branding = Huisstijl
admin-tab-system = Systeem
admin-att-title = Vraagt aandacht
admin-mast-users = { $count ->
    [one] { $count } gebruiker
   *[other] { $count } gebruikers
}
admin-mast-bookings = { $count ->
    [one] { $count } boeking
   *[other] { $count } boekingen
}
admin-mast-attention = { $count ->
    [one] { $count } punt vraagt aandacht.
   *[other] { $count } punten vragen aandacht.
}
admin-mast-clear = Niets vraagt aandacht.
admin-sev-failing = Mislukt
admin-sev-missing = Ontbreekt
admin-sev-off = Uit
admin-sev-exposed = Onbeschermd
admin-sev-expiring = Verloopt
admin-sev-stale = Verouderd
admin-fig-title = Kerncijfers
admin-chart-week-short = Wk { $week }
admin-chart-now = nu
admin-chart-ended-key = Geannuleerd of afgewezen, geteld onder elke week
admin-pct = { $value }%
admin-att-review = Bekijken
admin-att-smtp-error = De SMTP-configuratie uit de omgeving is ongeldig: er worden geen e-mails verzonden.
admin-att-smtp-missing = E-mail is niet geconfigureerd: gasten en hosts krijgen geen bevestigingen, herinneringen of annuleringen.
admin-att-smtp-disabled = Het verzenden van e-mail staat uit.
admin-att-sources-failing = Agendabronnen waarvan de synchronisatie mislukt
admin-att-writeback = Komende boekingen die ontbreken in de agenda van de host
admin-att-sms-captcha = Sms is geconfigureerd zonder captcha: het openbare boekingsformulier kan worden gebruikt om betaalde berichten te versturen.
admin-att-resources-failing = Faciliteitenfeeds waarvan de synchronisatie mislukt
admin-att-pending-expiring = Openstaande aanvragen die binnen 24 uur verlopen
admin-att-sources-stale = Agendabronnen die al meer dan 24 uur niet zijn gesynchroniseerd
admin-att-base-url = De openbare basis-URL is niet ingesteld: links in e-mails en doorverwijzingen voor single sign-on werken mogelijk niet.
admin-kpi-bookings = Boekingen, laatste 30 dagen
admin-kpi-prev = Vorige 30 dagen: { $count }
admin-kpi-upcoming = Komende afspraken
admin-kpi-upcoming-week = Komende 7 dagen: { $count }
admin-kpi-pending = Openstaande aanvragen
admin-kpi-pending-expiring = Verlopen binnen 24 uur: { $count }
admin-kpi-pending-none = Niets wacht op goedkeuring
admin-kpi-pending-ok = Geen enkele verloopt binnen 24 uur
admin-kpi-active = Actieve gebruikers
admin-kpi-active-sub = In de laatste 30 dagen ingelogd, van alle ingeschakelde accounts
admin-chart-weekly = Boekingen per week
admin-chart-weekly-help = Boekingen per week over de laatste 12 weken, naar hun huidige status.
admin-series-confirmed = Bevestigd
admin-series-pending = In afwachting
admin-series-ended = Geannuleerd of afgewezen
admin-chart-bar-label = Week van { $week }: { $confirmed } bevestigd, { $pending } in afwachting, { $ended } geannuleerd of afgewezen
admin-chart-table = Als tabel tonen
admin-chart-week = Week van
admin-chart-total = Totaal
admin-chart-empty = Geen boekingen in de laatste 12 weken.
admin-status-title = Boekingen per status
admin-status-empty = Nog geen boekingen.
admin-st-cancelled = Geannuleerd
admin-st-declined = Afgewezen
admin-svc-title = Diensten
admin-svc-email = E-mail (SMTP)
admin-svc-sms = Sms
admin-svc-oidc = Single sign-on (OIDC)
admin-svc-google = Google Agenda
admin-svc-microsoft = Microsoft 365 / Outlook.com
admin-svc-registration = Zelf registreren
admin-svc-jitsi = Jitsi-vergaderlinks
admin-svc-webhook = Vergaderwebhook
admin-svc-base-url = Openbare basis-URL
admin-state-error = fout
admin-top-event-types = Meest geboekte afspraaktypes
admin-top-hosts = Drukst bezette hosts
admin-top-help = Bevestigde boekingen van de laatste 90 dagen.
admin-top-empty = Geen bevestigde boekingen in de laatste 90 dagen.
admin-top-team = team
admin-health-title = Agendastatus
admin-health-help = Synchronisatiestatus van elke agendabron, en boekingen die niet in de agenda van een host konden worden geschreven.
admin-health-healthy = Gezond
admin-health-failing = Mislukt
admin-health-stale = 24 uur niet gesynchroniseerd
admin-health-disabled = Uitgeschakeld
admin-health-writeback-count = Mislukte terugschrijvingen
admin-health-no-sources = Er is nog geen agendabron gekoppeld.
admin-health-failing-title = Bronnen waarvan de synchronisatie mislukt
admin-health-col-source = Bron
admin-health-col-owner = Eigenaar
admin-health-col-error = Laatste fout
admin-health-col-since = Mislukt sinds
admin-health-attempts = Mislukte pogingen: { $count }
admin-health-owner-hint = De beschikbaarheid wordt verder berekend op basis van de laatste geslaagde synchronisatie. De eigenaar ziet de fout op de pagina met agendabronnen en krijgt een e-mail als die aanhoudt.
admin-health-writeback-title = Boekingen die ontbreken in de agenda van de host
admin-health-col-booking = Boeking
admin-health-col-host = Host
admin-health-col-retry = Nieuwe poging
admin-health-retrying = wordt opnieuw geprobeerd
admin-health-retry-exhausted = pogingen opgebruikt
admin-health-resources-title = Faciliteitenfeeds waarvan de synchronisatie mislukt
admin-inv-title = Inventaris
admin-inv-users = Gebruikersaccounts
admin-inv-users-sub = Beheerders: { $admins }, uitgeschakeld: { $disabled }
admin-inv-signin-sub = Wachtwoord: { $local }, SSO: { $sso }
admin-inv-new-users = Nieuwe accounts, 30 dagen
admin-inv-mfa-sub = Lokale accounts met tweestapsverificatie
admin-inv-teams = Teams
admin-inv-groups-sub = OIDC-groepen: { $count }
admin-inv-event-types = Afspraaktypes
admin-inv-event-types-sub = Ingeschakeld: { $enabled }, team: { $team }
admin-inv-bookings = Boekingen
admin-inv-bookings-sub = Bevestigd: { $count }
admin-inv-sources = Agendabronnen
admin-inv-sources-sub = Ingeschakeld: { $count }
admin-inv-resources-failing = Mislukte feeds: { $count }
admin-inv-sms = Sms, laatste 30 dagen
admin-inv-sms-today = vandaag: { $count }
admin-users-filter-label = Gebruikers filteren
admin-filter-all = Alle
admin-filter-admins = Beheerders
admin-filter-disabled = Uitgeschakeld
admin-filter-never = Nooit ingelogd
admin-provider-local = Wachtwoord
admin-provider-oidc = SSO
admin-col-user = Gebruiker
admin-col-role = Rol
admin-col-signin = Inloggen
admin-col-last = Laatst ingelogd
admin-col-created = Lid sinds
admin-col-actions = Acties
admin-user-actions = Acties voor { $name }
admin-role-user = lid
admin-badge-mfa = 2FA
admin-delete-confirm-btn = Definitief verwijderen
admin-users-shown = %1 van %2 gebruikers

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Gebruiker { $email } definitief verwijderen?{"\u000A"}{"\u000A"}Hiermee worden het gebruikersrecord, het planningsaccount, de agendabronnen, de afspraaktypes en alle gegevens die alleen van deze gebruiker zijn verwijderd. Eerdere boekingen worden samen met hun afspraaktypes verwijderd.{"\u000A"}{"\u000A"}Voor OIDC/SSO-gebruikers: als automatisch registreren is ingeschakeld, wordt deze persoon bij de volgende keer inloggen opnieuw aangemaakt.{"\u000A"}{"\u000A"}Dit kan niet ongedaan worden gemaakt.
admin-system-settings-help = Openbare URL en netwerkbeveiligingsinstellingen. Deze kun je ook instellen met de omgevingsvariabelen <code>CALRS_BASE_URL</code> en <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Als een omgevingsvariabele is ingesteld, <strong>heeft die voorrang</strong> op de waarde hieronder.
admin-set-by-env = ingesteld via omgevingsvariabele ({ $var }), overschrijft de opgeslagen waarde
admin-google-help = Om de koppeling met Google Agenda in te schakelen, maak je OAuth2-inloggegevens aan in de <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Schakel de <strong>CalDAV API</strong> (bezette tijden lezen en boekingen terugschrijven) en de <strong>Google Calendar API</strong> (Google Meet-links) in en voeg daarna { $redirect_uri } toe als geautoriseerde redirect-URI.
admin-room-pattern-help = Beschikbare tokens: <code>{"{"}username{"}"}</code> (organisator), <code>{"{"}event{"}"}</code> (slug van het afspraaktype), <code>{"{"}date{"}"}</code> (JJJJMMDD), <code>{"{"}random{"}"}</code> (8 tekens). Standaard: { $default }.
admin-room-pattern-warning = Zonder <code>{"{"}random{"}"}</code> is de ruimtenaam voorspelbaar: twee gasten die op dezelfde dag hetzelfde afspraaktype boeken, delen dan een ruimte en kunnen elkaars vergadering zien. Vaste ruimtes zijn toegestaan (bijv. één persoonlijke ruimte per organisator), maar schakel dit alleen in als je de afweging begrijpt.
admin-meeting-webhook-help = Als de locatie van een afspraaktype is ingesteld op "Webhook (eigen provider)", stuurt calrs bij bevestiging de boekingsgegevens via een POST naar deze URL en verwacht een JSON-body <code>{"{"}"url": "https://..."{"}"}</code> terug.
admin-auth-hmac-help = Met HMAC stuurt calrs <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> mee over de ruwe request-body.
admin-tls-none-warning = Kies <strong>Geen</strong> alleen voor een relay op deze machine die geen STARTTLS biedt of een zelfondertekend certificaat heeft. E-mail en eventuele inloggegevens gaan dan onversleuteld over het netwerk.
admin-smtp-env-error-help = Corrigeer de omgevingsvariabelen <code>CALRS_SMTP_*</code>, of verwijder ze om SMTP hier via de database te beheren.
admin-smtp-env-managed = Beheerd via <strong>omgevingsvariabelen</strong> (overschrijven de database). Pas de variabelen <code>CALRS_SMTP_*</code> aan om dit te wijzigen, of verwijder ze om SMTP hier te beheren.
admin-smtp-env-help = Je kunt dit ook instellen via omgevingsvariabelen (die dit overschrijven): <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> of <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Alleen <code>CALRS_SMTP_HOST</code> en <code>CALRS_SMTP_FROM_EMAIL</code> zijn verplicht; laat gebruikersnaam en wachtwoord weg om zonder authenticatie via een lokale MTA te versturen.
admin-sms-env-error-help = Corrigeer de omgevingsvariabelen <code>CALRS_SMS_*</code>, of verwijder ze om SMS hier via de database te beheren.
admin-sms-env-managed = Beheerd via <strong>omgevingsvariabelen</strong> (overschrijven de database). Pas de variabelen <code>CALRS_SMS_*</code> aan om dit te wijzigen, of verwijder ze om SMS hier te beheren.
admin-sms-env-help = Je kunt dit ook instellen via omgevingsvariabelen (die dit overschrijven): <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>De proefmodus van Twilio staat aan</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Gasten ontvangen het vooraf gedefinieerde <code>sms_appointment_reminders</code>-sjabloon van Twilio, niet het echte bericht, en alleen nummers die in je Twilio-console zijn geverifieerd worden bereikt. Dit is een testhulpmiddel voor proefaccounts. Verwijder de variabele voordat je boekingen aanneemt.

admin-show-more =
    { $count ->
        [one] Nog { $count } tonen
       *[other] Nog { $count } tonen
    }

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Kies het protocol dat je server gebruikt. EWS is bedoeld voor on-premises Exchange 2019/2016/2013.

admin-sms-going-live = <strong>Voordat je live gaat:</strong> beperk de bestemmingslanden in je gateway (Twilio noemt dit Geo Permissions), houd het account prepaid zonder automatisch opwaarderen en laat de captcha aan. Samen beperken die drie wat een poging tot SMS pumping je kan kosten.

troubleshoot-heading = Problemen met beschikbaarheid oplossen

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = Naam en slug zijn verplicht.
form-error-team-name-length = De naam mag maximaal 255 tekens lang zijn.
form-error-team-description-length = De beschrijving mag maximaal 5000 tekens lang zijn.
form-error-slug-charset = De slug mag alleen kleine letters, cijfers en streepjes bevatten.
form-error-slug-reserved = Deze slug is gereserveerd. Kies een andere.
form-error-team-slug-taken = Er bestaat al een team met deze slug.
form-error-title-required = Een titel is verplicht om een slug te genereren.
form-error-event-type-slug-taken = Er bestaat al een afspraaktype met deze slug.
form-error-event-type-slug-taken-team = Er bestaat in dit team al een afspraaktype met deze slug.
form-error-event-type-title-required = Een titel is verplicht.
form-error-event-type-title-length = De titel mag hoogstens 255 tekens lang zijn.
form-error-event-type-duration = De duur moet een geheel aantal minuten zijn, van 1 tot 1440 (24 uur).
form-error-event-type-number = Buffertijden, minimale aankondigingstijd, herinnering en extra gasten moeten gehele getallen zijn, 0 of meer.
form-error-event-type-horizon = De boekingshorizon moet een geheel aantal dagen zijn, 0 of meer. Laat het leeg voor geen limiet.
form-error-location-required = Locatiegegevens zijn verplicht (bijv. een link voor een videogesprek, een telefoonnummer of een adres).
form-error-bookable-range-order = De einddatum moet op of na de begindatum liggen.
form-error-bookable-range-invalid = Vul datums in als JJJJ-MM-DD.
form-error-not-team-admin = Je bent geen teambeheerder van dit team.
form-error-no-account = Geen planningsaccount gevonden. Neem contact op met een beheerder.
form-error-all-fields-required = Alle velden zijn verplicht.
form-error-encryption = Versleutelingsfout.
form-error-connection-failed = Verbinding mislukt: { $error }. Controleer de URL en inloggegevens, of vink "Verbindingstest overslaan" aan om toch op te slaan.

# Settings page flash (src/web/mod.rs)

settings-saved = Instellingen opgeslagen.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = De naam moet tussen 1 en 255 tekens lang zijn.
settings-error-username-length = De gebruikersnaam moet minstens 2 tekens lang zijn.
settings-error-username-taken = Deze gebruikersnaam is al in gebruik.
settings-error-booking-email = Voer een geldig e-mailadres voor boekingen in.
settings-error-save-failed = Instellingen opslaan mislukt.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = Team niet gevonden of je bent geen teambeheerder.
error-team-not-found = Team niet gevonden.
error-event-type-not-found = Afspraaktype niet gevonden.
error-decrypt-failed = Opgeslagen inloggegevens konden niet worden ontsleuteld.
error-source-not-found = Bron niet gevonden.
error-source-no-password = Voor deze bron is geen wachtwoord opgeslagen.
error-oauth-invalid-state = Ongeldige state-parameter. Probeer het opnieuw.
error-oauth-no-code = Geen autorisatiecode ontvangen.
error-oauth-not-configured = Google OAuth2 is niet geconfigureerd.
error-no-scheduling-account = Geen planningsaccount gevonden.
error-private-event-type-not-found = Privé-afspraaktype niet gevonden.
error-access-denied = Toegang geweigerd.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = Dit tijdslot is niet meer beschikbaar.
error-slot-too-soon = Dit tijdslot is niet meer beschikbaar (te kort van tevoren).
error-slot-beyond-horizon = Dit tijdslot valt buiten de boekingsperiode.
error-slot-not-yet-bookable = Dit tijdslot is nog niet te boeken.
error-invite-required = Voor dit afspraaktype is een uitnodigingslink nodig.
error-invite-invalid = Ongeldige uitnodigingslink.
error-invite-expired = Deze uitnodigingslink is verlopen.
error-invite-used = Deze uitnodigingslink is al gebruikt.
error-invalid-date = Ongeldige datum.
error-invalid-time = Ongeldige tijd.
error-invalid-date-format = Ongeldige datumnotatie.
error-invalid-time-format = Ongeldige tijdnotatie.
error-too-many-bookings = Te veel boekingspogingen. Probeer het over een paar minuten opnieuw.
error-too-many-requests = Te veel verzoeken. Probeer het later opnieuw.
error-no-members-available = Er zijn geen teamleden beschikbaar voor dit tijdslot.
error-dynamic-group-public-only = Dynamische groepslinks zijn alleen beschikbaar voor openbare afspraaktypes.
error-user-not-found = Gebruiker niet gevonden.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Captcha-verificatie mislukt
bae-title-invalid-booking = Ongeldige boekingsgegevens
bae-title-unavailable = Nu niet beschikbaar
bae-title-cannot-approve = Deze boeking kan niet worden goedgekeurd
bae-title-invalid-link = Ongeldige link
bae-title-invalid-or-expired = Ongeldige of verlopen link
bae-title-booking-not-found = Boeking niet gevonden
bae-title-already-approved = Al goedgekeurd
bae-title-already-declined = Al afgewezen
bae-title-already-cancelled = Al geannuleerd
bae-title-booking-cancelled = Boeking geannuleerd
bae-title-booking-declined = Boeking afgewezen

# Booking action error page: bodies

bae-body-go-back = Ga terug en probeer het opnieuw.
bae-body-unavailable = De organisator neemt voor deze datum geen boekingen meer aan. Kies een andere datum of kijk later nog eens.
bae-body-resource-gone = Een benodigde faciliteit is op dit tijdstip niet meer beschikbaar. Vraag de gast een ander tijdslot te kiezen.
bae-body-no-claim-token = Geen token voor overnemen opgegeven.
bae-body-claim-invalid = Deze link om over te nemen is niet meer geldig.
bae-body-booking-gone = Deze boeking bestaat niet meer.
bae-body-decline-link-invalid = Deze afwijslink is ongeldig of verlopen, of de boeking is al verwerkt.
bae-body-cancel-link-invalid = Deze annuleringslink is ongeldig of verlopen, of de boeking is al geannuleerd.
bae-body-cancel-link-invalid-short = Deze annuleringslink is ongeldig of verlopen.
bae-body-reschedule-link-invalid = Deze link om te verzetten is ongeldig of verlopen, of de boeking is al verwerkt.
bae-body-approval-link-invalid = Deze goedkeuringslink is ongeldig of verlopen.
bae-body-already-approved = Deze boeking is al goedgekeurd.
bae-body-already-declined = Deze boeking is al afgewezen.
bae-body-already-cancelled = Deze boeking is al geannuleerd.
bae-body-was-cancelled = Deze boeking is geannuleerd.
bae-body-declined-by-host = Deze boeking is afgewezen door de organisator.

# Booking form validation (src/web/mod.rs)

validate-name-length = De naam moet tussen 1 en 255 tekens lang zijn.
validate-email-length = Het e-mailadres moet tussen 1 en 255 tekens lang zijn.
validate-email-invalid = Voer een geldig e-mailadres in.
validate-notes-length = Opmerkingen mogen maximaal 5000 tekens lang zijn.
validate-date-too-far = Je kunt niet meer dan een jaar vooruit boeken.
validate-guest-footer-too-long = De voettekst van de e-mail mag maximaal 500 tekens bevatten.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = Extra gasten zijn niet toegestaan voor dit afspraaktype.
guests-too-many =
    { $max ->
        [one] Je kunt maximaal één extra gast toevoegen.
       *[other] Je kunt maximaal { $max } extra gasten toevoegen.
    }
guests-invalid-email = Ongeldig e-mailadres van extra gast: { $email }
dynamic-group-min-usernames = Voor dynamische groepslinks zijn minstens twee gebruikersnamen nodig.
dynamic-group-user-not-found = Gebruiker "{ $username }" niet gevonden.
dynamic-group-user-opted-out = Gebruiker "{ $username }" heeft dynamische groepslinks niet ingeschakeld.

error-slot-unavailable-member = Dit tijdslot is niet meer beschikbaar ({ $username } heeft een conflict).

# Pending requests: slot hold, expiry and approval re-check (#234)

sms-expired = Aanvraag verlopen: { $event }, { $date } om { $time } ({ $tz }). Geen reactie, kies een ander tijdstip.
email-pending-expired-status = Aanvraag verlopen
email-pending-expired-headline = Je boekingsaanvraag is verlopen: de organisator heeft niet op tijd gereageerd. Kies een ander tijdstip.
email-pending-expired-action = Kies een ander tijdstip
bae-body-host-busy = De organisator (of een vereist teamlid) is op dit tijdstip niet meer beschikbaar. Vraag de gast een ander tijdslot te kiezen.
bae-body-host-not-synced = De Microsoft-agenda van een organisator is nog niet voor het eerst gesynchroniseerd, dus calrs weet niet of dit tijdstip vrij is. Probeer het over een minuut opnieuw of bekijk de synchronisatiefout van de bron onder Agendabronnen.
bookings-error-generic = Er is iets misgegaan.
bookings-error-no-bookable-dates = Dit afspraaktype heeft geen boekbare dagen meer, dus de gast kan geen nieuwe tijd kiezen. Verleng eerst de boekingsperiode.
bookings-error-reschedule-pending = De gast is al gevraagd een nieuwe tijd te kiezen en heeft nog niet gereageerd.
bookings-error-reschedule-resend-too-soon = Het verzoek is minder dan een uur geleden naar de gast gestuurd. Probeer het later opnieuw.
bookings-resend-reschedule = Verzoek opnieuw sturen
bookings-resend-reschedule-hint = Stuur de gast dezelfde link nog eens (hoogstens één keer per uur)
troubleshoot-detail-pending-request = { $title } (openstaande aanvraag)
troubleshoot-detail-claimed-booking = { $title } (overgenomen boeking)
troubleshoot-detail-unsupported-recurrence = { $calendar } (herhaalregel die calrs niet kan lezen, vanaf het begin als bezet gerekend)
etf-pending-blocks-slot = Openstaande aanvragen blokkeren het tijdslot
etf-pending-blocks-slot-help = Zolang een aanvraag op je antwoord wacht, kan niemand anders dat tijdstip boeken, bij dit en bij je andere afspraaktypes.
etf-pending-expiry = Verloop van aanvragen
etf-pending-expiry-help = Een aanvraag die je dan nog niet hebt beantwoord, wordt automatisch afgewezen en de gast wordt gevraagd een ander tijdstip te kiezen. De termijn eindigt eerder als de afspraak te dichtbij komt om het tijdslot nog door iemand anders te laten boeken. Halverwege krijg je één herinnering.
etf-pending-expiry-never = Nooit

etf-pending-expiry-hours =
    { $hours ->
        [one] Na { $hours } uur
       *[other] Na { $hours } uur
    }

# Host, watcher and remaining guest emails (src/email.rs, #209)

email-footer-sent-by = Verzonden door
email-sender-via = { $host } via { $instance }
email-date-at-time = { $date } om { $time }
email-detail-phone = Telefoon:
email-detail-expires = Verloopt:
email-detail-new-date = Nieuwe datum:
email-detail-new-time = Nieuwe tijd:
email-detail-previous = Eerder:
email-detail-requested = Gevraagd:
email-detail-originally = Oorspronkelijk:
email-detail-organizer = Organisator:
email-detail-booked-by = Geboekt door:
email-detail-invited-by = Uitgenodigd door:
email-detail-message = Bericht:
email-action-approve = Goedkeuren
email-action-decline = Afwijzen
email-action-pick-new-time = Kies een nieuw tijdstip
email-host-approve-line = Goedkeuren: { $url }
email-host-decline-line = Afwijzen: { $url }
email-host-approval-dashboard = Log in op je dashboard om deze boeking te bevestigen of af te wijzen.
email-host-new-subject = Nieuwe boeking: { $event }, { $guest } ({ $date })
email-host-new-heading = Nieuwe boeking!
email-host-new-message = { $guest } heeft een afspraak met je geboekt.
email-host-confirmed-subject = Bevestigd: { $event }, { $guest } ({ $date })
email-host-confirmed-heading = Boeking bevestigd
email-host-confirmed-message = Je hebt de boeking met { $guest } goedgekeurd.
email-host-confirmed-footer = Het evenement is aan je agenda toegevoegd.
email-host-sync-failure-subject = Actie nodig: agenda niet bijgewerkt voor { $event } ({ $date })
email-host-sync-failure-heading = Je agenda is niet bijgewerkt
email-host-sync-failure-intro = Deze boeking is verzet, maar calrs kon het evenement in je Google Agenda niet bijwerken. De gast heeft de nieuwe tijd; je agenda toont nog de oude.
email-host-sync-failure-action = Open het evenement in Google Agenda en verplaats het naar de tijd hierboven. Verplaats het bestaande evenement in plaats van het opnieuw aan te maken, anders werkt de Google Meet-link die de gast al heeft niet meer.
email-writeback-failure-subject = Actie vereist: { $event } ({ $date }) staat niet in je agenda
email-writeback-failure-heading = Er ontbreekt een boeking in je agenda
email-writeback-failure-intro = calrs kon deze boeking ook na meerdere pogingen niet aan je agenda toevoegen. De boeking is bevestigd en de gast heeft de uitnodiging.
email-writeback-failure-action = Zet het evenement zelf in je agenda zodat je het niet mist, en controleer de agendabron in calrs. calrs blijft het nog een paar uur proberen; je krijgt deze e-mail voor deze boeking niet nog een keer.
email-detail-error = Fout:
email-source-failure-subject = Actie vereist: agendabron "{ $source }" synchroniseert niet
email-source-failure-heading = Een agendabron synchroniseert niet meer
email-source-failure-intro = calrs kan deze agendabron al meer dan een uur niet synchroniseren. Je boekingspagina's bieden nog steeds tijden aan, maar die zijn gebaseerd op de laatste geslaagde synchronisatie, dus evenementen die sindsdien zijn toegevoegd of verplaatst worden niet meegenomen.
email-source-failure-action = Controleer de bron op de pagina Agendabronnen: meestal is een verlopen wachtwoord of ingetrokken toegang de oorzaak. Je hoort pas weer over deze bron als die herstelt en daarna opnieuw faalt.
email-detail-source = Agendabron:
email-detail-failing-since = Faalt sinds:
email-host-reminder-subject = Herinnering: { $event }, { $guest } ({ $date })
email-host-reminder-heading = Komende boeking
email-host-reminder-message = Herinnering: je hebt binnenkort een boeking met { $guest }.
email-host-cancel-subject = Geannuleerd: { $event }, { $guest } ({ $date })
email-host-cancel-message-by-host = Je hebt deze boeking geannuleerd.
email-host-cancel-message-by-guest = { $guest } heeft de boeking geannuleerd.
email-host-approval-subject = Actie vereist: { $event }, { $guest } ({ $date })
email-host-approval-heading = Actie vereist
email-host-approval-message = { $guest } wil een afspraak met je boeken.
email-host-approval-reminder-subject = Herinnering: { $guest } wacht op je antwoord ({ $event }, { $date })
email-host-approval-reminder-heading = Herinnering: actie vereist
email-host-approval-reminder-message = { $guest } wacht nog op je antwoord. Als je niet reageert, wordt de aanvraag op { $deadline } automatisch afgewezen.
email-host-approval-footer = Je kunt dit ook vanuit je dashboard beheren.
email-host-expired-subject = Verlopen: { $event } met { $guest } ({ $date })
email-host-expired-heading = Boekingsaanvraag verlopen
email-host-expired-message = Niemand heeft de aanvraag van { $guest } voor de deadline beantwoord. Daarom is ze automatisch afgewezen en is de gast gevraagd een ander tijdstip te kiezen.
email-host-expired-footer = Hoe lang aanvragen wachten, wijzig je in de instellingen van het afspraaktype.
email-host-reschedule-request-subject = Verzetverzoek: { $event }, { $guest } ({ $email })
email-host-reschedule-request-message = { $guest } wil de boeking verzetten.
email-watcher-claim-subject = Over te nemen: { $event }, { $guest } ({ $date })
email-watcher-claim-message = Er is een nieuwe boeking die je kunt overnemen. Klik hieronder om als deelnemer aan te sluiten.
email-watcher-claim-link = Deze boeking overnemen: { $url }
email-watcher-claim-footer = Je kunt hem ook vanuit je dashboard overnemen.
email-claim-confirmed-subject = Boeking overgenomen: { $event }, { $guest } ({ $date })
email-claim-confirmed-footer = Je wordt als deelnemer aan deze afspraak toegevoegd.
email-attendee-greeting = Hoi,
email-attendee-message = Je bent als deelnemer aan een boeking toegevoegd.
email-attendee-rescheduled-message = Een boeking waaraan je deelneemt is verzet naar een nieuw tijdstip.
email-attendee-cancelled-message = Een boeking waaraan je deelneemt is geannuleerd.
email-attendee-cancelled-subject = Geannuleerd: { $event }
email-original-booking-cancelled = Je oorspronkelijke boeking is ook geannuleerd.
email-pick-new-time-subject = Verzetten: { $event }, kies een nieuw tijdstip
email-pick-new-time-message = { $host } moet je boeking verzetten. Kies een nieuw tijdstip.
email-pick-new-time-link = Kies een nieuw tijdstip: { $url }
email-rescheduled-message = Je boeking is verzet door { $host }.
email-reschedule-approved-message = { $host } heeft het nieuwe tijdstip van je boeking bevestigd.
email-rescheduled-ics-attached-plain = Er is een bijgewerkte agenda-uitnodiging bijgevoegd.
email-rescheduled-ics-attached-html = Er is een bijgewerkte agenda-uitnodiging bij deze e-mail gevoegd.
email-need-to-reschedule = Wil je verzetten? { $url }
email-invite-subject = { $host } heeft je uitgenodigd om te boeken: { $event }
email-invite-message = { $host } heeft je uitgenodigd om te boeken: { $event }
email-invite-link = Klik op de link hieronder om een tijdstip te kiezen:
email-invite-action = Kies een tijdstip
email-cancel-reason-calendar-deleted = De organisator heeft het agenda-evenement verwijderd.

# Embed snippet generator (templates/embed.html)
embed-page-title = Insluiten — { $title }
embed-heading = Insluiten: { $title }
embed-subtitle = Genereer een codefragment om dit afspraaktype op je eigen website in te sluiten.
embed-warn-internal = Let op: dit afspraaktype is <strong>intern</strong>. Bezoekers van de ingesloten pagina hebben een geldige uitnodigingslink nodig om te boeken — het gegenereerde fragment bevat die niet. Overweeg de zichtbaarheid op openbaar te zetten om direct in te sluiten.
embed-warn-private = Let op: dit afspraaktype is <strong>privé</strong>. Bezoekers van de ingesloten pagina hebben een geldige uitnodigingslink nodig om te boeken — het gegenereerde fragment bevat die niet. Overweeg de zichtbaarheid op openbaar te zetten om direct in te sluiten.
embed-warn-private-team = Let op: dit team is <strong>privé</strong>. Bezoekers van de ingesloten pagina kunnen niet boeken zonder de uitnodigingslink van het team — het gegenereerde fragment bevat die niet. Overweeg het team openbaar te maken om direct in te sluiten.
embed-need-username = Je moet een <strong>gebruikersnaam</strong> instellen voordat je dit afspraaktype kunt insluiten — de insluitlink is { $path }. { $link } en kom daarna terug.
embed-need-username-link = Stel een gebruikersnaam in bij de instellingen
embed-username-placeholder = gebruikersnaam
embed-tab-inline = Inline
embed-tab-floating = Zwevende knop
embed-tab-element = Klik op element
embed-layout = Indeling
embed-theme = Thema
embed-theme-auto = Automatisch (volgt bezoeker)
embed-brand-color = Merkkleur
embed-use-org-color = Kleur van de organisatie gebruiken
embed-fix-time-format = Vaste tijdnotatie (verbergt de 12u/24u-schakelaar)
embed-fixed-tz = Tijden tonen in de tijdzone van de afspraak (verbergt de tijdzonekeuze)
embed-fixed-tz-hint = Het best voor afspraken ter plaatse. Bezoekers zien de lokale tijd van de afspraak, met vermelding van de tijdzone.
embed-button-text = Knoptekst
embed-button-default = Afspraak maken
embed-button-position = Knoppositie
embed-pos-bottom-right = Rechtsonder
embed-pos-bottom-left = Linksonder
embed-pos-top-right = Rechtsboven
embed-pos-top-left = Linksboven
embed-button-colors = Knopkleuren
embed-button-fg-placeholder = tekst
embed-button-colors-hint = Achtergrond / tekst
embed-show-icon = Agenda-icoon tonen
embed-element-help = Plaats het fragment één keer in je site en voeg daarna de getoonde data-attributen toe aan elk klikbaar element (knop, link, div) dat het boekingsvenster moet openen.
embed-snippet = Codefragment
embed-loading = Laden…
embed-preview = Live voorbeeld
embed-preview-back = terug naar voorbeeld
embed-preview-floating-hint = Klik op de knop om het boekingsvenster te simuleren
embed-preview-element-hint = Demo-triggers — voeg op je site de data-attributen uit het fragment toe aan een willekeurig element. Klik op een van beide om het venster te proberen:
embed-preview-text-link = of klik op deze tekstlink
embed-snippet-attr-comment = Voeg deze attributen toe aan een klikbaar element op je pagina:

# Leftovers in partly translated pages (host reschedule, impersonation banner, slots, team settings)
host-resched-when = { $date } om { $start } – { $end }
base-impersonating = Je handelt als { $name }
slots-rescheduling-banner = Verzetten: { $title } (nu { $date } om { $time })
slots-invite-required-title = Voor boeken is een uitnodigingslink nodig
team-settings-slug-url = Gebruikt in de team-URL: { $url }
team-event-type-no-teams = Er zijn nog geen teams.
team-event-type-not-team-admin = Je bent van geen enkel team beheerder.

# Google Calendar connection errors (src/web/sources.rs)
google-error-not-configured = De Google Agenda-koppeling is niet ingesteld. Vraag je beheerder om de Google OAuth2-gegevens in te stellen in het beheerpaneel.
google-error-no-base-url = De openbare basis-URL is niet ingesteld. Stel die in via de omgevingsvariabele CALRS_BASE_URL of in het beheerpaneel onder Systeeminstellingen voordat je OAuth2-stromen gebruikt.
google-error-authorization = Google-autorisatie mislukt: { $error }
google-error-token-exchange = Tokenuitwisseling mislukt: { $error }
google-error-account-email = Kan het e-mailadres van het Google-account niet ophalen: { $error }

# Admin panel notices (src/web/admin.rs)
admin-resource-feed-invalid = Ongeldige feed-URL: { $error }
admin-resource-feed-check-failed = Feedcontrole mislukt: { $error }
admin-resource-caldav-invalid = Ongeldige CalDAV-URL: { $error }
admin-resource-credentials-failed = Kan de inloggegevens niet opslaan: { $error }
admin-resource-added = Resource "{ $name }" toegevoegd. Afspraken in cache: { $count }.
admin-resource-updated = Resource bijgewerkt. Afspraken in cache: { $count }.
admin-resource-deleted = Resource verwijderd.
admin-resource-not-found = Resource niet gevonden.
admin-resource-synced = Feed gesynchroniseerd. Afspraken in cache: { $count }.
admin-resource-sync-error = Synchronisatie mislukt: { $error }
admin-resource-no-caldav-url = Er is geen CalDAV-URL ingesteld en er kon er ook geen worden afgeleid uit de feed-URL.
admin-resource-no-write-credentials = Geen schrijfgegevens beschikbaar (geen serviceaccount, en geen lid dat toestemming gaf heeft een passende agendabron).
admin-resource-write-passed = Schrijftest geslaagd met { $user } (aangemaakt: { $created }, opgeruimd: { $cleaned }).
admin-resource-write-verified = geverifieerd
admin-resource-write-unverified = niet geverifieerd
admin-resource-write-check-calendar = controleer de resource-agenda
admin-resource-write-failed = Schrijftest mislukt. Laatste fout: { $error }
admin-base-url-invalid = De basis-URL moet een absolute http(s)-URL zijn, bijv. https://cal.example.com
admin-company-link-invalid = De bedrijfslink moet beginnen met http:// of https://
admin-smtp-host-required = De SMTP-host is verplicht.
admin-smtp-from-invalid = Voer een geldig afzenderadres in.
admin-smtp-port-invalid = De SMTP-poort moet een getal tussen 1 en 65535 zijn.
admin-smtp-tls-invalid = De TLS-modus moet "starttls", "tls" of "none" zijn.
admin-smtp-password-required = Bij de eerste SMTP-configuratie is een wachtwoord verplicht.
admin-sms-switch-needs-secret = Om van sms-gateway te wisselen moet je de inloggegevens van die gateway invoeren.
admin-sms-settings-invalid = Sms-instellingen: { $error }.
admin-sms-sender-format = Een numerieke sms-afzender moet in internationaal formaat zijn, bijv. +31612345678.
admin-sms-not-configured = Sms is niet ingesteld.
admin-sms-load-failed = Kan de sms-configuratie niet laden.
admin-sms-check-accepted = Inloggegevens geaccepteerd door de gateway.
admin-sms-bad-number = Dat is geen bruikbaar telefoonnummer.
admin-sms-test-body = Testbericht van calrs. Je sms-gateway werkt.
admin-sms-test-accepted = Testbericht geaccepteerd door de gateway.
admin-sms-test-over-cap = De dagelijkse SMS-limiet is bereikt, dus het testbericht is niet verzonden. Testberichten tellen mee voor de daglimiet zoals alle andere.
admin-sms-test-ledger-failed = Het SMS-gebruik kon niet worden vastgelegd, dus het testbericht is niet verzonden.

## Local multi-factor authentication

mfa-title = Tweestapsverificatie
mfa-invalid = Verificatie mislukt. Controleer je wachtwoord of code. Elke code werkt maar één keer; wacht zo nodig op een nieuwe code. Wacht na te veel pogingen 15 minuten. Is het instellen verlopen, begin dan opnieuw.
mfa-restart = Terug naar inloggen
mfa-manage = Tweestapsverificatie beheren
mfa-recovery-help = Bewaar deze herstelcodes op een veilige plek. Ze worden maar één keer getoond. Elke code vervangt één keer een code uit je authenticator-app. Nieuwe herstelcodes vervangen alle eerdere codes.
mfa-continue = Verder naar het dashboard
mfa-continue-consent = Doorgaan naar de goedkeuringspagina
mfa-enabled = Tweestapsverificatie is ingeschakeld.
mfa-disabled = Tweestapsverificatie is uitgeschakeld.
mfa-required-help = Je beheerder vereist tweestapsverificatie voor lokale accounts. Je kunt het niet uitschakelen.
mfa-code = Authenticator-code of herstelcode
mfa-regenerate = Nieuwe herstelcodes maken
mfa-disable = Tweestapsverificatie uitschakelen
mfa-enable = Tweestapsverificatie instellen
mfa-changes-help = Voor wijzigingen is je wachtwoord nodig. Voor uitschakelen of het vervangen van herstelcodes is ook een code nodig, en je andere sessies worden dan afgemeld.
mfa-back = Terug naar instellingen
mfa-setup-help = Scan deze QR-code met je authenticator-app, of voer de sleutel hieronder handmatig in. Voer een code van zes cijfers in om het instellen af te ronden. Het instellen verloopt na 10 minuten.
mfa-qr-alt = QR-code voor het instellen van de authenticator-app
mfa-login-help = Voer een code uit je authenticator-app in, of gebruik een van je bewaarde herstelcodes. Deze aanmelding verloopt na 10 minuten.
mfa-totp-code = Authenticator-code van zes cijfers
mfa-verify = Verifiëren
mfa-policy-help = Dit beleid geldt voor lokale accounts. SSO gebruikt de MFA van de identiteitsprovider. Lokale beheerders moeten MFA instellen voordat ze dit beleid wijzigen. MFA verplicht maken meldt lokale sessies zonder MFA-controle af en vereist het instellen bij de volgende aanmelding.
mfa-policy = MFA-beleid voor lokale accounts
mfa-optional = Optioneel
mfa-required = Verplicht
mfa-save-policy = MFA-beleid opslaan

# First day of the week (#330)
week-start-label = Eerste dag van de week
week-start-inherit-site = Standaard van de site gebruiken
week-start-inherit-owner = Standaard van de eigenaar gebruiken

## Booking directory and personal favorites

directory-title = Afsprakengids
directory-help = Vind de afspraaklink van een collega of team. Voeg afspraken die je vaak deelt toe aan je favorieten om ze op je dashboard te zien.
directory-search = Zoek op persoon, team of afspraaktitel
directory-find = Zoeken
directory-favorites-only = Alleen favorieten
directory-favorites = Favoriete afspraken
directory-browse = Gids bekijken
directory-generate = Uitnodigingslink maken
directory-invite-help = Eenmalige uitnodiging, 7 dagen geldig.
directory-add = { $title } toevoegen aan favorieten
directory-remove = { $title } verwijderen uit favorieten
directory-added = Toegevoegd aan favorieten.
directory-removed = Verwijderd uit favorieten.
directory-error = Deze actie kon niet worden voltooid. Vernieuw de pagina en probeer het opnieuw.
directory-manual-copy = Kopieer deze link handmatig:
directory-empty = Geen afspraken gevonden. Probeer een andere zoekopdracht.
directory-favorites-empty = Geen favorieten gevonden. Voeg afspraken uit de gids toe aan je favorieten om ze hier te zien.
directory-with-hosts = Met { $names }

microsoft-calendar-title = Microsoft 365 / Outlook.com-agenda
microsoft-calendar-help = Verbind je Microsoft-agenda met gedelegeerde toestemming. Je Microsoft-account mag verschillen van je calrs-aanmeldaccount. Gebruik hieronder EWS voor lokale Exchange.
microsoft-calendar-connect = Microsoft-agenda verbinden
microsoft-calendar-unconfigured = Een beheerder moet eerst de Microsoft-toepassing configureren.
microsoft-admin-help = Registreer een webtoepassing met precies deze omleidings-URI. Vraag gedelegeerde machtigingen Calendars.ReadWrite, User.Read en offline_access, geen toepassingsmachtigingen. Elke gebruiker verbindt zijn eigen account. Impliciete verlening en openbare-clientstromen zijn niet nodig.
microsoft-tenant-label = Microsoft-tenant
microsoft-tenant-help = common staat bewust werk-, school- en persoonlijke accounts toe als de app-registratie beide categorieën ondersteunt. Gebruik organizations voor werk-/schoolaccounts, consumers voor persoonlijke accounts, of een tenant-UUID / geverifieerd domein om toegang te beperken.
microsoft-admin-save = Microsoft-instellingen opslaan
microsoft-error-impersonation = Je kunt de Microsoft-agenda niet verbinden terwijl je als een andere gebruiker werkt.
microsoft-error-start = De Microsoft-autorisatie kon niet worden gestart. Controleer de app-instellingen, de openbare HTTPS-basis-URL en of deze bron bij je account hoort.
microsoft-error-state = De Microsoft-autorisatiestatus is ongeldig, verlopen of al gebruikt, of je sessie is veranderd. Start de verbinding opnieuw.
microsoft-error-consent = De Microsoft-autorisatie is niet voltooid. Begin opnieuw en geef toestemming voor het lezen en schrijven van de agenda, profieltoegang en offlinetoegang.
microsoft-error-connect = De Microsoft-agenda kon niet worden verbonden. Controleer de instellingen en machtigingen. Kies bij opnieuw verbinden het oorspronkelijke Microsoft-account; voeg voor een ander account een nieuwe bron toe.
microsoft-error-sync = Microsoft is verbonden, maar de synchronisatie is mislukt. Controleer of het account een Exchange Online- of Outlook.com-agenda heeft. Ga terug naar de agendabronnen om opnieuw te synchroniseren of te verbinden.
microsoft-error-selection = Microsoft is verbonden, maar de agendaselectie kon niet worden geladen. Ga terug naar de agendabronnen.
microsoft-error-settings = De Microsoft-instellingen zijn niet opgeslagen. Controleer de client-UUID, tenant en databasetoegang. Een leeg geheim behoudt het bestaande geheim.
microsoft-error-heading = Microsoft-agenda niet gekoppeld
microsoft-error-settings-heading = Microsoft-instellingen niet opgeslagen
microsoft-error-back-admin = Terug naar het beheerpaneel
source-test-microsoft-ok = Verbinding met de Microsoft-agenda is in orde.
source-test-microsoft-failed = Verbinding met de Microsoft-agenda mislukt. Controleer de toegang tot de agenda of maak opnieuw verbinding.
source-test-microsoft-credentials = De inloggegevens voor de Microsoft-agenda konden niet worden geladen. Verbind je account opnieuw.
microsoft-callback-needs-https = Stel de openbare basis-URL in op een HTTPS-origin zonder pad om de te registreren callback-URL te zien.
source-form-backend-ews = Microsoft Exchange on-premises (EWS)

# Global Exchange (EWS) impersonation: admin card and managed sources
sources-admin-managed = Beheerd door admin
sources-admin-managed-title = Door de beheerder ingericht
sources-locked-notice = Je beheerder beheert Exchange centraal. Extra agendabronnen toevoegen is uitgeschakeld.
sources-locked-by-admin = De beheerder heeft het toevoegen van persoonlijke agendabronnen uitgeschakeld. Je Exchange-agenda wordt centraal beheerd.
sources-managed-no-edit = Deze agendabron is door de beheerder ingericht en kan niet worden bewerkt.
sources-managed-no-remove = Deze agendabron is door de beheerder ingericht en kan niet worden verwijderd.
sources-managed-global-disabled = Deze bron wordt door de beheerder beheerd, maar globale Exchange is uitgeschakeld.
admin-ews-title = Exchange (EWS): globale impersonatie
admin-ews-help = Stel één Exchange Web Services-eindpunt in en een serviceaccount met de RBAC-rol <code>ApplicationImpersonation</code>. De agenda van elke gebruiker wordt dan via impersonatie gelezen, zonder dat er inloggegevens nodig zijn. PowerShell op Exchange: <code>New-ManagementRoleAssignment -Name CalrsImpersonation -Role ApplicationImpersonation -User svc-calrs@jouwdomein</code>.
admin-ews-endpoint = Eindpunt
admin-ews-managed-count = Beheerde bronnen:
admin-ews-enable = Globale Exchange via impersonatie inschakelen
admin-ews-url = URL van het EWS-eindpunt
admin-ews-username = Gebruikersnaam van het serviceaccount
admin-ews-password = Wachtwoord van het serviceaccount
admin-ews-password-keep = (leeg laten om het huidige te behouden)
admin-ews-password-unchanged = (ongewijzigd)
admin-ews-password-placeholder = wachtwoord van het serviceaccount
admin-ews-domain = Mailboxdomein
admin-ews-optional = (optioneel)
admin-ews-domain-help = Vul beide in wanneer de adressen die calrs kent (AD-UPN, OIDC-e-mail) op een ander domein staan dan de Exchange-mailboxen. Alleen een adres in precies het domein van de gebruikers wordt herschreven, met behoud van het lokale deel: <code>alice@example.local</code> → <code>alice@example.com</code>. Adressen in elk ander domein worden ongewijzigd gebruikt. Laat beide leeg om het calrs-adres van elke gebruiker ongewijzigd te gebruiken.
admin-ews-auto = Automatisch inrichten voor alle gebruikers (bestaande + toekomstige)
admin-ews-auto-help = Indien aangevinkt krijgt elke actieve gebruiker met een vertrouwd adres een beheerde Exchange-bron: accounts aangemaakt met calrs user create, SSO-accounts waarvan de identiteitsprovider het e-mailadres heeft geverifieerd, en hieronder bevestigde adressen. Opslaan voert de bewerking direct uit.
admin-ews-lock = Voorkom dat gewone gebruikers eigen agendabronnen toevoegen
admin-ews-lock-help = Beheerders zijn uitgezonderd. Door gebruikers al toegevoegde bronnen blijven bestaan tot je ze verwijdert.
admin-ews-disable-note = Zolang de functie uit staat, blijven beheerde bronnen bestaan maar worden ze niet gesynchroniseerd, en wordt er geen boeking naar geschreven. Weer inschakelen hervat ze.
admin-ews-remove = Beheerde bronnen verwijderen en het wachtwoord van het serviceaccount wissen
admin-ews-remove-confirm = Alle beheerde Exchange-bronnen verwijderen, met de agenda's en bezette tijden die calrs ervan bewaarde, en het opgeslagen wachtwoord van het serviceaccount? Globale Exchange wordt uitgeschakeld. Boekingen die al in de postvakken zijn geschreven, blijven daar staan.
admin-ews-notice-removed = Verwijderde beheerde bronnen: { $count }. Het wachtwoord van het serviceaccount is gewist en globale Exchange staat uit. Boekingen die al in de postvakken zijn geschreven, blijven daar staan.
admin-ews-notice-https-required = Het EWS-eindpunt moet https:// gebruiken: het wachtwoord van het serviceaccount wordt met elk verzoek meegestuurd. Er is niets opgeslagen.
admin-ews-notice-password-cleared = Instellingen opgeslagen. Het eindpunt of het serviceaccount is gewijzigd, dus het opgeslagen wachtwoord is gewist: het moet opnieuw worden ingevoerd voordat globale Exchange werkt.
admin-ews-notice-source-removed = Beheerde bron verwijderd, met de bewaarde agenda's en bezette tijden.
admin-ews-row-remove = Verwijderen
admin-ews-row-remove-confirm = De beheerde Exchange-bron van { $email } verwijderen, met de bewaarde agenda's en bezette tijden? Automatisch inrichten maakt hem opnieuw aan bij de volgende opslag, bevestiging of SSO-aanmelding; het account opschorten voorkomt dat.
admin-ews-trust-no-mailbox = geen (het adres wijst geen postvak aan)
admin-ews-notice-selections-reset = Het postvak dat sommige beheerde bronnen lezen is gewijzigd, dus hun agenda's worden opnieuw gevonden. Afspraaktypen waarvan de agendaselectie is gereset: { $count } (vermeld in het serverlogboek).
admin-ews-notice-reconfirm = De domeinvervanging koppelt sommige bevestigde adressen nu aan een ander postvak. Opnieuw te bevestigen adressen: { $count } (zie de lijst hieronder).
admin-ews-row-taken-over = Overgenomen van { $user } op { $when }
admin-ews-notice-trust-changed = Niets bevestigd: het adres van dit account, of het postvak dat het opent, is gewijzigd sinds de pagina werd getoond. Bekijk de lijst opnieuw.
sources-managed-user-untrusted = Deze bron wordt door de beheerder beheerd, maar het adres van het account is nog niet bevestigd, dus hij wordt niet gesynchroniseerd.
sources-managed-user-suspended = Deze bron wordt door de beheerder beheerd, maar het account is uitgeschakeld, dus hij wordt niet gesynchroniseerd.
admin-ews-save = EWS-instellingen opslaan
admin-ews-save-provision = Opslaan &amp; nu voor alle gebruikers inrichten
admin-ews-status-title = Synchronisatiestatus per gebruiker
admin-ews-status-summary = { $ok } van { $total } beheerde mailboxen worden gesynchroniseerd. Een gebruiker zonder Exchange-mailbox (bijv. een lokaal beheerdersaccount) toont altijd een fout. Dat is normaal.
admin-ews-col-user = Gebruiker
admin-ews-col-mailbox = Mailbox
admin-ews-col-last-sync = Laatste synchronisatie
admin-ews-col-status = Status
admin-ews-row-disabled = uitgeschakeld
admin-ews-row-synced = Gesynchroniseerd
admin-ews-row-never = Nooit gesynchroniseerd
admin-ews-row-no-mailbox = Geen Exchange-mailbox
admin-ews-notice-saved = Exchange-instellingen opgeslagen.
admin-ews-notice-provisioned = Exchange-instellingen opgeslagen. Aangemaakte beheerde bronnen: { $count }.
admin-ews-notice-invalid-url = De URL van het EWS-eindpunt is ongeldig (alleen http(s), geen privéadres tenzij toegestaan). Er is niets opgeslagen.
admin-ews-notice-incomplete = Vul de eindpunt-URL, het serviceaccount en het wachtwoord in om globale Exchange in te schakelen. Er is niets opgeslagen.
admin-ews-notice-provision-failed = Instellingen opgeslagen, maar het inrichten van de beheerde bronnen is mislukt. Zie het serverlog.
admin-ews-registration-warning = Vrije registratie staat open. Zo aangemaakte accounts krijgen nooit een beheerde Exchange-bron, ook niet via "nu voor alle gebruikers inrichten": niets bewijst dat het adres van hen is. Om een van hen een mailbox te geven, bevestig je het adres hieronder, of voer je calrs user trust-email uit, zodra je weet dat het van die persoon is.
admin-ews-from-domain = Domein van de gebruikers
admin-ews-notice-override-domains = Het overschrijven van het domein heeft beide domeinen nodig, elk een gewoon domein zoals example.com: het domein waarmee je gebruikers inloggen en dat van hun mailboxen. Er is niets opgeslagen.
admin-ews-notice-trusted = Adres bevestigd. Zolang globale Exchange aan staat, wordt de beheerde bron van de gebruiker nu aangemaakt: zie de statustabel.
admin-ews-row-suspended = Account uitgeschakeld: niet gesynchroniseerd
admin-ews-row-untrusted = Adres niet bevestigd: niet gesynchroniseerd
admin-ews-row-conflict = Niet ingericht: de bron van een andere gebruiker leest deze mailbox al
admin-ews-trust-title = Te bevestigen adressen
admin-ews-trust-help = Deze actieve accounts in de toegestane domeinen hebben een adres waar niemand voor instaat (vrije registratie, of aangemaakt vóór deze controle), dus krijgen ze geen beheerde Exchange-bron. Bevestig een adres alleen als je weet dat het van die persoon is: na bevestiging leest calrs voor hen de agenda van die mailbox.
admin-ews-trust-confirm = Bevestigen
admin-ews-search = Zoeken op gebruiker of postvak
admin-ews-no-match = Geen beheerd postvak of te bevestigen adres komt overeen met deze zoekopdracht.
admin-ews-conflicts-more = Postvak al in gebruik door een andere gebruiker: { $shown } van { $total } getoond.
admin-ews-http-warning = Het endpoint gebruikt http://: het wachtwoord van het serviceaccount wordt onversleuteld verzonden. Gebruik bij voorkeur https://.
# Time off (templates/time_off.html, src/web/time_off.rs, troubleshoot view)

nav-time-off = Afwezigheid
time-off-page-title = Afwezigheid
time-off-heading = Afwezigheid
time-off-subtitle = Periodes waarin je geen boekingen aanneemt. Alle afspraaktypes die je aanbiedt volgen ze, persoonlijk en van je teams. Gasten zien alleen dat die tijden niet beschikbaar zijn.
time-off-upcoming = Gepland
time-off-empty = Geen afwezigheid gepland.
time-off-kept-notice = Boekingen die al in een periode vallen blijven staan: afwezigheid toevoegen annuleert niets. Annuleer of verzet ze zo nodig via Boekingen.
time-off-pending-notice = Aanvragen die binnen een periode op je goedkeuring wachten, kun je niet meer goedkeuren: wijs ze af of verwijder eerst de afwezigheid.
time-off-in-progress = Bezig
time-off-entered-in = Ingevoerd in { $timezone }
time-off-delete = Verwijderen
time-off-delete-confirm = Deze afwezigheid verwijderen? De tijden worden weer boekbaar.
time-off-add-heading = Afwezigheid toevoegen
time-off-from = Van
time-off-to = Tot en met
time-off-time-optional = Tijd (optioneel)
time-off-times-help = Laat de tijden leeg om hele dagen te blokkeren, beide datums inbegrepen. De tijden zijn in je tijdzone, { $timezone }.
time-off-label = Notitie (optioneel)
time-off-label-help = Alleen jij ziet die, hier en bij Probleemoplossing. Gasten en teamgenoten zien die nooit.
time-off-team-admins-notice = De beheerders van je teams en die van de instantie zien de datums en tijden van je afwezigheid, nooit de notitie.
time-off-add-submit = Afwezigheid toevoegen
time-off-added = Afwezigheid toegevoegd.
time-off-deleted = Afwezigheid verwijderd.
time-off-range = { $start } tot { $end }
time-off-at = { $date } om { $time }
time-off-same-day = { $date }, { $start } tot { $end }
time-off-from-time = { $date }, vanaf { $time }
time-off-until-time = { $date }, tot { $time }
time-off-impersonating = Je bekijkt deze pagina als { $name }. Afwezigheid is hier alleen-lezen voor jou en de notities zijn verborgen.
time-off-error-date = Voer de datums in als JJJJ-MM-DD en de tijden als UU:MM.
time-off-error-order = Het einde moet na het begin liggen.
time-off-error-too-long = Een periode mag hoogstens een jaar duren.
time-off-error-past = Deze periode is al voorbij.
time-off-error-label = De notitie mag hoogstens 200 tekens lang zijn.
time-off-error-too-many = Je hebt al 100 periodes gepland. Verwijder er een paar voordat je nieuwe toevoegt.
time-off-hint = Gebruik { $link } om datums voor alle afspraaktypes tegelijk te blokkeren.
troubleshoot-label-time-off = Afwezigheid
troubleshoot-legend-time-off = Afwezigheid

directory-all-favorites = Alle favorieten
pager-label = Pagina’s
pager-range = { $from } tot { $to } van { $total }
pager-previous = Vorige
pager-next = Volgende
admin-users-search = Zoeken
admin-figures-cached = Boekingscijfers { $seconds } s geleden berekend.

# OAuth connection consent (templates/auth/consent.html)
oauth-title = Een toepassing verbinden
oauth-intro = Deze toepassing toegang geven tot calrs als { $email }? De naam wordt opgegeven door de ontwikkelaar. Controleer het terugkeeradres voordat je toestemming geeft.
oauth-events = Afspraaktypes lezen waartoe je toegang hebt
oauth-blocks = Je bezette agendatijden lezen
oauth-invites = Uitnodigingslinks maken met jouw rechten
oauth-allow = Toegang toestaan
oauth-deny = Weigeren

# Connected apps (templates/dashboard_apps.html, admin user menu)

apps-page-title = Gekoppelde apps
apps-heading = Gekoppelde apps
apps-subtitle = Apps die je toestemming hebt gegeven om namens jou te handelen, zoals AI-assistenten die via MCP zijn verbonden. Intrekken stopt hun toegang meteen.
apps-approved = Goedgekeurd op { $date }
apps-last-used = Laatst gebruikt op { $date }
apps-never-used = Nog niet gebruikt
apps-revoke = Intrekken
apps-revoke-confirm = Toegang van { $name } intrekken? De app moet opnieuw worden goedgekeurd om weer verbinding te maken.
apps-revoked = Toegang ingetrokken.
apps-empty = Er is geen app gekoppeld. Een MCP-client vraagt je om toestemming wanneer hij voor het eerst verbinding maakt.
oauth-slots = De boekbare tijden lezen van afspraaktypes die je beheert
oauth-bookings-read = De boekingen lezen waarvan je gastheer bent, met de gegevens van de gasten
oauth-bookings-write = Boeken, annuleren en verzetten op afspraaktypes die je beheert, met e-mails aan de gasten
oauth-api-closed = Deze app vraagt de REST API te gebruiken, die op deze instantie alleen beheerders mogen gebruiken. Hij kan niet worden goedgekeurd.
api-tokens-page-title = API-tokens
api-tokens-heading = API-tokens
api-tokens-subtitle = Tokens waarmee je eigen scripts de REST API van calrs namens jou aanroepen. Een token werkt tot het verloopt of je het intrekt.
api-tokens-not-allowed = Op deze instantie kunnen alleen beheerders API-tokens maken.
api-tokens-new = Nieuw token
api-tokens-name = Naam
api-tokens-name-hint = Waarvoor het dient, bijvoorbeeld CRM-synchronisatie
api-tokens-scopes = Rechten
api-tokens-expiry = Verloopt
api-tokens-expiry-7 = Over 7 dagen
api-tokens-expiry-30 = Over 30 dagen
api-tokens-expiry-90 = Over 90 dagen
api-tokens-expiry-365 = Over een jaar
api-tokens-expiry-never = Nooit
api-tokens-create = Token maken
api-tokens-created = Kopieer dit token nu: het wordt maar één keer getoond.
api-tokens-created-on = Gemaakt op { $date }
api-tokens-expires-on = Verloopt op { $date }
api-tokens-never-expires = Verloopt nooit
api-tokens-revoke-confirm = { $name } intrekken? Scripts die het gebruiken werken meteen niet meer.
api-tokens-revoked = Token ingetrokken.
api-tokens-empty = Nog geen API-token.
api-tokens-error-name = Geef het token een naam van hoogstens 100 tekens.
api-tokens-error-scope = Kies minstens één recht.
api-tokens-error-limit = Je hebt al 20 tokens. Trek er eerst een in.
api-tokens-docs = Hoe je een token gebruikt, staat in het hoofdstuk REST API van de calrs-documentatie.
api-docs-title = API-referentie
api-docs-subtitle = Alle endpoints van de REST API van calrs, beschreven door deze instantie zelf.
api-docs-closed = Op deze instantie kunnen alleen beheerders de REST API gebruiken.
api-docs-try = Kies Authorize en plak een persoonlijk toegangstoken om een verzoek uit te proberen. Deze pagina stuurt je sessie nooit naar de API.
api-docs-tokens = API-tokens
api-docs-spec = OpenAPI-document (JSON)
api-docs-guide = Handleiding REST API
api-docs-noscript = De interactieve referentie heeft JavaScript nodig. Het OpenAPI-document beschrijft dezelfde API.
admin-api-tokens = API-tokens
admin-api-tokens-allow-all = Alle gebruikers de REST API laten gebruiken
admin-api-tokens-allow-all-help = Standaard uit: alleen beheerders kunnen persoonlijke toegangstokens maken of apps via OAuth aan de REST API koppelen, omdat beide gastgegevens lezen. Uitzetten schort de tokens en API-apps van andere gebruikers op tot het weer aan wordt gezet. MCP-clients merken er niets van.
admin-revoke-apps = Gekoppelde apps intrekken ({ $count })

# Write destination per event type (#47)
etf-write-destination = Bevestigde boekingen schrijven naar
etf-write-destination-default = Standaard (instelling van de bron)
etf-write-destination-help = Alleen voor dit afspraaktype. Standaard komen ze in de schrijfagenda van elk van je bronnen. Bij een teamafspraaktype geldt dit alleen voor de boekingen die je zelf begeleidt.
etf-write-destination-team = Elk lid kiest zelf onder { $link } waar de boekingen die het begeleidt worden geschreven.
sources-destinations-heading = Boekingen per afspraaktype
sources-destinations-help = Kies per afspraaktype de agenda waarin de bevestigde boekingen worden geschreven. Bij een teamafspraaktype geldt dit voor de boekingen die je zelf begeleidt; de andere leden kiezen hun eigen agenda. Boekingen die al geschreven zijn, verhuizen bij de volgende schrijfactie: verzetten, goedkeuren, opnieuw proberen of claimen.
sources-destinations-saved = Opgeslagen. Nieuwe boekingen gaan naar de hier gekozen agenda's.
write-destination-not-yours = Er is niets opgeslagen: kies een van je eigen agenda's, voor een afspraaktype dat je begeleidt.
write-destination-unavailable = { $name } (niet beschikbaar)
write-destination-refused = Deze agenda weigerde de laatste boeking ({ $error }), die volgens de instelling van de bron is geschreven. De volgende boeking probeert het opnieuw.
# Booking webhooks (templates/webhooks.html, admin card, #168)
webhooks-title = Webhooks
webhooks-back = Terug naar afspraaktypes
webhooks-subtitle = calrs stuurt een ondertekend JSON-bericht naar deze adressen wanneer een boeking wordt gemaakt, aangevraagd, goedgekeurd, afgewezen, verloopt, wordt geannuleerd of verzet.
webhooks-not-allowed = Op deze instantie zijn webhooks voorbehouden aan beheerders.
webhooks-endpoints = Bestemmingen
webhooks-empty = Nog geen webhooks. Voeg er hieronder een toe zodat calrs een ander systeem op de hoogte houdt van je boekingen.
webhooks-scope = Gebeurtenissen sturen voor
webhooks-scope-personal = Al mijn persoonlijke afspraaktypes
webhooks-scope-event-type = Afspraaktype: { $title }
webhooks-scope-team-event-type = Afspraaktype: { $title } ({ $team })
webhooks-added-by = Toegevoegd door { $name }
webhooks-state-off = Uitgeschakeld
webhooks-never-sent = Nog niets verstuurd
webhooks-state-ok = Laatste aflevering gelukt
webhooks-state-failing = Laatste aflevering mislukt
webhooks-queued =
    { $count ->
        [one] 1 aflevering wacht op een nieuwe poging
       *[other] { $count } afleveringen wachten op een nieuwe poging
    }
webhooks-gave-up =
    { $count ->
        [one] 1 aflevering opgegeven na alle pogingen
       *[other] { $count } afleveringen opgegeven na alle pogingen
    }
webhooks-send-test = Test versturen
webhooks-enable = Inschakelen
webhooks-disable = Uitschakelen
webhooks-delete = Verwijderen
webhooks-delete-confirm = Deze webhook verwijderen? Afleveringen die nog wachten vervallen.
webhooks-edit = Bewerken
webhooks-url = URL
webhooks-url-hint = (https)
webhooks-secret = Ondertekeningsgeheim
webhooks-secret-hint = (minstens { $min } tekens, ondertekent elk verzoek, wordt nooit meer getoond)
webhooks-save = Opslaan
webhooks-add = Een webhook toevoegen
webhooks-add-submit = Webhook toevoegen
webhooks-privacy-note = Elk bericht bevat de naam, het e-mailadres, het telefoonnummer en de notities van de gast. Stuur het alleen naar een systeem dat je die gegevens toevertrouwt.
webhooks-notice-created = Webhook toegevoegd. Gebruik "Test versturen" om te controleren of hij aankomt.
webhooks-notice-updated = Webhook opgeslagen.
webhooks-notice-deleted = Webhook verwijderd.
webhooks-notice-enabled = Webhook ingeschakeld.
webhooks-notice-disabled = Webhook uitgeschakeld. Afleveringen die nog wachtten zijn vervallen.
webhooks-notice-test-ok = Test afgeleverd: de bestemming antwoordde met een 2xx-status.
webhooks-error-url = Vul een volledige URL in, bijvoorbeeld https://hooks.example.com/calrs.
webhooks-error-https = De URL moet beginnen met https://. Gewoon http werkt alleen voor een host op de lijst met toegestane privéhosts.
webhooks-error-credentials = Haal de gebruikersnaam en het wachtwoord uit de URL: het ondertekeningsgeheim authenticeert calrs.
webhooks-error-secret = Het ondertekeningsgeheim moet 16 tot 256 tekens lang zijn.
webhooks-error-scope = Je kunt geen webhook aan dat afspraaktype toevoegen.
webhooks-error-limit = Dit bereik heeft al het maximum van 10 webhooks.
webhooks-error-test = De test is niet afgeleverd. De reden staat bij de webhook.
admin-webhooks = Boekingswebhooks
admin-webhooks-help =
    { $count ->
        [one] Op deze instantie is 1 webhook ingesteld.
       *[other] Op deze instantie zijn { $count } webhooks ingesteld.
    }
admin-webhooks-allow-all = Alle gebruikers webhooks laten toevoegen
admin-webhooks-allow-all-help = Standaard uit: een webhook stuurt namen, e-mailadressen en telefoonnummers van gasten naar een adres dat de gebruiker kiest, dus alleen beheerders mogen er een toevoegen. Uitschakelen stopt de webhooks die andere gebruikers hebben toegevoegd.
admin-webhooks-manage = Je webhooks beheren
webhooks-events = Gebeurtenissen
webhooks-events-all = Alle gebeurtenissen
webhooks-event-created = Geboekt (bevestigd)
webhooks-event-requested = Aangevraagd (wacht op goedkeuring)
webhooks-event-confirmed = Goedgekeurd
webhooks-event-declined = Afgewezen
webhooks-event-expired = Onbeantwoord verlopen
webhooks-event-cancelled = Geannuleerd
webhooks-event-rescheduled = Verzet
webhooks-error-events = Kies minstens één gebeurtenis.
webhooks-error-test-limit = Te veel tests in korte tijd. Wacht een paar minuten en probeer het opnieuw.
webhooks-added-by-former = Toegevoegd door een voormalig lid
webhooks-signature-help = Elk verzoek is ondertekend: <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> is een HMAC-SHA256 van de ruwe inhoud met het ondertekeningsgeheim als sleutel. De inhoud en de controles aan ontvangende kant staan op de pagina Webhooks van de calrs-documentatie.
webhooks-state-suspended = Opgeschort: wie hem heeft toegevoegd, kan dit afspraaktype niet meer beheren. Neem hem over om hem te hervatten.
webhooks-adopt = Overnemen
webhooks-notice-adopted = Webhook overgenomen en hervat.
webhooks-event-claimed = Geclaimd door een volger
webhooks-error-address = Dit adres kan niet worden gebruikt: calrs stuurt alleen naar openbare internetadressen, tenzij de host op de lijst met toegestane privéhosts staat.
webhooks-adopt-help = Door hem over te nemen wordt hij van jou en hervat hij. Bevestig het adres waar hij naartoe stuurt en stel een nieuw ondertekeningsgeheim in: wie hem heeft toegevoegd, kent het oude misschien. Afleveringen die nog wachten vervallen.
webhooks-error-not-suspended = Alleen een opgeschorte webhook kan worden overgenomen.
webhooks-error-new-secret = De URL verwijst nu naar een andere host: vul ook een nieuw ondertekeningsgeheim in.
webhooks-url-masked-hint = (alleen wie hem heeft toegevoegd ziet de volledige URL: vul hem opnieuw in om op te slaan)

# Booking questions (#210): booking form, answers, event type editor

book-question-choose = Kies er een
book-question-pick-one = Vink minstens één vakje aan.
book-question-read-policy = Lees het beleid
book-question-new-tab = (opent in een nieuw tabblad)
answer-yes = Ja
answer-no = Nee
answer-agreed = Akkoord
answer-not-agreed = Niet akkoord
answer-you-agreed = Je ging akkoord
answer-you-did-not-agree = Je ging niet akkoord
answer-agreed-recorded-by = Akkoord, vastgelegd door { $name }
answer-recorded-for-you-by = Namens jou vastgelegd door { $name }
bookings-answers = Antwoorden
validate-notes-required = Voeg een opmerking voor de organisator toe.
validate-question-required = Beantwoord "{ $question }".
validate-question-too-long = Het antwoord op "{ $question }" is te lang.
validate-question-unknown-option = Het antwoord op "{ $question }" is geen van de aangeboden keuzes.
validate-questions-invalid = De boekingsvragen konden niet worden gelezen. Laad de pagina opnieuw en probeer het nog eens.
validate-questions-too-many = Een afspraaktype kan hoogstens { $max } boekingsvragen stellen.
validate-question-kind = Vraag { $position }: dit soort vraag is niet beschikbaar.
validate-question-kind-change = Vraag { $position }: een opgeslagen vraag houdt haar soort. Verwijder haar en voeg een nieuwe toe.
validate-question-label-empty = Vraag { $position }: de vraag is leeg.
validate-question-label-too-long = Vraag { $position }: de vraag is te lang (200 tekens, 1000 voor een selectievakje).
validate-question-help-too-long = Vraag { $position }: de helptekst is te lang (hoogstens 500 tekens).
validate-question-options-few = Vraag { $position }: geef minstens twee keuzes.
validate-question-options-many = Vraag { $position }: hoogstens 30 keuzes.
validate-question-option-empty = Vraag { $position }: een keuze is leeg.
validate-question-option-too-long = Vraag { $position }: een keuze is te lang (hoogstens 200 tekens).
validate-question-option-duplicate = Vraag { $position }: twee keuzes hebben dezelfde tekst.
validate-question-link = Vraag { $position }: de link naar het beleid moet een volledig adres zijn dat begint met https:// of http://.
etf-section-booking-form = Boekingsformulier
etf-booking-form-help = Gasten geven altijd hun naam en e-mailadres op. Voeg de vragen toe die dit afspraaktype nodig heeft: gasten beantwoorden ze bij het boeken, en de antwoorden komen met de boeking bij je binnen.
etf-notes-mode = Opmerkingenveld
etf-notes-mode-off = Verborgen
etf-notes-mode-optional = Optioneel
etf-notes-mode-required = Verplicht
etf-q-empty = Nog geen vragen.
etf-q-add = Vraag toevoegen
etf-q-kind = Soort
etf-q-kind-short-text = Korte tekst
etf-q-kind-long-text = Lange tekst
etf-q-kind-single-choice = Eén keuze
etf-q-kind-multiple-choice = Meerdere keuzes
etf-q-kind-checkbox = Selectievakje
etf-q-label = Vraag
etf-q-help = Helptekst
etf-q-required = Verplicht
etf-q-options = Keuzes
etf-q-option = Keuze %1
etf-q-add-option = Keuze toevoegen
etf-q-remove-option = Deze keuze verwijderen
etf-q-consent = Dit vakje legt een toestemming vast (privacybeleid, opname)
etf-q-link = Link naar het beleid
etf-q-link-help = Optioneel. Staat onder het vakje en opent in een nieuw tabblad.
etf-q-in-calendar = Het antwoord in de agenda-afspraak zetten
etf-q-in-calendar-help = Zet dit uit voor gevoelige antwoorden: wie je agenda kan lezen, zou ze zien.
etf-q-not-in-calendar = Niet in de agenda-afspraak
etf-q-consent-word = Toestemming
etf-q-edit = Bewerken
etf-q-done = Klaar
etf-q-move-up = Omhoog
etf-q-move-down = Omlaag
etf-q-remove = Verwijderen
etf-q-untitled = Vraag zonder titel
etf-q-kind-locked = Een opgeslagen vraag houdt haar soort.
etf-q-limit = Hoogstens %1 vragen.
etf-q-preview = Boekingspagina bekijken
etf-q-key = Sleutel voor webhooks: %1
etf-hint-questions-one = 1 vraag
etf-hint-questions-other = %1 vragen
etf-hint-no-questions = Geen vragen
