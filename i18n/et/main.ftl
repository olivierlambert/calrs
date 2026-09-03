# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Broneering ootel
confirmed-page-title-booked = Broneering kinnitatud

confirmed-heading-reschedule-requested = Aja muutmise taotlus saadetud
confirmed-heading-rescheduled = Aeg muudetud!
confirmed-heading-pending = Ootab kinnitust
confirmed-heading-booked = Broneering tehtud!

confirmed-subtitle-reschedule-requested = Sinu taotlus aja muutmiseks saadeti kasutajale { $host }. Kui see kinnitatakse, saad e-kirja aadressil { $email }.
confirmed-subtitle-rescheduled = Sinu broneeringu aeg on muudetud. Kinnituskiri saadeti aadressile { $email }.
confirmed-subtitle-pending = Sinu broneeringu taotlus saadeti kasutajale { $host }. Kui see kinnitatakse, saad e-kirja aadressil { $email }.
confirmed-subtitle-booked = Kinnituskiri saadeti aadressile { $email }.

confirmed-detail-event = Sündmus:
confirmed-detail-date = Kuupäev:
confirmed-detail-time = Kellaaeg:
confirmed-detail-with = Kellega:
confirmed-detail-location = Asukoht:
confirmed-detail-notes = Märkused:
confirmed-detail-additional-guests = Lisakülalised:

confirmed-book-another = Broneeri uus aeg

confirmed-add-to-calendar = Lisa kalendrisse

# Slot picker (templates/slots.html)

slots-location-video = Videokõne
slots-location-phone = Telefonikõne

slots-tz-label = Sinu ajavöönd
slots-time-format-label = Ajavorming

slots-view-month = Kuuvaade
slots-view-week = Nädalavaade
slots-view-column = Veeruvaade

slots-weekday-mon = E
slots-weekday-tue = T
slots-weekday-wed = K
slots-weekday-thu = N
slots-weekday-fri = R
slots-weekday-sat = L
slots-weekday-sun = P

slots-weekday-mon-short = E
slots-weekday-tue-short = T
slots-weekday-wed-short = K
slots-weekday-thu-short = N
slots-weekday-fri-short = R
slots-weekday-sat-short = L
slots-weekday-sun-short = P

slots-select-date = Vali kuupäev
slots-loading-availability = Vabade aegade laadimine...
slots-click-highlighted = Vabade aegade nägemiseks klõpsa esile tõstetud kuupäeval
slots-no-times-month = Sel kuul pole vabu aegu
slots-no-times-day = Sel päeval pole vabu aegu
slots-no-availability-participants = Sel kuul ei leidunud aega, mis sobiks kõigile osalejatele
slots-week-more = veel

# Booking form (templates/book.html)

book-page-title = Broneeri { $title }
book-back-to-times = Tagasi aegade juurde
book-name-label = Sinu nimi
book-name-placeholder = Mari Maasikas
book-email-label = E-post
book-email-placeholder = mari@example.com
book-email-invalid = Palun sisesta täielik e-posti aadress koos domeeniga (nt mari@example.com).
book-notes-label = Märkused
book-notes-optional = (valikuline)
book-notes-placeholder = Kas on midagi, mida soovid arutada?
book-additional-guests-label = Lisakülalised
book-additional-guests-hint = (valikuline, kuni { $max })
book-add-guest-btn = + Lisa külalise e-post
book-guest-email-placeholder = kolleeg@example.com
book-phone-label = Telefoninumber
book-phone-placeholder = 5123 4567
book-phone-help = Kohalikud numbrid sobivad; kui sa ei alusta plussmärgiga, eeldame riiki { $country }.
book-phone-optional-consequence = Jäta tühjaks, kui sa ei soovi selle broneeringu kohta lühisõnumeid saada.
book-phone-required = See broneering nõuab telefoninumbrit.
book-phone-invalid-title = Vigane telefoninumber
book-phone-invalid = Palun sisesta number, millele saame lühisõnumi saata, või jäta väli tühjaks.
book-phone-country-search = Otsi
book-phone-country-label = Vali riik
book-phone-country-none = Riiki pole valitud
book-phone-country-no-results = Ükski riik ei vasta otsingule
captcha-label = Turvakontroll
captcha-initial-state = Kinnita, et oled inimene
captcha-verifying = Kontrollimine...
captcha-solved = Oled inimene
captcha-error = Viga
captcha-troubleshooting = Tõrkeotsing
captcha-wasm-disabled = Luba WASM, et kontroll oleks tunduvalt kiirem
captcha-verify-aria = Klõpsa, et kinnitada, et oled inimene
captcha-verifying-aria = Kontrollime, palun oota
captcha-verified-aria = Kinnitatud
captcha-required = Palun kinnita, et oled inimene
captcha-error-aria = Tekkis viga, palun proovi uuesti
book-confirm-button = Kinnita broneering

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Broneering kinnitatud: { $event }, { $date } kell { $time } ({ $tz }).
sms-cancelled = Broneering tühistatud: { $event }, { $date } kell { $time } ({ $tz }).
sms-rescheduled = Broneering tõstetud: { $event } toimub nüüd { $date } kell { $time } ({ $tz }).
sms-reminder = Meeldetuletus: { $event } algab { $date } kell { $time } ({ $tz }).

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Külaline:
common-detail-reason = Põhjus:
common-reason-optional = (valikuline)
common-close-page = Võid selle lehe sulgeda.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Tühista broneering
cancel-heading = Tühista broneering
cancel-subtitle = Oled tühistamas oma broneeringut.
cancel-reason-label = Põhjus
cancel-reason-placeholder-host = Anna korraldajale teada, miks...
cancel-button = Tühista broneering
cancelled-heading = Broneering tühistatud
cancelled-subtitle = Sinu broneering on tühistatud ja korraldajat on teavitatud.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Lükka broneering tagasi
decline-heading = Lükka broneering tagasi
decline-subtitle = Oled tagasi lükkamas seda broneeringutaotlust.
decline-reason-placeholder-guest = Anna külalisele teada, miks...
decline-button = Lükka broneering tagasi
declined-heading = Broneering tagasi lükatud
declined-subtitle = Broneering lükati tagasi ja külalist on teavitatud.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Kinnita broneering
approve-heading = Kinnita broneering
approve-subtitle = Oled kinnitamas seda broneeringutaotlust.
approve-button = Kinnita broneering
approved-heading = Broneering kinnitatud
approved-subtitle = Broneering on kinnitatud ja kinnituskiri saadeti aadressile { $email }.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Võta broneering üle
claim-heading = Võta broneering üle
claim-subtitle = Oled seda broneeringut üle võtmas. Sind lisatakse osalejaks.
claim-assigned-to = Määratud:
claim-button = Võta see broneering üle
claimed-page-title = Broneering üle võetud
claimed-heading = Broneering üle võetud
claimed-subtitle = Võtsid selle broneeringu üle. Kalendrikutse saadeti sinu e-posti aadressile.
already-claimed-page-title = Juba üle võetud
already-claimed-heading = Juba üle võetud
already-claimed-subtitle = Selle broneeringu on juba üle võtnud { $name }.

# Generic error page (booking_action_error.html)

action-error-page-title = Viga broneeringu toimingus

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Muuda broneeringu aega — calrs
host-resched-heading = Muuda broneeringu aega
host-resched-subtitle = See saadab külalisele { $guest } e-kirja palvega valida uus aeg.
host-resched-currently = Praegu:
host-resched-button = Saada aja muutmise palve
host-resched-cancel-link = Loobu

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Kinnita aja muutmine
resched-confirm-heading = Kinnita aja muutmine
resched-confirm-subtitle = Oled oma broneeringut uuele ajale tõstmas.
resched-was = Oli:
resched-new = Uus:
resched-button = Kinnita aja muutmine
resched-back-to-picker = Tagasi aja valimise juurde

# Base layout chrome (templates/base.html)

base-loader-checking = Vabade aegade kontrollimine
base-loader-please-wait = Palun oota, laadime värskeimaid kalendriandmeid...
base-stop-impersonating = Lõpeta teise kasutajana toimetamine
base-theme-toggle = Vaheta teemat
base-powered-by = Töötab platvormil

# Profile (templates/profile.html)

profile-pick-event-type-invite = Aja broneerimiseks vali sündmuse tüüp.
profile-no-event-type = Sündmuse tüüpe pole veel saadaval.

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

common-month-1 = jaanuar
common-month-2 = veebruar
common-month-3 = märts
common-month-4 = aprill
common-month-5 = mai
common-month-6 = juuni
common-month-7 = juuli
common-month-8 = august
common-month-9 = september
common-month-10 = oktoober
common-month-11 = november
common-month-12 = detsember

common-weekday-long-mon = esmaspäev
common-weekday-long-tue = teisipäev
common-weekday-long-wed = kolmapäev
common-weekday-long-thu = neljapäev
common-weekday-long-fri = reede
common-weekday-long-sat = laupäev
common-weekday-long-sun = pühapäev

# Format patterns are parametric per locale to handle word order. Translators
# pick where each placeholder lands. Example outputs:
#   EN: April 2026  /  Tuesday, March 12, 2026
#   FR: avril 2026  /  mardi 12 mars 2026
#   ES: abril 2026  /  martes, 12 de marzo de 2026
common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday }, { $day }. { $month } { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Muuda aega
email-action-cancel-booking = Tühista broneering

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
email-confirm-greeting = Tere, { $name },
email-confirm-headline = Sinu broneering on kinnitatud!
email-confirm-ics-attached-plain = Kalendrikutse on manuses.
email-confirm-ics-attached-html = Kalendrikutse on selle kirja manuses.
email-confirm-need-to-cancel = Kas soovid tühistada? { $url }

# Email: guest reminder

email-reminder-subject = Meeldetuletus: { $event } kell { $time }
email-reminder-headline = Sinu kohtumine on peagi algamas.

# Email: guest cancellation

email-cancel-subject = Tühistatud: { $event } — { $date }
email-cancel-headline-by-host = Sinu broneeringu tühistas { $host }.
email-cancel-headline-by-guest = Sinu broneering on tühistatud.
email-cancel-ics-attached-plain = Kalendritühistus on manuses.
email-cancel-ics-attached-html = Kalendritühistus on selle kirja manuses.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Pane tähele: tühistamine nõuab vähemalt { $minutes } minutit etteteatamist.
email-confirm-reschedule-notice = Pane tähele: aja muutmine nõuab vähemalt { $minutes } minutit etteteatamist.

# Event type form: cancel/reschedule minimum notice (templates/event_type_form.html)

event-type-form-cancel-notice-label = Vähim etteteatamisaeg tühistamiseks
event-type-form-reschedule-notice-label = Vähim etteteatamisaeg aja muutmiseks
event-type-form-notice-help = Jäta tühjaks, kui piirangut pole vaja.
event-type-form-resources-label = Nõutavad ressursid
event-type-form-resources-hint = Aegu pakutakse ainult siis, kui valitud ressursid on allpool oleva režiimi järgi vabad.
event-type-form-resources-mode-all = Kõik valitud ressursid peavad olema vabad
event-type-form-resources-mode-round-robin = Piisab ühest vabast ressursist (see määratakse broneeringule)
event-type-form-notice-unit-minutes = minutit
event-type-form-notice-unit-hours = tundi
event-type-form-notice-unit-days = päeva
event-type-form-booking-horizon-label = Broneerimishorisont
event-type-form-booking-horizon-help = Mitu päeva ette saavad külalised broneerida. Tühi tähendab piiranguta, 0 ainult tänast päeva.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = Tühistamine nõuab enne kohtumist vähemalt { $minutes } minutit etteteatamist.
confirmed-reschedule-notice-info = Aja muutmine nõuab enne kohtumist vähemalt { $minutes } minutit etteteatamist.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Seda broneeringut ei saa enam veebis tühistada
booking-blocked-title-reschedule = Selle broneeringu aega ei saa enam veebis muuta
booking-blocked-body = Korraldaja nõuab vähemalt { $minutes } minutit etteteatamist. Kui sa ei saa osaleda, kirjuta otse aadressil <a href="mailto:{ $host_email }">{ $host_email }</a>.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Kopeeri
dashboard-event-types-copied = Kopeeritud!
dashboard-event-types-copy-title = Kopeeri broneerimislink
dashboard-event-types-copy-failed = Kopeerimine ebaõnnestus

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Ajaplaneerimine
nav-overview = Ülevaade
nav-event-types = Sündmuse tüübid
nav-bookings = Broneeringud
nav-teams = Meeskonnad
nav-section-shared-links = Jagatud lingid
nav-invite-links = Kutselingid
nav-section-calendars = Kalendrid
nav-sources = Allikad
nav-section-personal = Isiklik
nav-settings = Profiil ja seaded
nav-troubleshoot = Tõrkeotsing
nav-section-admin = Haldus
nav-admin-panel = Halduspaneel
nav-sign-out = Logi välja
nav-release-notes = Vaata versiooni märkmeid

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = Sinu brauseri ajavöönd on { $detected }, kuid broneerimise ajavööndiks on määratud { $current }.
tz-banner-update = Uuenda
tz-banner-dismiss = Peida

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = Sisesta URL:
editor-link-default-label = lingi tekst
editor-placeholder-text = tekst
editor-nothing-to-preview = Eelvaateks pole midagi

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Töölaud
overview-welcome = Tere, { $name }
overview-public-page = Avalik leht:
overview-avail-banner-title = Vaikimisi saadavus
overview-avail-banner-body = Sinu vaikimisi tööajaks määrati E–R kell 9.00–17.00. Seda kasutatakse siis, kui teised kaasavad sind dünaamilistesse rühmakohtumistesse.
overview-avail-banner-cta = Vaata oma saadavus üle
overview-dismiss = Peida
overview-getting-started = Alustamine
overview-getting-started-help = Broneeringute vastuvõtmiseks tee läbi need sammud.
overview-step-connect-calendar = Ühenda kalender
overview-step-first-event-type = Loo oma esimene sündmuse tüüp
overview-step-share-link = Jaga oma broneerimislinki
overview-pending-approval = Ootab kinnitust
overview-booking-with = { $title } külalisega { $guest }
overview-badge-pending = ootel
overview-guest-booked = Külalise broneeritud:
overview-confirm = Kinnita
overview-decline = Lükka tagasi
overview-stat-event-types = Sündmuse tüübid
overview-stat-upcoming = Tulevased broneeringud
overview-stat-pending = Kinnitust ootavad
overview-stat-sources = Kalendriallikad
overview-quick-actions = Loo uus sündmuse tüüp
overview-action-public-title = Avalik broneerimisleht
overview-action-public-desc = Jaga linki — igaüks saab valida aja ja sinuga kohtumise broneerida.
overview-action-team-title = Meeskonna ajaplaneerimine
overview-action-team-desc = Jaga broneeringud meeskonnaliikmete vahel või leia aeg, mil kõik on vabad.
overview-action-team-desc-empty = Loo esmalt meeskond ja seejärel seadista ühised sündmuse tüübid.
overview-action-private-title = Privaatne, ainult kutsega
overview-action-private-desc = Loo ühekordsed lingid kindlatele kontaktidele. Keegi teine broneerida ei saa.
overview-action-shared-title = Jagatud kutselingid
overview-action-shared-desc = Iga meeskonna kolleeg saab luua broneerimislinke ja neid väljapoole jagada.
overview-action-reason-calendar = Ühenda esmalt kalender
overview-action-reason-ask-admin = Palu haldajal meeskond luua
overview-action-reason-team-admin = Nõuab meeskonda — loo esmalt üks
overview-action-reason-team-member = Nõuab meeskonda — küsi haldajalt

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Broneeringud
bookings-pending-approval = Ootab kinnitust
bookings-available-to-claim = Saab üle võtta
bookings-upcoming = Tulevased broneeringud
bookings-with = { $title } külalisega { $guest }
bookings-guest-booked = Külalise broneeritud:
bookings-resource = Ressurss:
bookings-confirm = Kinnita
bookings-reschedule = Muuda aega
bookings-decline = Lükka tagasi
bookings-claim = Võta üle
bookings-badge-awaiting-reschedule = ootab aja muutmist
bookings-cancel = Tühista
bookings-reason-placeholder = Põhjus (valikuline)
bookings-confirm-cancel = Kinnita tühistamine
bookings-back = Tagasi
bookings-empty = Tulevasi broneeringuid veel pole.<br>Jaga oma { $link }, et teised saaksid sinuga aja broneerida.
bookings-empty-link-label = sündmuse tüüpide linke

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Meeskonnad
teams-heading = Meeskonnad
teams-new = Uus
teams-badge-public = avalik
teams-badge-private = privaatne
teams-settings = Seaded
teams-view = Vaata
teams-empty = Meeskondi veel pole.
teams-empty-admin = { $link }, et oma meeskonnaga koos töötada.
teams-empty-admin-link-label = Loo üks
teams-empty-member = Meeskondi loovad haldajad. Palu neil luua meeskond ja lisada sind liikmeks.

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Kutselingid
invite-links-heading = Kutselingid
invite-links-new = Uus sisemine sündmus
invite-links-help = Loo sisemistele sündmuse tüüpidele ühekordseid broneerimislinke. Iga sisse loginud kolleeg saab siin linke luua ja jagada.
invite-links-duration = { $minutes } min
invite-links-hosted-by = Korraldab { $host }
invite-links-get-link = Hangi link
invite-links-invites = Kutsed
invite-links-empty = Sisemisi sündmuse tüüpe veel pole.<br>{ $link } nähtavusega „Sisemine“, et iga kolleeg saaks broneerimislinke luua.
invite-links-empty-link-label = Loo sündmuse tüüp
invite-links-js-generating = Loomine...
invite-links-js-copied = Kopeeritud!
invite-links-js-error = Viga

teams-member-count =
    { $count ->
        [one] { $count } liige
       *[other] { $count } liiget
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Kalendriallikad
sources-heading = Kalendriallikad
sources-add = Lisa
sources-last-sync = Viimane sünkroonimine:
sources-sync = Sünkrooni
sources-full-resync = Täielik uuestisünkroonimine
sources-full-resync-title = Tühjenda vahemälu ja lae kõik sündmused serverist uuesti
sources-test = Testi
sources-reconnect = Ühenda uuesti
sources-reconnect-title = Käi Google’i nõusolekuprotsess uuesti läbi
sources-edit = Muuda
sources-remove = Eemalda
sources-remove-confirm = Kas eemaldada allikas „{ $name }“? See kustutab kõik sellest allikast sünkroonitud sündmused.
sources-no-write-calendar = Kirjutamiskalendrit pole valitud. Kinnitatud broneeringud jäävad calrsi ega jõua sellesse kalendrisse. Kirjutamise sisselülitamiseks vali allpool üks.
sources-write-bookings-to = Kirjuta broneeringud kalendrisse:
sources-write-none = Mitte ükski (ära kirjuta)
sources-empty = Ühtegi kalendriallikat pole ühendatud. { $link }, et saadavust kontrollida.
sources-empty-link-label = Lisa üks

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Sündmuse tüübid
event-types-heading = Sündmuse tüübid
event-types-new = Uus
event-types-badge-disabled = välja lülitatud
event-types-badge-internal = sisemine
event-types-badge-private = privaatne
event-types-badge-resources = ressursid
event-types-send-invites = Saada kutsed
event-types-duration = { $minutes } min
event-types-mode-collective = ühine
event-types-mode-round-robin = kordamööda
event-types-edit = Muuda
event-types-disable = Lülita välja
event-types-enable = Lülita sisse
event-types-embed = Manusta
event-types-overrides = Erandid
event-types-team-settings = Meeskonna seaded
event-types-invites = Kutsed
event-types-view-public = Vaata avalikku lehte
event-types-view-page = Vaata lehte
event-types-delete = Kustuta
event-types-delete-confirm = Kas kustutada sündmuse tüüp „{ $title }“? Seda ei saa tagasi võtta.
event-types-empty = Sündmuse tüüpe veel pole. { $link }, et hakata broneeringuid vastu võtma.
event-types-empty-link-label = Loo üks

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Rasvane (Ctrl+B)
editor-italic = Kaldkiri (Ctrl+I)
editor-strikethrough = Läbikriipsutus
editor-code = Kood tekstis
editor-link = Lisa link (Ctrl+K)
editor-toggle-preview = Näita või peida eelvaade
editor-preview = Eelvaade

# Profile and settings (templates/settings.html)

settings-page-title = Seaded
settings-heading = Profiil ja seaded
settings-public-page-label = Sinu avalik broneerimisleht
settings-copy = Kopeeri
settings-copied = Kopeeritud!
settings-open = Ava
settings-avatar = Avatar
settings-upload = Laadi üles
settings-remove = Eemalda
settings-display-name = Kuvatav nimi
settings-display-name-placeholder = Sinu nimi
settings-username = Kasutajanimi
settings-username-hint = (kasutatakse sinu broneerimislingis)
settings-username-pattern-title = Ainult väiketähed, numbrid ja sidekriipsud
settings-username-help = Sinu avalik broneerimisleht:
settings-title = Ametinimetus
settings-title-placeholder = nt tarkvarainsener, tootejuht
settings-title-help = Kuvatakse sinu avalikul profiilil ja külgribal.
settings-bio = Tutvustus
settings-bio-placeholder = Räägi endast veidi...
settings-bio-help = Kuvatakse sinu avalikul broneerimislehel. Toetab **rasvast**, *kaldkirja*, ~~läbikriipsutust~~, `koodi` ja [linke](url).
settings-booking-email = Broneeringute e-post
settings-booking-email-help = See aadress ilmub sinu avalikel broneerimislehtedel ja e-posti teavitustes. Jäta tühjaks, et kasutada sisselogimisaadressi.
settings-booking-email-warning = Veendu, et see aadress sinu e-posti teenusepakkuja juures olemas oleks. Vastasel juhul teavitusi kohale ei toimetata.
settings-timezone = Ajavöönd
settings-timezone-help = Sinu saadavuse reeglid ja broneerimisajad arvutatakse selles ajavööndis.
settings-language = Keel
settings-language-auto = Automaatne (brauseri keel)
settings-language-help = Vali kasutajaliidese keel või jäta valikuks „Automaatne“, et järgida brauseri seadet.
settings-dynamic-group = Luba teistel kaasata mind dünaamilistesse rühmalinkidesse
settings-dynamic-group-help = Kui see on sisse lülitatud, saavad teised kasutajad luua ühekordseid ühiskohtumise linke, mis sind kaasavad (nt { $example }).
settings-lend-resource = Laena minu kalendriõigusi ressursside broneerimiseks
settings-lend-resource-help = Kui broneering peab kinni panema jagatud ressursi (demolabor, koosolekuruum), kuhu sinu kalendrikontol on kirjutusõigus, luba calrsil kasutada selleks sinu salvestatud kalendri pääsuandmeid.
settings-default-availability = Vaikimisi saadavus
settings-default-availability-help = Sinu vaikimisi tööaeg. Kasutatakse dünaamilistes rühmalinkides, kui teised kaasavad sind kohtumisele.
settings-copy-to-all = Kopeeri kõigile päevadele
settings-copy-to-all-title = Kopeeri esimese sisse lülitatud päeva ajavahemikud kõigile teistele sisse lülitatud päevadele
settings-add-window = Lisa ajavahemik
settings-remove-window = Eemalda ajavahemik
settings-save = Salvesta seaded
settings-appearance = Välimus
settings-theme-system = Süsteemi järgi
settings-theme-light = Hele
settings-theme-dark = Tume

# Sign in (templates/auth/login.html)

login-page-title = Logi sisse
login-heading = Logi sisse
login-subtitle = Logi oma calrsi kontole sisse
login-sso = Logi sisse SSO abil
login-or = või
login-email = E-post
login-password = Parool
login-submit = Logi sisse e-postiga
login-no-account = Sul pole veel kontot? { $link }
login-register-link = Registreeru

# Registration (templates/auth/register.html)

register-page-title = Registreerumine
register-heading = Loo konto
register-subtitle = Registreeri uus calrsi konto
register-domains-limited = Registreerumine on lubatud ainult aadressidele: { $domains }
register-name = Nimi
register-name-placeholder = Sinu nimi
register-email = E-post
register-password = Parool
register-password-hint = (vähemalt 12 tähemärki)
register-submit = Loo konto
register-have-account = Sul on juba konto? { $link }
register-signin-link = Logi sisse

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Liiga palju sisselogimiskatseid. Palun proovi hiljem uuesti.
auth-error-invalid-credentials = Vale e-posti aadress või parool
auth-error-internal = Sisemine viga
auth-error-registration-disabled = Registreerumine on välja lülitatud.
auth-error-name-length = Nimi peab olema 1 kuni 255 tähemärki pikk
auth-error-email-length = E-posti aadress peab olema 1 kuni 255 tähemärki pikk
auth-error-email-invalid = Palun sisesta kehtiv e-posti aadress
auth-error-email-domain = See e-posti domeen ei ole lubatud
auth-error-password-length = Parool peab olema vähemalt 12 tähemärki pikk
auth-error-email-taken = See e-posti aadress on juba registreeritud
auth-error-create-failed = Konto loomine ebaõnnestus

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Kalendriallikas
source-test-sync-heading = Sünkroonimine: { $name }
source-test-heading = Ühenduse test
source-write-page-title = Seadista kalendrisse kirjutamine
source-write-back = Tagasi töölauale
source-write-heading = Kuhu peaksid broneeringud minema?
source-write-help = Kui keegi broneerib sinuga kohtumise, saab calrs sündmuse automaatselt sinu kalendrisse luua. Vali, millisesse kalendrisse kirjutada allika { $name } broneeringud.
source-write-save = Salvesta
source-write-skip = Jäta praegu vahele
source-write-sync-results = Sünkroonimise tulemused

source-write-event-count =
    { $count ->
        [one] { $count } sündmus
       *[other] { $count } sündmust
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Kuupäevaerandid
overrides-heading = Kuupäevaerandid
overrides-back-teams = Tagasi meeskondade juurde
overrides-back-event-types = Tagasi sündmuse tüüpide juurde
overrides-intro = Lisa sündmusele { $title } kindlate kuupäevade erandeid
overrides-add-heading = Lisa uus erand
overrides-date = Kuupäev
overrides-type = Erandi liik
overrides-type-blocked = Blokeeri terve päev
overrides-type-custom = Muudetud kellaajad
overrides-start-time = Algusaeg
overrides-end-time = Lõpuaeg
overrides-add-submit = Lisa erand
overrides-existing = Olemasolevad erandid
overrides-badge-blocked = blokeeritud
overrides-badge-custom = muudetud kellaajad
overrides-delete = Kustuta
overrides-delete-confirm = Kas kustutada see erand?
overrides-empty = Kuupäevaerandeid veel pole.<br>Kasuta ülalolevat vormi, et blokeerida kindlaid kuupäevi (pühad, vabad päevad) või määrata muudetud kellaajad.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Aja broneerimiseks vali sündmuse tüüp.
team-profile-empty = Sündmuse tüüpe pole veel saadaval.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Tõrkeotsing
troubleshoot-empty = Sündmuse tüüpe ei leitud. { $link }, et alustada saadavuse tõrkeotsingut.
troubleshoot-empty-link-label = Loo üks
troubleshoot-subtitle = Vaata, miks sündmuse { $title } ajad on vabad või blokeeritud
troubleshoot-duration = { $minutes } min
troubleshoot-buffer-before = { $minutes } min puhvrit enne
troubleshoot-buffer-after = { $minutes } min puhvrit pärast
troubleshoot-min-notice = { $minutes } min etteteatamist
troubleshoot-blocked-override = Blokeeritud kuupäevaerandiga (vaba päev)
troubleshoot-custom-hours-active = Muudetud kellaaegade erand on aktiivne (asendab nädalareeglid)
troubleshoot-legend-available = Vaba
troubleshoot-legend-calendar-event = Kalendrisündmus
troubleshoot-legend-booking = Broneering
troubleshoot-legend-resource = Ressurss hõivatud
troubleshoot-legend-outside = Väljaspool tööaega
troubleshoot-legend-buffer = Puhver / vähim etteteatamine
troubleshoot-blocked-slots = Blokeeritud ajad
troubleshoot-none-date-blocked = See kuupäev on blokeeritud saadavuse erandiga (vaba päev). Vabu aegu pole.
troubleshoot-none-custom-hours = Muudetud kellaaegade erand on aktiivne, kuid ükski ajavahemik ei sobi. Kontrolli erandi seadeid.
troubleshoot-none-no-rules = Sellel nädalapäeval saadavuse reegleid pole. Seda sündmuse tüüpi ei saa { $date } broneerida.
troubleshoot-none-all-bookable = Saadavuse aegade sees pole ühtegi blokeeritud aega. Kõik ajad on broneeritavad.
troubleshoot-label-outside = Väljaspool saadavust
troubleshoot-label-available = Vaba
troubleshoot-label-min-notice = Vähim etteteatamine ({ $minutes } min)
troubleshoot-label-beyond-horizon = Väljaspool broneerimishorisonti ({ $days } päeva)
troubleshoot-label-buffer = Puhver ({ $minutes } min)
troubleshoot-label-resource-busy = Ressurss hõivatud: { $names }
troubleshoot-detail-around = Selle ümber: { $label }
troubleshoot-detail-around-booking = Külalise { $guest } broneeringu ümber
troubleshoot-reason-calendar-event = Kalendrisündmus: { $label }
troubleshoot-reason-booking = Broneering: { $label }

# Invite management (templates/invite_form.html)

invites-heading = Kutsed
invites-back-teams = Tagasi meeskondade juurde
invites-back-event-types = Tagasi sündmuse tüüpide juurde
invites-intro = Saada kutselinke sündmusele { $title }
invites-capped = <strong>Sisestus piirati { $max } saajale korraga.</strong> Saada ülejäänud järgmise partiiga.
invites-failed-hint = — üksikasjad leiad serveri logidest.
invites-quick-link = Kiirlink
invites-quick-link-help = Loo ühekordne link ja kopeeri see lõikelauale.
invites-get-link = Hangi link
invites-or-email = Või saada e-postiga
invites-recipients = Saajad
invites-recipients-hint = (üks aadress real, kuni { $max })
invites-message = Isiklik sõnum
invites-message-hint = (valikuline, saadetakse igale saajale)
invites-message-placeholder = Ootan huviga, et sulle demo näidata...
invites-expires-in = Aegub
invites-expires-days = { $days } päeva pärast
invites-expires-never = Mitte kunagi
invites-allow-multiple = Luba saaja kohta mitu broneeringut
invites-send = Saada kutsed
invites-sent-heading = Saadetud kutsed
invites-badge-expired = aegunud
invites-badge-used = kasutatud
invites-badge-active = aktiivne
invites-sent-by = Saatis { $name }
invites-uses = { $used }/{ $max } kasutust
invites-expires-at = Aegub { $date }
invites-copy-link = Kopeeri link
invites-delete = Kustuta
invites-delete-confirm = Kas kustutada see kutse?
invites-empty = Ühtegi kutset pole veel saadetud. Kasuta ülalolevat vormi, et saata kellelegi broneerimislink.
invites-js-generating = Loomine...
invites-js-copied = Kopeeritud!
invites-js-error = Viga

invites-sent-count =
    { $count ->
        [one] Saadeti { $count } kutse.
       *[other] Saadeti { $count } kutset.
    }

invites-skipped-invalid =
    { $count ->
        [one] Jäeti vahele { $count } vigane rida:
       *[other] Jäeti vahele { $count } vigast rida:
    }

invites-skipped-duplicate =
    { $count ->
        [one] Jäeti vahele { $count } korduv rida:
       *[other] Jäeti vahele { $count } korduvat rida:
    }

invites-failed =
    { $count ->
        [one] { $count } kutse ebaõnnestus (andmebaas või SMTP):
       *[other] { $count } kutset ebaõnnestus (andmebaas või SMTP):
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Muuda kalendriallikat
source-form-title-add = Lisa kalender
source-form-heading-edit = Muuda kalendriallikat
source-form-heading-add = Ühenda kalender
source-form-subtitle-edit = Uuenda ühendust. Jäta parool tühjaks, et praegune alles jääks. Pärast aadressi või kasutajanime muutmist käivita sünkroonimine, et leitud kalendrite loend värskeneks.
source-form-subtitle-add = Ühenda CalDAV-server või Microsoft Exchange (EWS), et calrs saaks broneerimisel saadavust kontrollida.
source-form-backend = Taustsüsteem
source-form-preset = Eelseadistus
source-form-connect-google = Ühenda Google’iga
source-form-google-unavailable = Google’i kalender ei ole saadaval. Võta ühendust haldajaga.
source-form-name = Kuvatav nimi
source-form-name-placeholder = Minu kalender
source-form-url-caldav = CalDAV-i aadress
source-form-url-ews = EWS-i lõpp-punkti aadress
source-form-username = Kasutajanimi
source-form-password = Parool
source-form-password-keep = Jäta tühjaks, et praegune alles jääks
source-form-password-placeholder = Rakenduse parool või konto parool
source-form-skip-test = Jäta ühenduse test vahele
source-form-skip-test-help = Kasuta seda, kui test jääb toppama (juhtub mõne BlueMindi või Zimbra paigalduse puhul). Ühendust saad testida hiljem.
source-form-save = Salvesta muudatused
source-form-add = Lisa kalendriallikas
source-form-help-google-configured = Klõpsa allolevat nuppu, et lubada calrsil oma Google’i kalendrile ligi pääseda.
source-form-help-google-unconfigured = Google’i kalendri sidumine pole veel seadistatud. Palu haldajal sisestada halduspaneelis Google’i OAuth2 pääsuandmed.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — Kasuta oma BlueMindi serveri DAV-lõpp-punkti.<br> Tavaliselt: <code>https://mail.yourcompany.com/dav/</code><br> Kasutajanimi on sinu <strong>e-posti aadress</strong> (nt <code>alice@yourcompany.com</code>), mitte ainult sisselogimisnimi.<br> Kui ühenduse test jääb toppama, märgi „Jäta ühenduse test vahele“ ja sünkrooni otse.
source-form-help-nextcloud = <strong>Nextcloud</strong> — Kasuta WebDAV-i juurkataloogi, mitte üksiku kalendri aadressi.<br> Tavaliselt: <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — Kasuta aadressi teel oma täielikku e-posti aadressi.<br> Näide: <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Kasuta rakenduse parooli (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — Kasuta <code>https://caldav.icloud.com/</code><br> Vajad rakenduse parooli aadressilt <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Turvalisus &rarr; Rakendusepõhised paroolid).
source-form-help-zimbra = <strong>Zimbra</strong> — Kasuta oma Zimbra serveri DAV-lõpp-punkti.<br> Tavaliselt: <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — Kasuta SOGo DAV-lõpp-punkti.<br> Tavaliselt: <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — Kasuta serveri juuraadressi.<br> Tavaliselt: <code>https://cal.example.com/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong>. Kasuta SOAP-lõpp-punkti:<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> Kasutajanimi on postkasti aadress; parool peab lubama HTTP Basicut üle TLS-i (kui teie keskkonnas on Basic keelatud, luba see teeninduspostkastil).<br> Vali kindlasti ka ülalolevast taustsüsteemi menüüst <strong>Microsoft Exchange (EWS)</strong>.
source-form-help-google = <strong>Google’i kalender</strong>: ühendus OAuth2 kaudu. Parooli pole vaja.<br>
source-form-help-other = Sisesta oma CalDAV-serveri <strong>DAV-i juuraadress</strong> — mitte üksiku kalendri oma ega avalikku linki.<br> calrs leiab sinu kalendrid ise PROPFINDi abil (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Rasvane
editor-italic-short = Kaldkiri
editor-link-short = Lisa link

# Team creation (templates/team_form.html)

team-form-heading = Uus meeskond
team-form-name = Meeskonna nimi
team-form-name-placeholder = Arendus
team-form-slug = Identifikaator
team-form-slug-hint = (aadressisõbralik identifikaator)
team-form-slug-pattern-title = Ainult väiketähed, numbrid ja sidekriipsud
team-form-description = Kirjeldus
team-form-optional = (valikuline)
team-form-description-placeholder = Millega see meeskond tegeleb...
team-form-description-help = Kuvatakse meeskonna lehel. Toetab **rasvast**, *kaldkirja* ja [linke](url).
team-form-visibility = Nähtavus
team-form-public = Avalik
team-form-private = Privaatne
team-form-visibility-help = Privaatsed meeskonnad saavad jagamiseks kutsetokeni. Avalikud on näha meeskonna profiililehel.
team-form-members = Liikmed
team-form-members-help = Sind lisatakse automaatselt meeskonna haldajaks. Lisa üksikuid kasutajaid või seo OIDC-rühmi.
team-form-search-placeholder = Otsi kasutajaid või rühmi...
team-form-search-users = Kasutajad
team-form-search-groups = OIDC-rühmad
team-form-you = (sina)
team-form-submit = Loo meeskond

# Team settings (templates/team_settings.html)

team-settings-page-title = Seaded
team-settings-subtitle = Meeskonna seaded — neid saavad muuta meeskonna haldajad.
team-settings-public-url = Avalik aadress
team-settings-public-url-help = Selle lingi kaudu saab broneerida igaüks.
team-settings-invite-link = Kutselink
team-settings-invite-link-help = Jaga seda linki, et anda ligipääs selle privaatse meeskonna broneerimislehele.
team-settings-avatar = Meeskonna avatar
team-settings-profile = Profiil
team-settings-description-placeholder = Räägi sellest meeskonnast...
team-settings-description-help = Kuvatakse meeskonna avalikul broneerimislehel. Toetab **rasvast**, *kaldkirja* ja [linke](url).
team-settings-visibility-help = Avalikud meeskonnad on näha meeskonna profiililehel. Privaatsed vajavad kutselinki.
team-settings-members-help = Halda selle meeskonna koosseisu. Lisa üksikuid kasutajaid või seo OIDC-rühmi automaatseks sünkroonimiseks.
team-settings-role-member = Liige
team-settings-role-admin = Haldaja
team-settings-oidc-group = OIDC-rühm
team-settings-remove = Eemalda
team-settings-save = Salvesta muudatused
team-settings-danger-zone = Ohtlik ala
team-settings-danger-help = Kustuta see meeskond jäädavalt. Sündmuse tüübid seotakse lahti, mitte ei kustutata. Seda ei saa tagasi võtta.
team-settings-delete = Kustuta see meeskond
team-settings-delete-confirm = Kas kustutada meeskond „{ $name }“? Seda ei saa tagasi võtta.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Muuda sündmuse tüüpi
etf-heading-new = Uus sündmuse tüüp
etf-team = Meeskond
etf-team-hint = (valikuline — jäta tühjaks isikliku sündmuse tüübi jaoks)
etf-team-personal = Isiklik
etf-scheduling-mode = Jaotusrežiim
etf-mode-round-robin = Kordamööda — määra ühele vabale liikmele
etf-mode-collective = Ühine — kõik liikmed peavad olema vabad
etf-scheduling-mode-help = „Kordamööda“ määrab broneeringu ühele vabale liikmele (kõigepealt kõige vähem hõivatule). „Ühine“ nõuab, et kõik liikmed oleksid samal ajal vabad.
etf-title = Pealkiri
etf-title-placeholder = 30-minutiline tutvumisvestlus
etf-slug = Identifikaator
etf-slug-placeholder = luuakse pealkirja järgi
etf-description-placeholder = Lühike tutvumisvestlus, et arutada...
etf-description-help = Kuvatakse broneerimislehel. Toetab **rasvast**, *kaldkirja* ja [linke](url).
etf-location = Asukoht
etf-location-link = Videokõne (püsiv aadress)
etf-location-jitsi = Jitsi (automaatselt loodud ruum)
etf-location-webhook = Webhook (oma teenusepakkuja)
etf-location-phone = Telefon
etf-location-in-person = Kohapeal
etf-location-custom = Kohandatud
etf-location-details = Üksikasjad
etf-location-details-placeholder = https://meet.example.com/minu-ruum
etf-pattern-placeholder = Jäta tühjaks, et kasutada organisatsiooni vaikemustrit
etf-duration = Kestus (minutites)
etf-slot-interval = Aegade vahe (minutites)
etf-slot-interval-placeholder = Sama mis kestus
etf-slot-interval-help = Kui tihti ajad algavad. Jäta tühjaks, et järgida kestust.
etf-required-members = Nõutavad liikmed
etf-required-members-help = Aja pakkumiseks peavad kõik märgitud liikmed olema vabad. Eemalda märge liikmetelt, keda soovid välja jätta (nende saadavust ei arvestata).
etf-member-priority = Liikmete prioriteet
etf-member-priority-help = Kõrgema prioriteediga liikmed saavad broneeringuid esimesena, kui nad on vabad. Võrdse prioriteedi korral otsustab hiljutiste broneeringute arv.
etf-member-timezone-title = Liikme ajavöönd. Tema isiklikku tööaega tõlgendatakse selles ajavööndis.
etf-priority-high = Kõrge
etf-priority-medium = Keskmine
etf-priority-low = Madal
etf-section-availability = Saadavus
etf-timezone-help = Allolevaid kellaaegu tõlgendatakse selles ajavööndis. Meeskonna sündmuse tüüpide puhul vali meeskonna tööajavöönd (mitte tingimata looja oma).
etf-reset-default = Taasta minu vaikeväärtused
etf-reset-default-title = Asenda need kellaajad sinu profiili vaikimisi saadavusega
etf-availability-prefilled = Eeltäidetud sinu { $link } põhjal. Võid selle siin selle sündmuse tüübi jaoks üle kirjutada.
etf-availability-prefilled-link = vaikimisi saadavuse
etf-section-buffers = Puhvrid ja etteteatamine
etf-buffer-before = Puhver enne (min)
etf-buffer-after = Puhver pärast (min)
etf-min-notice = Vähim etteteatamisaeg
etf-min-notice-help = Kui palju aega ette tuleb broneerida.
etf-section-limits = Broneerimispiirangud
etf-first-slot-only = Üks aeg päevas
etf-first-slot-only-help = Näita iga päeva kohta ainult kõige varasemat vaba aega.
etf-freq-limit = Piira broneerimise sagedust
etf-freq-limit-help = Piira, mitu korda seda sündmust saab perioodi jooksul broneerida.
etf-add-limit = Lisa piirang
etf-section-options = Broneerimisvalikud
etf-requires-confirmation = Nõuab kinnitust
etf-requires-confirmation-help = Broneeringud jäävad ootele, kuni sa need töölaual kinnitad.
etf-sms = SMS-teavitused
etf-sms-off = Väljas, telefoninumbrit ei küsita
etf-sms-optional = Valikuline, külalised võivad numbri jätta
etf-sms-required = Kohustuslik, külalised peavad numbri jätma
etf-sms-help = Saadab külalisele lisaks e-kirjale lühisõnumi, kui tema broneering kinnitatakse, tõstetakse, tühistatakse või on kohe algamas. Kes jätab välja tühjaks, lihtsalt ei saa lühisõnumit. Nõuab SMS-lüüsi { $link }.
etf-admin-panel-link = halduspaneelis
etf-additional-guests = Lisakülalised
etf-guests-none = Külalised ei saa teisi lisada
etf-additional-guests-help = Luba broneerijal kutsuda lisaosalejaid, kes saavad kalendrikutse.
etf-default-view = Vaikimisi kalendrivaade
etf-view-month = Kuu — kalendriruudustik koos aegade loendiga
etf-view-week = Nädal — seitsme päeva veerud koos aegadega
etf-view-column = Veerg — päevad loendis koos aegadega
etf-view-week-short = nädala
etf-view-column-short = veeru
etf-default-view-help = Vaade, mida külalised esimesena näevad. Nad saavad seda igal ajal vahetada.
etf-conflict-calendars = Konfliktide kontrolli kalendrid
etf-conflict-calendars-help = Vali, milliseid kalendreid konfliktide suhtes kontrollida. Kui ühtegi ei vali, kasutatakse kõiki.
etf-no-resources = Ühtegi jagatud ressurssi pole veel seadistatud. Lisa { $link } mõni (demolabor, koosolekuruum), et seda siin nõuda.
etf-section-access = Ligipääs ja teavitused
etf-visibility-public = Avalik — nähtav sinu profiilil
etf-visibility-internal = Sisemine — iga kolleeg saab luua kutselinke
etf-visibility-private = Privaatne — ainult kutselingiga
etf-visibility-help = Määrab, kes saab seda sündmuse tüüpi näha ja broneerida.
etf-vis-internal = Sisemine
etf-reminder = Broneeringu meeldetuletus
etf-reminder-none = Meeldetuletust ei saadeta
etf-reminder-help = Saada enne kohtumist meeldetuletuskiri nii sinule kui ka külalisele.
etf-dynamic-group = Dünaamiline rühmalink
etf-dynamic-group-help = Loo ühekordne kohtumislink, mis kontrollib sinu ja teiste kasutajate saadavust.
etf-dynamic-group-search = Otsi lisatavat kasutajat...
etf-dynamic-group-note = Näidatakse ainult kasutajaid, kes lubavad dünaamilisi rühmalinke.
etf-dynamic-group-url = Rühmalingi aadress
etf-watcher-teams = Jälgivad meeskonnad
etf-watcher-teams-help = Valitud meeskondi teavitatakse igast broneeringust. Nende liikmed saavad broneeringu üle võtta, et sellel osaleda.
etf-save = Salvesta muudatused
etf-create = Loo sündmuse tüüp
etf-js-loading = Laadimine...
etf-js-no-default = Vaikeväärtust pole määratud
etf-js-reset-done = Taastatud!
etf-js-error = Viga
etf-js-remove-limit = Eemalda piirang
etf-period-day = Päevas
etf-period-week = Nädalas
etf-period-month = Kuus
etf-period-year = Aastas

# Event type form: runtime summary hints (templates/event_type_form.html)


# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = Ühtegi päeva pole määratud
etf-hint-every-day = Iga päev
etf-fmt-day-one = %1 päev
etf-fmt-day-other = %1 päeva
etf-fmt-hours = %1 h
etf-fmt-minutes = %1 min
etf-hint-buffer-both = %1 min enne, %2 min pärast
etf-hint-buffer-before = %1 min puhvrit enne
etf-hint-buffer-after = %1 min puhvrit pärast
etf-hint-notice = %1 etteteatamist
etf-hint-no-buffers = Puhvriteta, broneeritav igal ajal
etf-hint-max = Kuni %1
etf-hint-period-day = /päevas
etf-hint-period-week = /nädalas
etf-hint-period-month = /kuus
etf-hint-period-year = /aastas
etf-hint-no-limits = Piiranguteta
etf-hint-confirmation-required = Nõuab kinnitust
etf-hint-auto-confirmed = Kinnitatakse automaatselt
etf-hint-extra-guests-one = kuni %1 lisakülaline
etf-hint-extra-guests-other = kuni %1 lisakülalist
etf-hint-view = %1vaade
etf-hint-reminder = meeldetuletus %1 varem
etf-hint-no-reminder = meeldetuletuseta

etf-guests-up-to =
    { $count ->
        [one] Kuni { $count } lisakülaline
       *[other] Kuni { $count } lisakülalist
    }

etf-reminder-hours =
    { $count ->
        [one] { $count } tund varem
       *[other] { $count } tundi varem
    }

etf-reminder-days =
    { $count ->
        [one] { $count } päev varem
       *[other] { $count } päeva varem
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Lood <strong>avalikku</strong> sündmuse tüüpi &mdash; broneerida saab igaüks, kellel on link.
etf-preset-private = Lood <strong>privaatset</strong> sündmuse tüüpi &mdash; broneerida saavad ainult need, keda kutsud.
etf-preset-internal = Lood <strong>sisemist</strong> sündmuse tüüpi &mdash; iga kolleeg saab broneerimislinki jagada.
etf-preset-team = Lood <strong>meeskonna</strong> sündmuse tüüpi &mdash; broneeringud jaotatakse meeskonnaliikmete vahel.
etf-pattern-hint = Valikuline oma muster. Kohatäited: <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Jäta tühjaks, et kasutada haldaja määratud organisatsiooni vaikemustrit.
etf-pattern-random-warning = Selles mustris puudub kohatäide <code>{"{"}random{"}"}</code>. Kaks sama sündmuse tüübi broneeringut samal päeval satuvad samasse ruumi ja teine külaline võib sattuda esimese kohtumisele. Kasuta püsivaid ruume ainult siis, kui just seda soovidki.
etf-webhook-hint = Iga broneeringu kohtumisaadress võetakse webhookist, mille haldaja on seadistanud jaotises Haldus &rarr; Kohtumise webhook. Siia aadressi vaja ei ole.

# Admin panel (templates/admin.html)

admin-page-title = Haldus
admin-heading = Halduse töölaud
admin-action-refused = Toiming keelati:
admin-logo = Ettevõtte logo
admin-logo-help = Kuvatakse avalikel broneerimislehtedel. Soovitatav: PNG või SVG, kuni 2 MB.
admin-company-link = Ettevõtte link
admin-company-link-help = Avalikel broneerimislehtedel viib logo sellele aadressile. Jäta tühjaks, kui linki pole vaja.
admin-theme = Teema
admin-theme-help = Vali kõigile lehtedele värviteema. Hele ja tume režiim lülituvad sellest sõltumatult — teemad sobituvad mõlemaga.
admin-theme-default = Vaikimisi
admin-theme-default-desc = Puhas sinine
admin-theme-nord-desc = Arktiline härmatis
admin-theme-dracula-desc = Tume lilla
admin-theme-gruvbox-desc = Soe retro
admin-theme-solarized-desc = Ethani klassika
admin-theme-tokyo-desc = Neoonlinn
admin-theme-custom = Kohandatud
admin-theme-custom-desc = Sinu värvid
admin-custom-colors = Kohandatud värvid
admin-color-accent = Rõhuvärv
admin-color-accent-hover = Rõhuvärv hiire all
admin-color-bg = Taust
admin-color-surface = Pind
admin-color-text = Tekst
admin-save-theme = Salvesta teema
admin-users = Kasutajad ({ $count })
admin-user-filter = Filtreeri nime või e-posti järgi…
admin-badge-admin = haldaja
admin-badge-disabled = välja lülitatud
admin-impersonate = Toimeta tema nimel
admin-demote = Võta õigused
admin-promote = Anna õigused
admin-disable = Lülita välja
admin-enable = Lülita sisse
admin-delete = Kustuta
admin-no-users-match = Ükski kasutaja ei vasta filtrile.
admin-no-users = Kasutajaid veel pole.
admin-groups = Rühmad ({ $count })
admin-group-filter = Filtreeri rühma nime järgi…
admin-group-name = Rühma nimi
admin-weight = kaal:
admin-no-groups-match = Ükski rühm ei vasta filtrile.
admin-no-groups = Ühtegi rühma pole veel sünkroonitud. Rühmad tulevad automaatselt sinu OIDC-teenusepakkujalt.
admin-auth-settings = Sisselogimise seaded
admin-registration-enabled = Registreerumine lubatud
admin-allowed-domains = Lubatud e-posti domeenid
admin-allowed-domains-hint = (komadega eraldatud, tühi lubab kõik)
admin-save-auth = Salvesta sisselogimise seaded
admin-system-settings = Süsteemi seaded
admin-base-url = Baasaadress
admin-base-url-help = Selle paigalduse avalik aadress. Kasutatakse OIDC-ümbersuunamistes ja e-kirjade linkides (kinnita/lükka tagasi, tühista, meeldetuletused).
admin-private-hosts = Lubatud privaathostide loend
admin-private-hosts-help = Komadega eraldatud hostinimed, mis tohivad CalDAV-i ja EWS-i allikate puhul osutada privaatsetele või reserveeritud IP-aadressidele (erand SSRF-kaitsest). Lisa ainult hoste, mida ise haldad (näiteks kalendriserver samas Dockeri võrgus). Jäta tühjaks, et kaitse kehtiks kõigi hostide kohta.
admin-unset-env = Eemalda keskkonnamuutuja, et seda siit muuta.
admin-save-system = Salvesta süsteemi seaded
admin-status = Olek:
admin-status-enabled = sees
admin-status-disabled = väljas
admin-status-disabled-paren = (väljas)
admin-status-configured = seadistatud
admin-status-not-configured = seadistamata
admin-via-environment = (keskkonna kaudu)
admin-issuer = Väljastaja:
admin-client-id = Kliendi ID:
admin-instance = Paigaldus:
admin-oidc-settings = OIDC seaded
admin-oidc-enabled = OIDC sees
admin-issuer-url = Väljastaja aadress
admin-client-id-label = Kliendi ID
admin-client-secret = Kliendi saladus
admin-keep-current-hint = (jäta tühjaks, et praegune alles jääks)
admin-keep-current-set-hint = (jäta tühjaks, et praegune alles jääks — praegu on määratud)
admin-keep-unchanged = Jäta tühjaks, et midagi ei muutuks
admin-oidc-auto-register = Registreeri uued OIDC kasutajad automaatselt
admin-save-oidc = Salvesta OIDC seaded
admin-google = Google’i kalender (OAuth2)
admin-save-google = Salvesta Google’i OAuth2 seaded
admin-captcha = Captcha
admin-instance-url = Paigalduse aadress
admin-site-key = Saidi võti
admin-secret = Saladus
admin-widget-url = Vidina skripti aadress
admin-widget-url-help = Muuda, kui CDN on blokeeritud. Muudatused jõustuvad kohe pärast salvestamist.
admin-captcha-disable-help = Jäta paigalduse aadress, saidi võti ja saladus tühjaks, et captcha broneerimislehtedel välja lülitada.
admin-save-captcha = Salvesta captcha seaded
admin-resources = Ressursid
admin-resources-help = Jagatud broneeritavad ressursid (demolabor, koosolekuruumid), mis põhinevad kalendrivoos. Sündmuse tüüpidega seotuna blokeerib hõivatud ressurss broneeringud.
admin-resource-stats = Sündmusi vahemälus: { $events } &middot; Seotud { $attached } sündmuse tüübiga
admin-never = mitte kunagi
admin-resource-sync-failed = (viimane katse ebaõnnestus: { $error })
admin-writeback-enabled = Kirjutamine: sees ({ $via })
admin-writeback-readonly = Kirjutamine: ainult lugemine
admin-teams-allowed = Lubatud meeskonnad:
admin-teams-allowed-none = pole (ainult üldhaldajad)
admin-sync-now = Sünkrooni kohe
admin-test-write = Testi kirjutamist
admin-delete-resource-confirm = Kas kustutada see ressurss? Sündmuse tüübid, mis seda kasutavad, lõpetavad selle kontrollimise.
admin-name = Nimi
admin-name-help = Jäta tühjaks, et võtta nimi voost.
admin-feed-url = ICS-voo aadress (avaldamisaadress)
admin-feed-url-help = BlueMind: ressursi kalendri avalik või privaatne kalendriaadress.
admin-caldav-url = CalDAV-kogumi aadress (kirjutamiseks)
admin-caldav-url-help = Valikuline. BlueMindi puhul tuletatakse see automaatselt voo aadressist.
admin-caldav-username = CalDAV-i kasutajanimi
admin-caldav-password = CalDAV-i parool
admin-resource-teams = Meeskonnad, kes tohivad seda ressurssi kasutada
admin-resource-teams-help = Nende meeskondade haldajad saavad ressursi oma meeskonna sündmuse tüüpidega siduda. Tühi: ainult üldhaldajad.
admin-no-teams = Meeskondi veel pole.
admin-save-resource = Salvesta ressurss
admin-add-resource = Lisa ressurss
admin-jitsi = Jitsi (automaatselt loodud kohtumislingid)
admin-jitsi-help = Kui sündmuse tüübi asukohaks on „Jitsi (automaatselt loodud ruum)“, koostab calrs iga broneeringu jaoks uue ruumiaadressi, lisades allpool oleva mustri sinu Jitsi baasaadressi järele. Väline API-päring pole vajalik.
admin-display-name = Kuvatav nimi
admin-jitsi-display-name-placeholder = nt Meet DYB
admin-jitsi-display-name-help = Kuvatakse külalistele aja valimisel ja broneerimisvormil. Tühjaks jättes kasutatakse „Videokõne“.
admin-room-pattern = Ruumi nime muster
admin-jitsi-disable-help = Jäta baasaadress tühjaks, et Jitsi automaatne loomine välja lülitada.
admin-save-jitsi = Salvesta Jitsi seaded
admin-meeting-webhook = Kohtumise webhook (oma teenusepakkuja)
admin-webhook-url = Webhooki aadress
admin-webhook-display-name-placeholder = nt Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Kuvatakse külalistele üldise sildi „Videokõne“ asemel.
admin-authentication = Autentimine
admin-auth-none = Puudub
admin-auth-hmac = HMAC-SHA256 (päis X-Calrs-Signature)
admin-shared-secret = Jagatud saladus
admin-webhook-disable-help = Jäta aadress tühjaks, et kohtumise webhook välja lülitada.
admin-save-webhook = Salvesta webhooki seaded
admin-smtp = SMTP seaded
admin-smtp-test-sent = Testkiri saadetud.
admin-smtp-test-failed = Testkirja ei õnnestunud saata. Kontrolli serveri logisid ja oma SMTP seadeid.
admin-smtp-env-error = Viga keskkonnast tulevas SMTP seadistuses:
admin-smtp-host = Host:
admin-smtp-from = Saatja:
admin-smtp-enabled = SMTP sees
admin-host = Host
admin-port = Port
admin-tls-mode = TLS-i režiim
admin-tls-starttls = STARTTLS (port 587)
admin-tls-implicit = Kaudne TLS (port 465)
admin-tls-none = Puudub, krüptimata (ainult kohalik MTA)
admin-smtp-username-hint = (jäta tühjaks autentimiseta edastuse jaoks)
admin-from-email = Saatja e-posti aadress
admin-from-name = Saatja nimi
admin-save-smtp = Salvesta SMTP seaded
admin-send-test-email = Saada testkiri aadressile
admin-send-test-email-hint = (vaikimisi sinu konto e-posti aadress)
admin-send-test-email-btn = Saada testkiri
admin-smtp-clear-confirm = Kas kustutada andmebaasi salvestatud SMTP seadistus?
admin-clear-db-config = Kustuta andmebaasi seadistus
admin-sms = SMS-i seaded
admin-sms-help = Valikuline. Lühisõnumeid saadetakse ainult nende sündmuse tüüpide broneeringute puhul, kus „SMS-teavitused“ on sisse lülitatud, ja ainult siis, kui külaline jättis telefoninumbri.
admin-sms-test-sent = Testsõnum saadetud.
admin-sms-test-checked = Pääsuandmed aktsepteeritud.
admin-sms-test-error = SMS-lüüs lükkas päringu tagasi.
admin-sms-captcha-warning = Broneerimisvorm on avalik ja saaja numbri annab külaline, seega on captchata SMS avatud edastuskanal, mille eest võib arve tulla sinule. Seadista ülal captcha ja piira sihtriike oma lüüsi enda seadetes.
admin-sms-sent-today = Täna saadetud:
admin-sms-of-cap = { $cap }-st
admin-sms-config-error = Viga SMS-i seadistuses:
admin-sms-gateway = Lüüs:
admin-sms-account = Konto:
admin-sms-sender = Saatja:
admin-sms-enabled = SMS sees
admin-sms-gateway-label = Lüüs
admin-required-on-switch = Nõutav lüüsi vahetamisel
admin-sms-docs = { $provider } API dokumentatsioon
admin-sms-country = Vaikimisi riigi suunakood
admin-sms-country-hint = (kasutatakse, kui külalised sisestavad kohaliku numbri)
admin-sms-daily-cap = Päevalimiit
admin-sms-daily-cap-hint = (sõnumeid päevas kogu paigalduse kohta, 0 tähendab piiranguta)
admin-sms-daily-cap-help = Limiidi ületamisel lõpetab calrs lühisõnumite saatmise ja jätkab e-kirjadega, nii et ükski broneering ei ebaõnnestu otsa saanud SMS-eelarve tõttu.
admin-save-sms = Salvesta SMS-i seaded
admin-send-test-sms = Saada testsõnum numbrile
admin-send-test-sms-hint-check = (jäta tühjaks, et kontrollida ainult pääsuandmeid)
admin-send-test-sms-hint-e164 = (E.164 vormingus)
admin-test-gateway = Testi lüüsi
admin-sms-clear-confirm = Kas kustutada andmebaasi salvestatud SMS-i seadistus?
admin-sms-allow-all = Luba igal kasutajal oma sündmuse tüüpidel SMS sisse lülitada
admin-sms-allow-all-help = Vaikimisi väljas: lühisõnumid kulutavad siin seadistatud konto krediiti, seega tohivad sündmuse tüübi SMS-režiimi lülitada ainult haldajad.
admin-save-policy = Salvesta reegel
admin-page-of = Lehekülg %1 / %2
admin-show-more-js = Näita veel %1
admin-show-fewer = Näita vähem

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Kas kustutada kasutaja { $email } jäädavalt?{"\u000A"}{"\u000A"}See eemaldab tema kasutajakonto, ajaplaneerimise profiili, kalendriallikad, sündmuse tüübid ja kõik andmed, mis kuuluvad ainult temale. Varasemad broneeringud kustutatakse koos tema sündmuse tüüpidega.{"\u000A"}{"\u000A"}OIDC/SSO kasutajate puhul: kui automaatne registreerumine on sees, luuakse see inimene järgmisel sisselogimisel uuesti.{"\u000A"}{"\u000A"}Seda ei saa tagasi võtta.
admin-system-settings-help = Avalik aadress ja võrguturbe seaded. Neid saab määrata ka keskkonnamuutujatega <code>CALRS_BASE_URL</code> ja <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Kui keskkonnamuutuja on määratud, <strong>on sellel eelis</strong> allpool oleva väärtuse ees.
admin-set-by-env = — määratud keskkonnast ({ $var }), tühistab salvestatud väärtuse
admin-google-help = Google’i kalendri sidumiseks loo OAuth2 pääsuandmed <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console’is</a>. Luba <strong>Google Calendar API</strong> ja lisa seejärel { $redirect_uri } lubatud ümbersuunamisaadressiks.
admin-room-pattern-help = Saadaolevad kohatäited: <code>{"{"}username{"}"}</code> (korraldaja), <code>{"{"}event{"}"}</code> (sündmuse tüübi identifikaator), <code>{"{"}date{"}"}</code> (AAAAKKPP), <code>{"{"}random{"}"}</code> (8 märki). Vaikimisi: { $default }.
admin-room-pattern-warning = Ilma kohatäiteta <code>{"{"}random{"}"}</code> on ruumi nimi ettearvatav: kaks külalist, kes broneerivad sama sündmuse tüübi samal päeval, satuvad ühte ruumi ja näevad teineteise kohtumist. Püsivad ruumid on lubatud (näiteks üks isiklik ruum korraldaja kohta), kuid lülita see sisse ainult siis, kui mõistad tagajärgi.
admin-meeting-webhook-help = Kui sündmuse tüübi asukohaks on „Webhook (oma teenusepakkuja)“, saadab calrs kinnitamisel broneeringu andmed POST-päringuga sellele aadressile ja ootab vastuseks JSON-keha <code>{"{"}"url": "https://..."{"}"}</code>.
admin-auth-hmac-help = HMAC-i korral saadab calrs päringu töötlemata kehast arvutatud <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code>.
admin-tls-none-warning = Vali <strong>Puudub</strong> ainult selles masinas töötava edastusteenuse jaoks, mis ei paku STARTTLS-i või mille sertifikaat on ise allkirjastatud. Kirjad ja kõik pääsuandmed liiguvad siis krüptimata kujul.
admin-smtp-env-error-help = Paranda keskkonnamuutujad <code>CALRS_SMTP_*</code> või eemalda need, et hallata SMTP-d siit andmebaasist.
admin-smtp-env-managed = Hallatakse <strong>keskkonnamuutujatega</strong> (need on andmebaasist ülimuslikud). Muuda muutujaid <code>CALRS_SMTP_*</code> või eemalda need, et hallata SMTP-d siit.
admin-smtp-env-help = Teise võimalusena seadista keskkonnamuutujatega (need on siinsest ülimuslikud): <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> või <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Kohustuslikud on ainult <code>CALRS_SMTP_HOST</code> ja <code>CALRS_SMTP_FROM_EMAIL</code>; jäta kasutajanimi ja parool välja, et edastada kirju kohaliku MTA kaudu ilma autentimiseta.
admin-sms-env-error-help = Paranda keskkonnamuutujad <code>CALRS_SMS_*</code> või eemalda need, et hallata SMS-i siit andmebaasist.
admin-sms-env-managed = Hallatakse <strong>keskkonnamuutujatega</strong> (need on andmebaasist ülimuslikud). Muuda muutujaid <code>CALRS_SMS_*</code> või eemalda need, et hallata SMS-i siit.
admin-sms-env-help = Teise võimalusena seadista keskkonnamuutujatega (need on siinsest ülimuslikud): <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>Twilio prooviredžiim on sees</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Külalised saavad tegeliku sõnumi asemel Twilio eelmääratud malli <code>sms_appointment_reminders</code> ja kohale jõuavad ainult sinu Twilio konsoolis kinnitatud numbrid. See on abivahend proovikontode testimiseks. Eemalda muutuja, enne kui hakkad broneeringuid vastu võtma.

admin-show-more =
    { $count ->
        [one] Näita veel { $count }
       *[other] Näita veel { $count }
    }

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Vali protokoll, mida sinu server räägib. EWS on mõeldud kohapeal majutatud Exchange 2019/2016/2013 jaoks.

admin-sms-going-live = <strong>Enne päriskasutusse minekut:</strong> piira oma lüüsis sihtriike (Twilios kannab see nime Geo Permissions), hoia konto ettemaksuga ja ilma automaatse laadimiseta, ning jäta captcha sisse. Need kolm meedet piiravad koos, kui kalliks võib SMS pumping’u katse minna.

troubleshoot-heading = Saadavuse tõrkeotsing

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = Nimi ja identifikaator on kohustuslikud.
form-error-team-name-length = Nimi tohib olla kuni 255 tähemärki pikk.
form-error-team-description-length = Kirjeldus tohib olla kuni 5000 tähemärki pikk.
form-error-slug-charset = Identifikaator tohib sisaldada ainult väiketähti, numbreid ja sidekriipse.
form-error-slug-reserved = See identifikaator on reserveeritud. Palun vali mõni muu.
form-error-team-slug-taken = Selle identifikaatoriga meeskond on juba olemas.
form-error-title-required = Identifikaatori loomiseks on vaja pealkirja.
form-error-event-type-slug-taken = Selle identifikaatoriga sündmuse tüüp on juba olemas.
form-error-event-type-slug-taken-team = Selles meeskonnas on juba selle identifikaatoriga sündmuse tüüp.
form-error-location-required = Asukoha andmed on kohustuslikud (näiteks videokõne link, telefoninumber või aadress).
form-error-not-team-admin = Sa ei ole selle meeskonna haldaja.
form-error-no-account = Ajaplaneerimise profiili ei leitud. Palun võta ühendust haldajaga.
form-error-all-fields-required = Kõik väljad on kohustuslikud.
form-error-encryption = Krüptimise viga.
form-error-connection-failed = Ühendus ebaõnnestus: { $error }. Kontrolli aadressi ja pääsuandmeid või märgi „Jäta ühenduse test vahele“, et ikkagi salvestada.

# Settings page flash (src/web/mod.rs)

settings-saved = Seaded salvestatud.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = Nimi peab olema 1 kuni 255 tähemärki pikk.
settings-error-username-length = Kasutajanimi peab olema vähemalt 2 tähemärki pikk.
settings-error-username-taken = See kasutajanimi on juba võetud.
settings-error-booking-email = Palun sisesta kehtiv broneeringute e-posti aadress.
settings-error-save-failed = Seadete salvestamine ebaõnnestus.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = Meeskonda ei leitud või sa ei ole selle haldaja.
error-team-not-found = Meeskonda ei leitud.
error-event-type-not-found = Sündmuse tüüpi ei leitud.
error-decrypt-failed = Salvestatud pääsuandmeid ei õnnestunud dekrüptida.
error-source-not-found = Allikat ei leitud.
error-source-no-password = Sellel allikal pole salvestatud parooli.
error-oauth-invalid-state = Vigane olekuparameeter. Palun proovi uuesti.
error-oauth-no-code = Autoriseerimiskoodi ei saadud.
error-oauth-not-configured = Google OAuth2 ei ole seadistatud.
error-no-scheduling-account = Ajaplaneerimise profiili ei leitud.
error-private-event-type-not-found = Privaatset sündmuse tüüpi ei leitud.
error-access-denied = Ligipääs keelatud.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = See aeg ei ole enam saadaval.
error-slot-too-soon = See aeg ei ole enam saadaval (liiga lühikese etteteatamisega).
error-slot-beyond-horizon = See aeg jääb broneerimisaknast välja.
error-invite-required = See sündmuse tüüp nõuab kutselinki.
error-invite-invalid = Vigane kutselink.
error-invite-expired = See kutselink on aegunud.
error-invite-used = See kutselink on juba kasutatud.
error-invalid-date = Vigane kuupäev.
error-invalid-time = Vigane kellaaeg.
error-invalid-date-format = Vigane kuupäevavorming.
error-invalid-time-format = Vigane kellaajavorming.
error-too-many-bookings = Liiga palju broneerimiskatseid. Palun proovi mõne minuti pärast uuesti.
error-too-many-requests = Liiga palju päringuid. Palun proovi hiljem uuesti.
error-no-members-available = Ükski meeskonnaliige ei ole sel ajal saadaval.
error-dynamic-group-public-only = Dünaamilised rühmalingid on saadaval ainult avalike sündmuse tüüpide puhul.
error-user-not-found = Kasutajat ei leitud.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Captcha kontroll ebaõnnestus
bae-title-invalid-booking = Vigased broneeringu andmed
bae-title-unavailable = Praegu pole saadaval
bae-title-cannot-approve = Seda broneeringut ei saa kinnitada
bae-title-invalid-link = Vigane link
bae-title-invalid-or-expired = Vigane või aegunud link
bae-title-booking-not-found = Broneeringut ei leitud
bae-title-already-approved = Juba kinnitatud
bae-title-already-declined = Juba tagasi lükatud
bae-title-already-cancelled = Juba tühistatud
bae-title-booking-cancelled = Broneering tühistatud
bae-title-booking-declined = Broneering tagasi lükatud

# Booking action error page: bodies

bae-body-go-back = Palun mine tagasi ja proovi uuesti.
bae-body-unavailable = Korraldaja ei võta sellele kuupäevale rohkem broneeringuid vastu. Palun vali mõni muu kuupäev või vaata hiljem uuesti.
bae-body-resource-gone = Vajalik ressurss ei ole sel ajal enam saadaval. Palu külalisel valida mõni muu aeg.
bae-body-no-claim-token = Ülevõtmistokenit ei antud.
bae-body-claim-invalid = See link ei kehti enam.
bae-body-booking-gone = Seda broneeringut enam ei ole.
bae-body-decline-link-invalid = See tagasilükkamise link on vigane, aegunud või on broneering juba menetletud.
bae-body-cancel-link-invalid = See tühistamise link on vigane, aegunud või on broneering juba tühistatud.
bae-body-cancel-link-invalid-short = See tühistamise link on vigane või aegunud.
bae-body-reschedule-link-invalid = See aja muutmise link on vigane, aegunud või on broneering juba menetletud.
bae-body-approval-link-invalid = See kinnitamise link on vigane või aegunud.
bae-body-already-approved = See broneering on juba kinnitatud.
bae-body-already-declined = See broneering on juba tagasi lükatud.
bae-body-already-cancelled = See broneering on juba tühistatud.
bae-body-was-cancelled = See broneering tühistati.
bae-body-declined-by-host = Korraldaja lükkas selle broneeringu tagasi.

# Booking form validation (src/web/mod.rs)

validate-name-length = Nimi peab olema 1 kuni 255 tähemärki pikk.
validate-email-length = E-posti aadress peab olema 1 kuni 255 tähemärki pikk.
validate-email-invalid = Palun sisesta kehtiv e-posti aadress.
validate-notes-length = Märkused ei tohi olla pikemad kui 5000 tähemärki.
validate-date-too-far = Rohkem kui aasta ette ei saa broneerida.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = See sündmuse tüüp ei luba lisakülalisi.
guests-too-many =
    { $max ->
        [one] Võid lisada kõige rohkem ühe lisakülalise.
       *[other] Võid lisada kõige rohkem { $max } lisakülalist.
    }
guests-invalid-email = Vigane lisakülalise e-posti aadress: { $email }
dynamic-group-min-usernames = Dünaamilise grupi lingid vajavad vähemalt kahte kasutajanime.
dynamic-group-user-not-found = Kasutajat „{ $username }“ ei leitud.
dynamic-group-user-opted-out = Kasutaja „{ $username }“ ei ole dünaamilise grupi linke lubanud.

error-slot-unavailable-member = See aeg ei ole enam saadaval ({ $username } on hõivatud).
