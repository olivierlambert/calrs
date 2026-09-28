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
slots-time-format-label = Tijdnotatie

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
overview-welcome = Welkom, { $name }
overview-public-page = Openbare pagina:
overview-avail-banner-title = Standaardbeschikbaarheid
overview-avail-banner-body = Je standaardwerktijden zijn ingesteld op ma–vr, 9:00–17:00. Deze worden gebruikt wanneer anderen je toevoegen aan dynamische groepsafspraken.
overview-avail-banner-cta = Controleer je beschikbaarheid
overview-dismiss = Sluiten
overview-getting-started = Aan de slag
overview-getting-started-help = Volg deze stappen om boekingen te gaan ontvangen.
overview-step-connect-calendar = Koppel een agenda
overview-step-first-event-type = Maak je eerste afspraaktype aan
overview-step-share-link = Deel je boekingslink
overview-pending-approval = In afwachting van goedkeuring
overview-booking-with = { $title } met { $guest }
overview-badge-pending = in afwachting
overview-guest-booked = Geboekt door gast:
overview-confirm = Bevestigen
overview-decline = Afwijzen
overview-stat-event-types = Afspraaktypes
overview-stat-upcoming = Komende boekingen
overview-stat-pending = In afwachting van goedkeuring
overview-stat-sources = Agendabronnen
overview-quick-actions = Nieuw afspraaktype aanmaken
overview-action-public-title = Openbare boekingspagina
overview-action-public-desc = Deel een link — iedereen kan een tijdslot kiezen en een afspraak met je boeken.
overview-action-team-title = Teamplanning
overview-action-team-desc = Verdeel boekingen over teamleden of vind een tijd waarop iedereen kan.
overview-action-team-desc-empty = Maak eerst een team aan en stel daarna gedeelde afspraaktypes in.
overview-action-private-title = Privé, alleen op uitnodiging
overview-action-private-desc = Genereer eenmalige links voor specifieke contacten. Niemand anders kan boeken.
overview-action-shared-title = Gedeelde uitnodigingslinks
overview-action-shared-desc = Elke collega in het team kan boekingslinks genereren om extern te delen.
overview-action-reason-calendar = Koppel eerst een agenda
overview-action-reason-ask-admin = Vraag een beheerder om een team aan te maken
overview-action-reason-team-admin = Vereist een team — maak er eerst een aan
overview-action-reason-team-member = Vereist een team — vraag een beheerder

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Boekingen
bookings-pending-approval = In afwachting van goedkeuring
bookings-available-to-claim = Beschikbaar om over te nemen
bookings-upcoming = Komende boekingen
bookings-with = { $title } met { $guest }
bookings-guest-booked = Geboekt door gast:
bookings-resource = Faciliteit:
bookings-confirm = Bevestigen
bookings-reschedule = Verzetten
bookings-decline = Afwijzen
bookings-claim = Overnemen
bookings-badge-awaiting-reschedule = wacht op nieuw tijdstip
bookings-cancel = Annuleren
bookings-reason-placeholder = Reden (optioneel)
bookings-confirm-cancel = Annulering bevestigen
bookings-back = Terug
bookings-empty = Nog geen komende boekingen.<br>Deel je { $link } zodat mensen een afspraak met je kunnen boeken.
bookings-empty-link-label = links naar afspraaktypes

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
sources-write-bookings-to = Boekingen schrijven naar:
sources-write-none = Geen (niet schrijven)
sources-empty = Geen agendabronnen gekoppeld. { $link } om beschikbaarheid te controleren.
sources-empty-link-label = Voeg er een toe

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Afspraaktypes
event-types-heading = Afspraaktypes
event-types-new = Nieuw
event-types-badge-disabled = uitgeschakeld
event-types-badge-internal = intern
event-types-badge-private = privé
event-types-badge-resources = faciliteiten
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
event-types-empty = Nog geen afspraaktypes. { $link } om boekingen te gaan ontvangen.
event-types-empty-link-label = Maak er een aan

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
etf-conflict-calendars-help = Kies welke agenda's worden gecontroleerd op conflicten. Als je niets selecteert, worden alle agenda's gebruikt.
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
admin-heading = Beheerdashboard
admin-action-refused = Actie geweigerd:
admin-logo = Bedrijfslogo
admin-logo-help = Wordt getoond op openbare boekingspagina's. Aanbevolen: PNG of SVG, max. 2 MB.
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
admin-users = Gebruikers ({ $count })
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
admin-groups = Groepen ({ $count })
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
admin-keep-current-set-hint = (laat leeg om de huidige waarde te behouden — momenteel ingesteld)
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
admin-sms-daily-cap-hint = (berichten per dag voor de hele installatie, 0 voor geen limiet)
admin-sms-daily-cap-help = Boven de limiet stopt calrs met SMS'en en blijft het e-mails versturen, zodat boekingen nooit mislukken omdat het SMS-budget op is.
admin-save-sms = SMS-instellingen opslaan
admin-send-test-sms = Stuur een testbericht naar
admin-send-test-sms-hint-check = (laat leeg om alleen de inloggegevens te controleren)
admin-send-test-sms-hint-e164 = (E.164-notatie)
admin-test-gateway = Gateway testen
admin-sms-clear-confirm = De SMS-configuratie in de database verwijderen?
admin-sms-allow-all = Alle gebruikers SMS laten inschakelen voor hun afspraaktypes
admin-sms-allow-all-help = Standaard uit: SMS kost tegoed op het hier ingestelde account, dus alleen beheerders mogen een afspraaktype in een SMS-modus zetten.
admin-save-policy = Beleid opslaan
admin-page-of = Pagina %1 van %2
admin-show-more-js = Nog %1 tonen
admin-show-fewer = Minder tonen

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Gebruiker { $email } definitief verwijderen?{"\u000A"}{"\u000A"}Hiermee worden het gebruikersrecord, het planningsaccount, de agendabronnen, de afspraaktypes en alle gegevens die alleen van deze gebruiker zijn verwijderd. Eerdere boekingen worden samen met hun afspraaktypes verwijderd.{"\u000A"}{"\u000A"}Voor OIDC/SSO-gebruikers: als automatisch registreren is ingeschakeld, wordt deze persoon bij de volgende keer inloggen opnieuw aangemaakt.{"\u000A"}{"\u000A"}Dit kan niet ongedaan worden gemaakt.
admin-system-settings-help = Openbare URL en netwerkbeveiligingsinstellingen. Deze kun je ook instellen met de omgevingsvariabelen <code>CALRS_BASE_URL</code> en <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Als een omgevingsvariabele is ingesteld, <strong>heeft die voorrang</strong> op de waarde hieronder.
admin-set-by-env = — ingesteld via omgevingsvariabele ({ $var }), overschrijft de opgeslagen waarde
admin-google-help = Om de koppeling met Google Agenda in te schakelen, maak je OAuth2-inloggegevens aan in de <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Schakel de <strong>Google Calendar API</strong> in en voeg daarna { $redirect_uri } toe als geautoriseerde redirect-URI.
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
form-error-location-required = Locatiegegevens zijn verplicht (bijv. een link voor een videogesprek, een telefoonnummer of een adres).
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
