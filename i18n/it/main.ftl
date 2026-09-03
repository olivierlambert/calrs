# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Prenotazione in attesa
confirmed-page-title-booked = Prenotazione confermata

confirmed-heading-reschedule-requested = Riprogrammazione richiesta
confirmed-heading-rescheduled = Riprogrammato!
confirmed-heading-pending = In attesa di conferma
confirmed-heading-booked = Tutto pronto!

confirmed-subtitle-reschedule-requested = La tua richiesta di riprogrammazione è stata inviata a { $host }. Riceverai un'e-mail all'indirizzo { $email } una volta approvata.
confirmed-subtitle-rescheduled = La tua prenotazione è stata riprogrammata. È stata inviata un'e-mail di conferma a { $email }.
confirmed-subtitle-pending = La tua richiesta di prenotazione è stata inviata a { $host }. Riceverai un'e-mail all'indirizzo { $email } una volta confermata.
confirmed-subtitle-booked = È stata inviata un'e-mail di conferma a { $email }.

confirmed-detail-event = Evento:
confirmed-detail-date = Data:
confirmed-detail-time = Ora:
confirmed-detail-with = Con:
confirmed-detail-location = Luogo:
confirmed-detail-notes = Note:
confirmed-detail-additional-guests = Ospiti aggiuntivi:

confirmed-book-another = Prenota un altro orario

confirmed-add-to-calendar = Aggiungi al calendario

# Slot picker (templates/slots.html)

slots-location-video = Videochiamata
slots-location-phone = Chiamata telefonica
slots-location-google-meet = Google Meet

slots-tz-label = Il tuo fuso orario
slots-time-format-label = Formato dell'ora

slots-view-month = Vista mese
slots-view-week = Vista settimana
slots-view-column = Vista lista

slots-weekday-mon = Lun
slots-weekday-tue = Mar
slots-weekday-wed = Mer
slots-weekday-thu = Gio
slots-weekday-fri = Ven
slots-weekday-sat = Sab
slots-weekday-sun = Dom

slots-weekday-mon-short = L
slots-weekday-tue-short = M
slots-weekday-wed-short = M
slots-weekday-thu-short = G
slots-weekday-fri-short = V
slots-weekday-sat-short = S
slots-weekday-sun-short = D

slots-select-date = Seleziona una data
slots-loading-availability = Caricamento delle disponibilità...
slots-click-highlighted = Clicca su una data evidenziata per vedere gli orari disponibili
slots-no-times-month = Nessun orario disponibile in questo mese
slots-no-times-day = Nessun orario disponibile in questo giorno
slots-no-availability-participants = Nessuna disponibilità comune per tutti i partecipanti in questo mese
slots-week-more = altri

# Booking form (templates/book.html)

book-page-title = Prenota { $title }
book-back-to-times = Torna agli orari
book-name-label = Il tuo nome
book-name-placeholder = Maria Rossi
book-email-label = E-mail
book-email-placeholder = maria@example.com
book-email-invalid = Per favore, inserisci un indirizzo e-mail completo, dominio incluso (per es. jane@example.com).
book-notes-label = Note
book-notes-optional = (opzionale)
book-notes-placeholder = C'è qualcosa di cui vorresti discutere?
book-additional-guests-label = Ospiti aggiuntivi
book-additional-guests-hint = (opzionale, fino a { $max })
book-add-guest-btn = + Aggiungi ospite
book-guest-email-placeholder = collega@example.com
book-phone-label = Numero di telefono
book-phone-placeholder = 312 345 6789
book-phone-help = I numeri locali vanno bene; senza il + iniziale si presume { $country }.
book-phone-optional-consequence = Lascialo vuoto se preferisci non ricevere SMS su questa prenotazione.
book-phone-required = Per questa prenotazione serve un numero di telefono.
book-phone-invalid-title = Numero di telefono non valido
book-phone-invalid = Per favore, inserisci un numero a cui possiamo inviare SMS, oppure lascia il campo vuoto.
book-phone-country-search = Cerca
book-phone-country-label = Scegli il paese
book-phone-country-none = Nessun paese selezionato
book-phone-country-no-results = Nessun paese corrisponde alla ricerca
captcha-label = Verifica di sicurezza
captcha-initial-state = Verifica di essere umano
captcha-verifying = Verifica in corso...
captcha-solved = Sei umano
captcha-error = Errore
captcha-troubleshooting = Risoluzione dei problemi
captcha-wasm-disabled = Abilita WASM per una risoluzione significativamente più rapida
captcha-verify-aria = Clicca per verificare di essere umano
captcha-verifying-aria = Verifica in corso, attendere prego
captcha-verified-aria = Verificato
captcha-required = Verifica di essere umano
captcha-error-aria = Si è verificato un errore, riprova
book-confirm-button = Conferma prenotazione

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Prenotazione confermata: { $event }, { $date } alle { $time } ({ $tz }).
sms-cancelled = Prenotazione annullata: { $event }, { $date } alle { $time } ({ $tz }).
sms-rescheduled = Prenotazione spostata: { $event } è ora il { $date } alle { $time } ({ $tz }).
sms-reminder = Promemoria: { $event } inizia il { $date } alle { $time } ({ $tz }).

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Ospite:
common-detail-reason = Motivo:
common-reason-optional = (opzionale)
common-close-page = Puoi chiudere questa pagina.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Annulla prenotazione
cancel-heading = Annulla prenotazione
cancel-subtitle = Stai per annullare la tua prenotazione.
cancel-reason-label = Motivo
cancel-reason-placeholder-host = Spiega all'organizzatore il motivo...
cancel-button = Annulla prenotazione
cancelled-heading = Prenotazione annullata
cancelled-subtitle = La tua prenotazione è stata annullata e l'organizzatore è stato notificato.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Rifiuta prenotazione
decline-heading = Rifiuta prenotazione
decline-subtitle = Stai per rifiutare questa richiesta di prenotazione.
decline-reason-placeholder-guest = Spiega all'ospite il motivo...
decline-button = Rifiuta prenotazione
declined-heading = Prenotazione rifiutata
declined-subtitle = La prenotazione è stata rifiutata e l'ospite è stato notificato.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Approva prenotazione
approve-heading = Approva prenotazione
approve-subtitle = Stai per approvare questa richiesta di prenotazione.
approve-button = Approva prenotazione
approved-heading = Prenotazione approvata
approved-subtitle = La prenotazione è stata confermata e un'e-mail di conferma è stata inviata a { $email }.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Prendi in carico la prenotazione
claim-heading = Prendi in carico la prenotazione
claim-subtitle = Stai per prendere in carico questa prenotazione. Sarai aggiunto come partecipante.
claim-assigned-to = Assegnata a:
claim-button = Prendi in carico
claimed-page-title = Prenotazione presa in carico
claimed-heading = Prenotazione presa in carico
claimed-subtitle = Hai preso in carico questa prenotazione. Un invito al calendario è stato inviato al tuo indirizzo e-mail.
already-claimed-page-title = Già presa in carico
already-claimed-heading = Già presa in carico
already-claimed-subtitle = Questa prenotazione è già stata presa in carico da { $name }.

# Generic error page (booking_action_error.html)

action-error-page-title = Errore nell'azione di prenotazione

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Riprogramma prenotazione — calrs
host-resched-heading = Riprogramma prenotazione
host-resched-subtitle = Verrà inviata a { $guest } un'e-mail con la richiesta di scegliere un nuovo orario.
host-resched-currently = Attualmente:
host-resched-button = Invia richiesta di riprogrammazione
host-resched-cancel-link = Annulla

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Conferma riprogrammazione
resched-confirm-heading = Conferma riprogrammazione
resched-confirm-subtitle = Stai per spostare la tua prenotazione a un nuovo orario.
resched-was = Prima:
resched-new = Nuovo:
resched-button = Conferma riprogrammazione
resched-back-to-picker = Torna alla selezione dell'orario

# Base layout chrome (templates/base.html)

base-loader-checking = Verifica della disponibilità
base-loader-please-wait = Attendi, caricamento dei dati del calendario...
base-stop-impersonating = Termina impersonificazione
base-theme-toggle = Cambia tema
base-powered-by = Offerto da

# Profile (templates/profile.html)

profile-pick-event-type-invite = Scegli un tipo di evento per prenotare un orario.
profile-no-event-type = Ancora nessun tipo di evento disponibile.

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

common-month-1 = gennaio
common-month-2 = febbraio
common-month-3 = marzo
common-month-4 = aprile
common-month-5 = maggio
common-month-6 = giugno
common-month-7 = luglio
common-month-8 = agosto
common-month-9 = settembre
common-month-10 = ottobre
common-month-11 = novembre
common-month-12 = dicembre

common-weekday-long-mon = lunedì
common-weekday-long-tue = martedì
common-weekday-long-wed = mercoledì
common-weekday-long-thu = giovedì
common-weekday-long-fri = venerdì
common-weekday-long-sat = sabato
common-weekday-long-sun = domenica

# Format patterns are parametric per locale to handle word order. Translators
# pick where each placeholder lands. Example outputs:
#   EN: April 2026  /  Tuesday, March 12, 2026
#   FR: avril 2026  /  mardi 12 mars 2026
#   ES: abril 2026  /  martes, 12 de marzo de 2026
common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday } { $day } { $month } { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Riprogramma
email-action-cancel-booking = Annulla prenotazione

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
email-confirm-greeting = Ciao { $name },
email-confirm-headline = La tua prenotazione è confermata!
email-confirm-ics-attached-plain = Un invito al calendario è in allegato.
email-confirm-ics-attached-html = Un invito al calendario è in allegato a questa e-mail.
email-confirm-need-to-cancel = Devi annullare? { $url }

# Email: guest reminder

email-reminder-subject = Promemoria: { $event } alle { $time }
email-reminder-headline = Il tuo appuntamento si avvicina.

# Email: guest cancellation

email-cancel-subject = Annullata: { $event } — { $date }
email-cancel-headline-by-host = La tua prenotazione è stata annullata da { $host }.
email-cancel-headline-by-guest = La tua prenotazione è stata annullata.
email-cancel-ics-attached-plain = Un'annullamento al calendario è in allegato.
email-cancel-ics-attached-html = Un'annullamento al calendario è in allegato a questa e-mail.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Nota: per annullare servono almeno { $minutes } minuti di preavviso.
email-confirm-reschedule-notice = Nota: per spostare servono almeno { $minutes } minuti di preavviso.

# Event type form: cancel/reschedule minimum notice (templates/event_type_form.html)


# Google Meet (English placeholders until translated)
event-type-form-location-google-meet = Google Meet (auto-generated link)
event-type-form-location-google-meet-hint = A unique Google Meet link is created on confirmation, owned by the assigned host. Every host (you, or every eligible team member) must have Google Calendar connected with a write-back calendar selected.
google-meet-prereq-no-host = Google Meet requires a host with Google Calendar connected.
google-meet-prereq-no-eligible = Google Meet requires at least one eligible team member with Google Calendar connected.
google-meet-prereq-missing = Google Meet requires every host to have Google Calendar connected with a write-back calendar selected. Still missing: { $names }. Connect them at Dashboard → Calendar sources.
google-meet-unavailable-title = Google Meet is not available
google-meet-dynamic-group-unavailable = The host needs Google Calendar connected with a write-back calendar selected.

event-type-form-cancel-notice-label = Preavviso minimo per annullare
event-type-form-reschedule-notice-label = Preavviso minimo per spostare
event-type-form-notice-help = Lascia vuoto per non porre limiti.
event-type-form-resources-label = Risorse necessarie
event-type-form-resources-hint = Gli slot vengono proposti solo quando le risorse selezionate sono disponibili, secondo la modalità qui sotto.
event-type-form-resources-mode-all = Tutte le risorse selezionate devono essere libere
event-type-form-resources-mode-round-robin = Basta una risorsa libera (viene assegnata alla prenotazione)
event-type-form-notice-unit-minutes = minuti
event-type-form-notice-unit-hours = ore
event-type-form-notice-unit-days = giorni
event-type-form-booking-horizon-label = Orizzonte di prenotazione
event-type-form-booking-horizon-help = Con quanti giorni di anticipo gli ospiti possono prenotare. Vuoto per nessun limite, 0 per il solo giorno corrente.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = Per annullare servono almeno { $minutes } minuti di preavviso prima dell'incontro.
confirmed-reschedule-notice-info = Per spostare servono almeno { $minutes } minuti di preavviso prima dell'incontro.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Questa prenotazione non può più essere annullata online
booking-blocked-title-reschedule = Questa prenotazione non può più essere spostata online
booking-blocked-body = L'organizzatore richiede almeno { $minutes } minuti di preavviso. Se non puoi partecipare, scrivi direttamente a <a href="mailto:{ $host_email }">{ $host_email }</a>.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Copia
dashboard-event-types-copied = Copiato!
dashboard-event-types-copy-title = Copia il link di prenotazione
dashboard-event-types-copy-failed = Copia non riuscita

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Pianificazione
nav-overview = Panoramica
nav-event-types = Tipi di evento
nav-bookings = Prenotazioni
nav-teams = Team
nav-section-shared-links = Link condivisi
nav-invite-links = Link di invito
nav-section-calendars = Calendari
nav-sources = Origini
nav-section-personal = Personale
nav-settings = Profilo e impostazioni
nav-troubleshoot = Diagnostica
nav-section-admin = Amministrazione
nav-admin-panel = Pannello di amministrazione
nav-sign-out = Esci
nav-release-notes = Vedi le note di rilascio

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = Il fuso orario del tuo browser è { $detected }, ma il tuo fuso orario di prenotazione è impostato su { $current }.
tz-banner-update = Aggiorna
tz-banner-dismiss = Ignora

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = Inserisci l'URL:
editor-link-default-label = testo del link
editor-placeholder-text = testo
editor-nothing-to-preview = Niente da visualizzare in anteprima

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Pannello
overview-welcome = Ciao, { $name }
overview-public-page = Pagina pubblica:
overview-avail-banner-title = Disponibilità predefinita
overview-avail-banner-body = Il tuo orario di lavoro predefinito è stato impostato da lunedì a venerdì, 9:00–17:00. Viene usato quando altri ti includono in incontri di gruppo dinamici.
overview-avail-banner-cta = Controlla la tua disponibilità
overview-dismiss = Ignora
overview-getting-started = Per iniziare
overview-getting-started-help = Segui questi passaggi per iniziare ad accettare prenotazioni.
overview-step-connect-calendar = Collega un calendario
overview-step-first-event-type = Crea il tuo primo tipo di evento
overview-step-share-link = Condividi il tuo link di prenotazione
overview-pending-approval = In attesa di approvazione
overview-booking-with = { $title } con { $guest }
overview-badge-pending = in attesa
overview-guest-booked = Prenotato dall'ospite:
overview-confirm = Conferma
overview-decline = Rifiuta
overview-stat-event-types = Tipi di evento
overview-stat-upcoming = Prossime prenotazioni
overview-stat-pending = In attesa di approvazione
overview-stat-sources = Origini calendario
overview-quick-actions = Crea un tipo di evento
overview-action-public-title = Pagina di prenotazione pubblica
overview-action-public-desc = Condividi un link: chiunque può scegliere uno slot e prenotare con te.
overview-action-team-title = Pianificazione di team
overview-action-team-desc = Distribuisci le prenotazioni tra i membri del team, o trova un orario in cui sono tutti liberi.
overview-action-team-desc-empty = Crea prima un team, poi imposta i tipi di evento condivisi.
overview-action-private-title = Privato, solo su invito
overview-action-private-desc = Genera link monouso per contatti specifici. Nessun altro può prenotare.
overview-action-shared-title = Link di invito condivisi
overview-action-shared-desc = Qualsiasi collega del team può generare link di prenotazione da condividere all'esterno.
overview-action-reason-calendar = Collega prima un calendario
overview-action-reason-ask-admin = Chiedi a un amministratore di creare un team
overview-action-reason-team-admin = Richiede un team: creane prima uno
overview-action-reason-team-member = Richiede un team: chiedi a un amministratore

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Prenotazioni
bookings-pending-approval = In attesa di approvazione
bookings-available-to-claim = Da prendere in carico
bookings-upcoming = Prossime prenotazioni
bookings-with = { $title } con { $guest }
bookings-guest-booked = Prenotato dall'ospite:
bookings-resource = Risorsa:
bookings-confirm = Conferma
bookings-reschedule = Sposta
bookings-decline = Rifiuta
bookings-claim = Prendi in carico
bookings-badge-awaiting-reschedule = spostamento in attesa
bookings-cancel = Annulla
bookings-reason-placeholder = Motivo (facoltativo)
bookings-confirm-cancel = Conferma l'annullamento
bookings-back = Indietro
bookings-empty = Ancora nessuna prenotazione in arrivo.<br>Condividi i tuoi { $link } così gli altri possono prenotare con te.
bookings-empty-link-label = link ai tipi di evento

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Team
teams-heading = Team
teams-new = Nuovo
teams-badge-public = pubblico
teams-badge-private = privato
teams-settings = Impostazioni
teams-view = Vedi
teams-empty = Ancora nessun team.
teams-empty-admin = { $link } per collaborare con il tuo team.
teams-empty-admin-link-label = Creane uno
teams-empty-member = I team vengono creati dagli amministratori. Chiedi loro di crearne uno e di aggiungerti come membro.

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Link di invito
invite-links-heading = Link di invito
invite-links-new = Nuovo evento interno
invite-links-help = Genera link di prenotazione monouso per i tipi di evento interni. Qualsiasi collega autenticato può creare e condividere link da qui.
invite-links-duration = { $minutes } min
invite-links-hosted-by = Organizzato da { $host }
invite-links-get-link = Ottieni il link
invite-links-invites = Inviti
invite-links-empty = Ancora nessun tipo di evento interno.<br>{ $link } con visibilità «Interno» per permettere a ogni collega di generare link di prenotazione.
invite-links-empty-link-label = Crea un tipo di evento
invite-links-js-generating = Generazione...
invite-links-js-copied = Copiato!
invite-links-js-error = Errore

teams-member-count =
    { $count ->
        [one] { $count } membro
       *[other] { $count } membri
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Origini calendario
sources-heading = Origini calendario
sources-add = Aggiungi
sources-last-sync = Ultima sincronizzazione:
sources-sync = Sincronizza
sources-full-resync = Risincronizzazione completa
sources-full-resync-title = Svuota la cache e riscarica tutti gli eventi dal server
sources-test = Prova
sources-reconnect = Ricollega
sources-reconnect-title = Ripeti la procedura di consenso Google
sources-edit = Modifica
sources-remove = Rimuovi
sources-remove-confirm = Rimuovere l'origine «{ $name }»? Verranno eliminati tutti gli eventi sincronizzati da questa origine.
sources-no-write-calendar = Nessun calendario di scrittura selezionato. Le prenotazioni confermate restano in calrs e non vengono inviate a questo calendario. Scegline uno qui sotto per attivare la scrittura.
sources-write-bookings-to = Scrivi le prenotazioni in:
sources-write-none = Nessuno (non scrivere)
sources-empty = Nessuna origine calendario collegata. { $link } per controllare la disponibilità.
sources-empty-link-label = Aggiungine una

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Tipi di evento
event-types-heading = Tipi di evento
event-types-new = Nuovo
event-types-badge-disabled = disattivato
event-types-badge-internal = interno
event-types-badge-private = privato
event-types-badge-resources = risorse
event-types-send-invites = Invia inviti
event-types-duration = { $minutes } min
event-types-mode-collective = collettivo
event-types-mode-round-robin = a rotazione
event-types-edit = Modifica
event-types-disable = Disattiva
event-types-enable = Attiva
event-types-embed = Incorpora
event-types-overrides = Eccezioni
event-types-team-settings = Impostazioni del team
event-types-invites = Inviti
event-types-view-public = Vedi la pagina pubblica
event-types-view-page = Vedi la pagina
event-types-delete = Elimina
event-types-delete-confirm = Eliminare il tipo di evento «{ $title }»? L'operazione non può essere annullata.
event-types-empty = Ancora nessun tipo di evento. { $link } per iniziare ad accettare prenotazioni.
event-types-empty-link-label = Creane uno

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Grassetto (Ctrl+B)
editor-italic = Corsivo (Ctrl+I)
editor-strikethrough = Barrato
editor-code = Codice inline
editor-link = Inserisci un link (Ctrl+K)
editor-toggle-preview = Mostra o nascondi l'anteprima
editor-preview = Anteprima

# Profile and settings (templates/settings.html)

settings-page-title = Impostazioni
settings-heading = Profilo e impostazioni
settings-public-page-label = La tua pagina di prenotazione pubblica
settings-copy = Copia
settings-copied = Copiato!
settings-open = Apri
settings-avatar = Avatar
settings-upload = Carica
settings-remove = Rimuovi
settings-display-name = Nome visualizzato
settings-display-name-placeholder = Il tuo nome
settings-username = Nome utente
settings-username-hint = (usato nel tuo URL di prenotazione)
settings-username-pattern-title = Solo lettere minuscole, cifre e trattini
settings-username-help = La tua pagina di prenotazione pubblica:
settings-title = Ruolo
settings-title-placeholder = per es. Ingegnera del software, Product manager
settings-title-help = Compare sul tuo profilo pubblico e nella barra laterale.
settings-bio = Biografia
settings-bio-placeholder = Racconta qualcosa di te...
settings-bio-help = Compare sulla tua pagina di prenotazione pubblica. Supporta **grassetto**, *corsivo*, ~~barrato~~, `codice` e [link](url).
settings-booking-email = E-mail per le prenotazioni
settings-booking-email-help = Questo indirizzo comparirà sulle tue pagine di prenotazione pubbliche e nelle notifiche via e-mail. Lascialo vuoto per usare l'indirizzo di accesso.
settings-booking-email-warning = Assicurati che questo indirizzo esista presso il tuo provider di posta. Altrimenti le notifiche non verranno recapitate.
settings-timezone = Fuso orario
settings-timezone-help = Le tue regole di disponibilità e gli orari di prenotazione sono calcolati in questo fuso orario.
settings-language = Lingua
settings-language-auto = Automatico (lingua del browser)
settings-language-help = Scegli una lingua per l'interfaccia, o lascia su Automatico per seguire l'impostazione del browser.
settings-dynamic-group = Consenti ad altri di includermi nei link di gruppo dinamici
settings-dynamic-group-help = Se attivo, altri utenti possono creare al volo URL di incontro collettivo che ti includono (per es. { $example }).
settings-lend-resource = Presta il mio accesso al calendario per le prenotazioni di risorse
settings-lend-resource-help = Quando una prenotazione deve riservare una risorsa condivisa (laboratorio demo, sala riunioni) su cui il tuo account calendario può scrivere, consenti a calrs di usare le tue credenziali salvate per quella scrittura.
settings-default-availability = Disponibilità predefinita
settings-default-availability-help = Il tuo orario di lavoro predefinito. Viene usato per i link di gruppo dinamici quando altri ti includono in un incontro.
settings-copy-to-all = Copia su tutti i giorni
settings-copy-to-all-title = Copia le fasce del primo giorno attivo su tutti gli altri giorni attivi
settings-add-window = Aggiungi una fascia oraria
settings-remove-window = Rimuovi la fascia
settings-save = Salva le impostazioni
settings-appearance = Aspetto
settings-theme-system = Sistema
settings-theme-light = Chiaro
settings-theme-dark = Scuro

# Sign in (templates/auth/login.html)

login-page-title = Accedi
login-heading = Accedi
login-subtitle = Accedi al tuo account calrs
login-sso = Accedi con SSO
login-or = oppure
login-email = E-mail
login-password = Password
login-submit = Accedi con l'e-mail
login-no-account = Non hai un account? { $link }
login-register-link = Registrati

# Registration (templates/auth/register.html)

register-page-title = Registrazione
register-heading = Crea un account
register-subtitle = Registra un nuovo account calrs
register-domains-limited = La registrazione è limitata a: { $domains }
register-name = Nome
register-name-placeholder = Il tuo nome
register-email = E-mail
register-password = Password
register-password-hint = (min. 12 caratteri)
register-submit = Crea un account
register-have-account = Hai già un account? { $link }
register-signin-link = Accedi

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Troppi tentativi di accesso. Per favore, riprova più tardi.
auth-error-invalid-credentials = E-mail o password non validi
auth-error-internal = Errore interno
auth-error-registration-disabled = La registrazione è disattivata.
auth-error-name-length = Il nome deve essere lungo tra 1 e 255 caratteri
auth-error-email-length = L'e-mail deve essere lunga tra 1 e 255 caratteri
auth-error-email-invalid = Per favore, inserisci un indirizzo e-mail valido
auth-error-email-domain = Dominio e-mail non consentito
auth-error-password-length = La password deve essere lunga almeno 12 caratteri
auth-error-email-taken = Questa e-mail è già registrata
auth-error-create-failed = Impossibile creare l'account

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Origine calendario
source-test-sync-heading = Sincronizzazione: { $name }
source-test-heading = Prova di connessione
source-write-page-title = Configura la scrittura sul calendario
source-write-back = Torna al pannello
source-write-heading = Dove devono finire le prenotazioni?
source-write-help = Quando qualcuno prenota un incontro con te, calrs può creare automaticamente l'evento nel tuo calendario. Scegli in quale calendario scrivere le prenotazioni per { $name }.
source-write-save = Salva
source-write-skip = Salta per ora
source-write-sync-results = Esiti della sincronizzazione

source-write-event-count =
    { $count ->
        [one] { $count } evento
       *[other] { $count } eventi
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Eccezioni per data
overrides-heading = Eccezioni per data
overrides-back-teams = Torna ai team
overrides-back-event-types = Torna ai tipi di evento
overrides-intro = Aggiungi eccezioni per date specifiche a { $title }
overrides-add-heading = Aggiungi un'eccezione
overrides-date = Data
overrides-type = Tipo di eccezione
overrides-type-blocked = Blocca l'intera giornata
overrides-type-custom = Orario personalizzato
overrides-start-time = Ora di inizio
overrides-end-time = Ora di fine
overrides-add-submit = Aggiungi l'eccezione
overrides-existing = Eccezioni esistenti
overrides-badge-blocked = bloccata
overrides-badge-custom = orario personalizzato
overrides-delete = Elimina
overrides-delete-confirm = Eliminare questa eccezione?
overrides-empty = Ancora nessuna eccezione per data.<br>Usa il modulo qui sopra per bloccare date specifiche (festività, giorni liberi) o impostare un orario personalizzato.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Scegli un tipo di evento per prenotare un orario.
team-profile-empty = Ancora nessun tipo di evento disponibile.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Diagnostica
troubleshoot-empty = Nessun tipo di evento trovato. { $link } per iniziare a diagnosticare la disponibilità.
troubleshoot-empty-link-label = Creane uno
troubleshoot-subtitle = Scopri perché gli slot di { $title } sono disponibili o bloccati
troubleshoot-duration = { $minutes } min
troubleshoot-buffer-before = { $minutes } min di margine prima
troubleshoot-buffer-after = { $minutes } min di margine dopo
troubleshoot-min-notice = { $minutes } min di preavviso
troubleshoot-blocked-override = Bloccato da un'eccezione per data (giorno libero)
troubleshoot-custom-hours-active = Eccezione con orario personalizzato attiva (sostituisce le regole settimanali)
troubleshoot-legend-available = Disponibile
troubleshoot-legend-calendar-event = Evento di calendario
troubleshoot-legend-booking = Prenotazione
troubleshoot-legend-resource = Risorsa occupata
troubleshoot-legend-outside = Fuori orario
troubleshoot-legend-buffer = Margine / preavviso minimo
troubleshoot-blocked-slots = Slot bloccati
troubleshoot-none-date-blocked = Questa data è bloccata da un'eccezione di disponibilità (giorno libero). Nessuno slot disponibile.
troubleshoot-none-custom-hours = Eccezione con orario personalizzato attiva, ma nessuna fascia corrispondente. Controlla le impostazioni dell'eccezione.
troubleshoot-none-no-rules = Nessuna regola di disponibilità per questo giorno della settimana. Questo tipo di evento non è prenotabile il { $date }.
troubleshoot-none-all-bookable = Nessuno slot bloccato durante l'orario di disponibilità. Tutti gli orari sono prenotabili.
troubleshoot-label-outside = Fuori disponibilità
troubleshoot-label-available = Disponibile
troubleshoot-label-min-notice = Preavviso minimo ({ $minutes } min)
troubleshoot-label-beyond-horizon = Oltre l'orizzonte di prenotazione ({ $days } giorni)
troubleshoot-label-buffer = Margine ({ $minutes } min)
troubleshoot-label-resource-busy = Risorsa occupata: { $names }
troubleshoot-detail-around = Intorno a: { $label }
troubleshoot-detail-around-booking = Intorno alla prenotazione di { $guest }
troubleshoot-reason-calendar-event = Evento di calendario: { $label }
troubleshoot-reason-booking = Prenotazione: { $label }

# Invite management (templates/invite_form.html)

invites-heading = Inviti
invites-back-teams = Torna ai team
invites-back-event-types = Torna ai tipi di evento
invites-intro = Invia link di invito per { $title }
invites-capped = <strong>L'inserimento è stato limitato a { $max } destinatari per invio.</strong> Invia i restanti in un altro giro.
invites-failed-hint = — controlla i log del server per i dettagli.
invites-quick-link = Link rapido
invites-quick-link-help = Genera un link monouso e copialo negli appunti.
invites-get-link = Ottieni il link
invites-or-email = Oppure invia per e-mail
invites-recipients = Destinatari
invites-recipients-hint = (un indirizzo per riga, al massimo { $max })
invites-message = Messaggio personale
invites-message-hint = (facoltativo, inviato a tutti i destinatari)
invites-message-placeholder = Non vedo l'ora di mostrarti una demo...
invites-expires-in = Scade tra
invites-expires-days = { $days } giorni
invites-expires-never = Mai
invites-allow-multiple = Consenti più prenotazioni per destinatario
invites-send = Invia gli inviti
invites-sent-heading = Inviti inviati
invites-badge-expired = scaduto
invites-badge-used = usato
invites-badge-active = attivo
invites-sent-by = Inviato da { $name }
invites-uses = { $used }/{ $max } utilizzi
invites-expires-at = Scade il { $date }
invites-copy-link = Copia il link
invites-delete = Elimina
invites-delete-confirm = Eliminare questo invito?
invites-empty = Ancora nessun invito inviato. Usa il modulo qui sopra per mandare a qualcuno un link di prenotazione.
invites-js-generating = Generazione...
invites-js-copied = Copiato!
invites-js-error = Errore

invites-sent-count =
    { $count ->
        [one] { $count } invito inviato.
       *[other] { $count } inviti inviati.
    }

invites-skipped-invalid =
    { $count ->
        [one] { $count } riga non valida ignorata:
       *[other] { $count } righe non valide ignorate:
    }

invites-skipped-duplicate =
    { $count ->
        [one] { $count } riga duplicata ignorata:
       *[other] { $count } righe duplicate ignorate:
    }

invites-failed =
    { $count ->
        [one] { $count } invito non riuscito (DB o SMTP):
       *[other] { $count } inviti non riusciti (DB o SMTP):
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Modifica l'origine calendario
source-form-title-add = Aggiungi un calendario
source-form-heading-edit = Modifica l'origine calendario
source-form-heading-add = Collega un calendario
source-form-subtitle-edit = Aggiorna la connessione. Lascia la password vuota per mantenere quella esistente. Dopo aver cambiato URL o nome utente, esegui una sincronizzazione per aggiornare l'elenco dei calendari rilevati.
source-form-subtitle-add = Collega un server CalDAV o Microsoft Exchange (EWS) così calrs può controllare la disponibilità quando gli ospiti prenotano.
source-form-backend = Backend
source-form-preset = Preimpostazione
source-form-connect-google = Collega con Google
source-form-google-unavailable = Google Calendar non è disponibile. Rivolgiti all'amministrazione.
source-form-name = Nome visualizzato
source-form-name-placeholder = Il mio calendario
source-form-url-caldav = URL CalDAV
source-form-url-ews = URL dell'endpoint EWS
source-form-username = Nome utente
source-form-password = Password
source-form-password-keep = Lascia vuoto per mantenere quella esistente
source-form-password-placeholder = Password per app o password dell'account
source-form-skip-test = Salta la prova di connessione
source-form-skip-test-help = Usalo se la prova si blocca (succede con alcune installazioni BlueMind o Zimbra). Puoi provare la connessione più tardi.
source-form-save = Salva le modifiche
source-form-add = Aggiungi l'origine calendario
source-form-help-google-configured = Premi il pulsante qui sotto per autorizzare calrs ad accedere al tuo Google Calendar.
source-form-help-google-unconfigured = L'integrazione con Google Calendar non è ancora configurata. Chiedi all'amministrazione di impostare le credenziali OAuth2 di Google nel pannello di amministrazione.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — Usa l'endpoint DAV del tuo server BlueMind.<br> Di norma: <code>https://mail.yourcompany.com/dav/</code><br> Il nome utente è il tuo <strong>indirizzo e-mail</strong> (per es. <code>alice@yourcompany.com</code>), non solo il nome di accesso.<br> Se la prova di connessione si blocca, seleziona «Salta la prova di connessione» e sincronizza direttamente.
source-form-help-nextcloud = <strong>Nextcloud</strong> — Usa la radice WebDAV, non l'URL di un singolo calendario.<br> Di norma: <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — Usa il tuo indirizzo completo nel percorso dell'URL.<br> Esempio: <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Usa una password per app (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — Usa <code>https://caldav.icloud.com/</code><br> Ti serve una password per app da <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Sicurezza &rarr; Password per app).
source-form-help-zimbra = <strong>Zimbra</strong> — Usa l'endpoint DAV del tuo server Zimbra.<br> Di norma: <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — Usa l'endpoint DAV di SOGo.<br> Di norma: <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — Usa l'URL radice del server.<br> Di norma: <code>https://cal.example.com/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong>. Usa l'endpoint SOAP:<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> Il nome utente è l'indirizzo della casella; la password deve accettare HTTP Basic su TLS (attivalo su una casella di servizio se il tuo tenant lo ha disabilitato).<br> Ricordati di scegliere anche <strong>Microsoft Exchange (EWS)</strong> nel menu Backend qui sopra.
source-form-help-google = <strong>Google Calendar</strong>: connessione tramite OAuth2. Nessuna password necessaria.<br>
source-form-help-other = Inserisci l'<strong>URL radice DAV</strong> del tuo server CalDAV, non quella di un singolo calendario né un link pubblico.<br> calrs rileverà i tuoi calendari automaticamente tramite PROPFIND (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Grassetto
editor-italic-short = Corsivo
editor-link-short = Inserisci un link

# Team creation (templates/team_form.html)

team-form-heading = Nuovo team
team-form-name = Nome del team
team-form-name-placeholder = Ingegneria
team-form-slug = Identificativo
team-form-slug-hint = (identificativo adatto agli URL)
team-form-slug-pattern-title = Solo lettere minuscole, cifre e trattini
team-form-description = Descrizione
team-form-optional = (facoltativo)
team-form-description-placeholder = Di cosa si occupa questo team...
team-form-description-help = Compare sulla pagina del team. Supporta **grassetto**, *corsivo* e [link](url).
team-form-visibility = Visibilità
team-form-public = Pubblico
team-form-private = Privato
team-form-visibility-help = I team privati ricevono un token di invito da condividere. I team pubblici compaiono sulla pagina del profilo del team.
team-form-members = Membri
team-form-members-help = Verrai aggiunto automaticamente come amministratore del team. Aggiungi singoli utenti o collega gruppi OIDC.
team-form-search-placeholder = Cerca utenti o gruppi...
team-form-search-users = Utenti
team-form-search-groups = Gruppi OIDC
team-form-you = (tu)
team-form-submit = Crea il team

# Team settings (templates/team_settings.html)

team-settings-page-title = Impostazioni
team-settings-subtitle = Impostazioni del team: possono modificarle gli amministratori del team.
team-settings-public-url = URL pubblico
team-settings-public-url-help = Con questo link chiunque può prenotare.
team-settings-invite-link = Link di invito
team-settings-invite-link-help = Condividi questo link per dare accesso alla pagina di prenotazione di questo team privato.
team-settings-avatar = Avatar del team
team-settings-profile = Profilo
team-settings-description-placeholder = Racconta qualcosa di questo team...
team-settings-description-help = Compare sulla pagina di prenotazione pubblica del team. Supporta **grassetto**, *corsivo* e [link](url).
team-settings-visibility-help = I team pubblici compaiono sulla pagina del profilo del team. Per quelli privati serve un link di invito.
team-settings-members-help = Gestisci chi fa parte di questo team. Aggiungi singoli utenti o collega gruppi OIDC per la sincronizzazione automatica.
team-settings-role-member = Membro
team-settings-role-admin = Amministratore
team-settings-oidc-group = Gruppo OIDC
team-settings-remove = Rimuovi
team-settings-save = Salva le modifiche
team-settings-danger-zone = Zona pericolosa
team-settings-danger-help = Elimina definitivamente questo team. I tipi di evento verranno scollegati, non eliminati. L'operazione non può essere annullata.
team-settings-delete = Elimina questo team
team-settings-delete-confirm = Eliminare il team «{ $name }»? L'operazione non può essere annullata.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Modifica il tipo di evento
etf-heading-new = Nuovo tipo di evento
etf-team = Team
etf-team-hint = (facoltativo: lascia vuoto per un tipo di evento personale)
etf-team-personal = Personale
etf-scheduling-mode = Modalità di assegnazione
etf-mode-round-robin = A rotazione: assegna a un membro disponibile
etf-mode-collective = Collettivo: tutti i membri devono essere disponibili
etf-scheduling-mode-help = «A rotazione» assegna la prenotazione a un membro disponibile (prima il meno impegnato). «Collettivo» richiede che tutti i membri siano liberi nello stesso momento.
etf-title = Titolo
etf-title-placeholder = Chiamata conoscitiva da 30 min
etf-slug = Identificativo
etf-slug-placeholder = generato dal titolo
etf-description-placeholder = Una breve chiamata conoscitiva per parlare di...
etf-description-help = Compare sulla pagina di prenotazione. Supporta **grassetto**, *corsivo* e [link](url).
etf-location = Luogo
etf-location-link = Videochiamata (URL fisso)
etf-location-jitsi = Jitsi (stanza generata automaticamente)
etf-location-webhook = Webhook (fornitore personalizzato)
etf-location-phone = Telefono
etf-location-in-person = Di persona
etf-location-custom = Personalizzato
etf-location-details = Dettagli
etf-location-details-placeholder = https://meet.example.com/mia-stanza
etf-pattern-placeholder = Lascia vuoto per usare lo schema predefinito dell'organizzazione
etf-duration = Durata (minuti)
etf-slot-interval = Intervallo tra gli slot (minuti)
etf-slot-interval-placeholder = Come la durata
etf-slot-interval-help = Ogni quanto iniziano gli slot. Lascia vuoto per seguire la durata.
etf-required-members = Membri necessari
etf-required-members-help = Tutti i membri selezionati devono essere liberi perché uno slot venga proposto. Deseleziona chi vuoi escludere (la sua disponibilità verrà ignorata).
etf-member-priority = Priorità dei membri
etf-member-priority-help = I membri con priorità più alta ricevono prima le prenotazioni, se disponibili. A parità di priorità si bilancia in base alle prenotazioni recenti.
etf-member-timezone-title = Fuso orario del membro. Il suo orario di lavoro personale è interpretato in questo fuso.
etf-priority-high = Alta
etf-priority-medium = Media
etf-priority-low = Bassa
etf-section-availability = Disponibilità
etf-timezone-help = Gli orari qui sotto sono interpretati in questo fuso orario. Per i tipi di evento di team, scegli il fuso di lavoro del team (non per forza quello di chi lo crea).
etf-reset-default = Ripristina i miei valori predefiniti
etf-reset-default-title = Sostituisci questi orari con la disponibilità predefinita del tuo profilo
etf-availability-prefilled = Precompilato dalla tua { $link }. Puoi modificarlo qui per questo tipo di evento.
etf-availability-prefilled-link = disponibilità predefinita
etf-section-buffers = Margini e preavviso
etf-buffer-before = Margine prima (min)
etf-buffer-after = Margine dopo (min)
etf-min-notice = Preavviso minimo
etf-min-notice-help = Con quanto anticipo si deve prenotare.
etf-section-limits = Limiti di prenotazione
etf-first-slot-only = Uno slot al giorno
etf-first-slot-only-help = Mostra solo il primo orario disponibile di ogni giorno.
etf-freq-limit = Limita la frequenza delle prenotazioni
etf-freq-limit-help = Limita quante volte questo evento può essere prenotato per periodo.
etf-add-limit = Aggiungi un limite
etf-section-options = Opzioni di prenotazione
etf-requires-confirmation = Richiede conferma
etf-requires-confirmation-help = Le prenotazioni resteranno in attesa finché non le approvi dal pannello.
etf-sms = Notifiche SMS
etf-sms-off = Disattivate, nessun numero richiesto
etf-sms-optional = Facoltativo, gli ospiti possono lasciare un numero
etf-sms-required = Obbligatorio, gli ospiti devono lasciare un numero
etf-sms-help = Invia un SMS all'ospite, oltre all'e-mail, quando la sua prenotazione viene confermata, spostata, annullata o sta per iniziare. Chi lascia il campo vuoto semplicemente non riceve SMS. Richiede un gateway SMS nel { $link }.
etf-admin-panel-link = pannello di amministrazione
etf-additional-guests = Ospiti aggiuntivi
etf-guests-none = Gli ospiti non possono aggiungerne altri
etf-additional-guests-help = Consenti a chi prenota di invitare altri partecipanti, che riceveranno l'invito nel calendario.
etf-default-view = Vista calendario predefinita
etf-view-month = Mese: griglia del calendario con elenco degli slot
etf-view-week = Settimana: colonne su 7 giorni con gli slot
etf-view-column = Colonna: giorni in elenco con i relativi slot
etf-view-week-short = settimanale
etf-view-column-short = a colonne
etf-default-view-help = La vista che gli ospiti vedono per prima. Possono cambiarla quando vogliono.
etf-conflict-calendars = Calendari per i conflitti
etf-conflict-calendars-help = Scegli quali calendari controllare per i conflitti. Se non ne selezioni nessuno, vengono usati tutti.
etf-no-resources = Ancora nessuna risorsa condivisa configurata. Aggiungine una (laboratorio demo, sala riunioni) nel { $link } per richiederla qui.
etf-section-access = Accesso e notifiche
etf-visibility-public = Pubblico: visibile sul tuo profilo
etf-visibility-internal = Interno: ogni collega può generare link di invito
etf-visibility-private = Privato: solo tramite link di invito
etf-visibility-help = Determina chi può vedere e prenotare questo tipo di evento.
etf-vis-internal = Interno
etf-reminder = Promemoria della prenotazione
etf-reminder-none = Nessun promemoria
etf-reminder-help = Invia un'e-mail di promemoria a te e al tuo ospite prima dell'incontro.
etf-dynamic-group = Link di gruppo dinamico
etf-dynamic-group-help = Crea un link d'incontro estemporaneo che controlla la disponibilità tua e di altri utenti.
etf-dynamic-group-search = Cerca un utente da aggiungere...
etf-dynamic-group-note = Vengono mostrati solo gli utenti che consentono i link di gruppo dinamici.
etf-dynamic-group-url = URL del link di gruppo
etf-watcher-teams = Team osservatori
etf-watcher-teams-help = I team selezionati riceveranno una notifica a ogni prenotazione. I loro membri possono prendere in carico una prenotazione per parteciparvi.
etf-save = Salva le modifiche
etf-create = Crea il tipo di evento
etf-js-loading = Caricamento...
etf-js-no-default = Nessun valore predefinito
etf-js-reset-done = Ripristinato!
etf-js-error = Errore
etf-js-remove-limit = Rimuovi il limite
etf-period-day = Al giorno
etf-period-week = A settimana
etf-period-month = Al mese
etf-period-year = All'anno

# Event type form: runtime summary hints (templates/event_type_form.html)


# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = Nessun giorno impostato
etf-hint-every-day = Tutti i giorni
etf-fmt-day-one = %1 giorno
etf-fmt-day-other = %1 giorni
etf-fmt-hours = %1 h
etf-fmt-minutes = %1 min
etf-hint-buffer-both = %1 min prima, %2 min dopo
etf-hint-buffer-before = %1 min di margine prima
etf-hint-buffer-after = %1 min di margine dopo
etf-hint-notice = %1 di preavviso
etf-hint-no-buffers = Nessun margine, prenotabile a qualsiasi ora
etf-hint-max = Max %1
etf-hint-period-day = /giorno
etf-hint-period-week = /settimana
etf-hint-period-month = /mese
etf-hint-period-year = /anno
etf-hint-no-limits = Nessun limite
etf-hint-confirmation-required = Richiede conferma
etf-hint-auto-confirmed = Conferma automatica
etf-hint-extra-guests-one = fino a %1 ospite in più
etf-hint-extra-guests-other = fino a %1 ospiti in più
etf-hint-view = vista %1
etf-hint-reminder = promemoria %1 prima
etf-hint-no-reminder = nessun promemoria

etf-guests-up-to =
    { $count ->
        [one] Fino a { $count } ospite aggiuntivo
       *[other] Fino a { $count } ospiti aggiuntivi
    }

etf-reminder-hours =
    { $count ->
        [one] { $count } ora prima
       *[other] { $count } ore prima
    }

etf-reminder-days =
    { $count ->
        [one] { $count } giorno prima
       *[other] { $count } giorni prima
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Stai creando un tipo di evento <strong>pubblico</strong> &mdash; chiunque abbia il link può prenotare.
etf-preset-private = Stai creando un tipo di evento <strong>privato</strong> &mdash; possono prenotare solo le persone che inviti.
etf-preset-internal = Stai creando un tipo di evento <strong>interno</strong> &mdash; ogni collega può condividere il link di prenotazione.
etf-preset-team = Stai creando un tipo di evento <strong>di team</strong> &mdash; le prenotazioni vengono distribuite tra i membri del team.
etf-pattern-hint = Schema personalizzato facoltativo. Segnaposto: <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Lascia vuoto per usare lo schema predefinito dell'organizzazione impostato dall'amministrazione.
etf-pattern-random-warning = Questo schema non contiene il segnaposto <code>{"{"}random{"}"}</code>. Due prenotazioni di questo tipo di evento nello stesso giorno condivideranno la stessa stanza, e il secondo ospite può ritrovarsi nell'incontro del primo. Usa stanze fisse solo se è proprio ciò che vuoi.
etf-webhook-hint = L'URL dell'incontro per ogni prenotazione viene recuperato dal webhook che l'amministrazione ha configurato in Amministrazione &rarr; Webhook incontri. Qui non serve alcun URL.

# Admin panel (templates/admin.html)

admin-page-title = Amministrazione
admin-heading = Pannello di amministrazione
admin-action-refused = Azione rifiutata:
admin-logo = Logo aziendale
admin-logo-help = Compare sulle pagine di prenotazione pubbliche. Consigliato: PNG o SVG, max 2 MB.
admin-company-link = Link aziendale
admin-company-link-help = Sulle pagine di prenotazione pubbliche il logo rimanda a questo URL. Lascia vuoto per non inserire alcun link.
admin-theme = Tema
admin-theme-help = Scegli un tema di colori per tutte le pagine. Il passaggio tra chiaro e scuro è indipendente: i temi si adattano a entrambe le modalità.
admin-theme-default = Predefinito
admin-theme-default-desc = Blu pulito
admin-theme-nord-desc = Brina artica
admin-theme-dracula-desc = Viola scuro
admin-theme-gruvbox-desc = Retrò caldo
admin-theme-solarized-desc = Il classico di Ethan
admin-theme-tokyo-desc = Città al neon
admin-theme-custom = Personalizzato
admin-theme-custom-desc = I tuoi colori
admin-custom-colors = Colori personalizzati
admin-color-accent = Colore d'accento
admin-color-accent-hover = Accento al passaggio del mouse
admin-color-bg = Sfondo
admin-color-surface = Superficie
admin-color-text = Testo
admin-save-theme = Salva il tema
admin-users = Utenti ({ $count })
admin-user-filter = Filtra per nome o e-mail…
admin-badge-admin = amministratore
admin-badge-disabled = disattivato
admin-impersonate = Impersona
admin-demote = Retrocedi
admin-promote = Promuovi
admin-disable = Disattiva
admin-enable = Attiva
admin-delete = Elimina
admin-no-users-match = Nessun utente corrisponde al filtro.
admin-no-users = Ancora nessun utente.
admin-groups = Gruppi ({ $count })
admin-group-filter = Filtra per nome del gruppo…
admin-group-name = Nome del gruppo
admin-weight = peso:
admin-no-groups-match = Nessun gruppo corrisponde al filtro.
admin-no-groups = Ancora nessun gruppo sincronizzato. I gruppi vengono sincronizzati automaticamente dal tuo provider OIDC.
admin-auth-settings = Impostazioni di accesso
admin-registration-enabled = Registrazione attiva
admin-allowed-domains = Domini e-mail consentiti
admin-allowed-domains-hint = (separati da virgole, vuoto per consentirli tutti)
admin-save-auth = Salva le impostazioni di accesso
admin-system-settings = Impostazioni di sistema
admin-base-url = URL di base
admin-base-url-help = URL pubblico di questa istanza. Viene usato per i reindirizzamenti OIDC e per i link nelle e-mail (approva/rifiuta, annulla, promemoria).
admin-private-hosts = Elenco di host privati consentiti
admin-private-hosts-help = Nomi host, separati da virgole, a cui è consentito risolvere su IP privati o riservati per le origini CalDAV/EWS (deroga alla protezione SSRF). Aggiungi solo host che controlli (per esempio un server calendario sulla stessa rete Docker). Lascia vuoto per mantenere la protezione attiva su tutti gli host.
admin-unset-env = Rimuovi la variabile d'ambiente per poter modificare questo valore da qui.
admin-save-system = Salva le impostazioni di sistema
admin-status = Stato:
admin-status-enabled = attivo
admin-status-disabled = disattivato
admin-status-disabled-paren = (disattivato)
admin-status-configured = configurato
admin-status-not-configured = non configurato
admin-via-environment = (tramite l'ambiente)
admin-issuer = Emittente:
admin-client-id = ID client:
admin-instance = Istanza:
admin-oidc-settings = Impostazioni OIDC
admin-oidc-enabled = OIDC attivo
admin-issuer-url = URL dell'emittente
admin-client-id-label = ID client
admin-client-secret = Client secret
admin-keep-current-hint = (lascia vuoto per mantenere quello attuale)
admin-keep-current-set-hint = (lascia vuoto per mantenere quello attuale: ne è già impostato uno)
admin-keep-unchanged = Lascia vuoto per non modificarlo
admin-oidc-auto-register = Registra automaticamente i nuovi utenti da OIDC
admin-save-oidc = Salva le impostazioni OIDC
admin-google = Google Calendar (OAuth2)
admin-save-google = Salva le impostazioni OAuth2 di Google
admin-captcha = Captcha
admin-instance-url = URL dell'istanza
admin-site-key = Chiave del sito
admin-secret = Secret
admin-widget-url = URL dello script del widget
admin-widget-url-help = Cambialo se il CDN è bloccato. Le modifiche hanno effetto subito dopo il salvataggio.
admin-captcha-disable-help = Lascia vuoti URL dell'istanza, chiave del sito e secret per disattivare il captcha sulle pagine di prenotazione.
admin-save-captcha = Salva le impostazioni del captcha
admin-resources = Risorse
admin-resources-help = Risorse condivise prenotabili (laboratorio demo, sale riunioni) basate su un feed di calendario. Collegate ai tipi di evento, una risorsa occupata blocca le prenotazioni.
admin-resource-stats = Eventi in cache: { $events } &middot; Collegata a { $attached } tipo/i di evento
admin-never = mai
admin-resource-sync-failed = (ultimo tentativo non riuscito: { $error })
admin-writeback-enabled = Scrittura: attiva ({ $via })
admin-writeback-readonly = Scrittura: sola lettura
admin-teams-allowed = Team autorizzati:
admin-teams-allowed-none = nessuno (solo amministratori globali)
admin-sync-now = Sincronizza ora
admin-test-write = Prova la scrittura
admin-delete-resource-confirm = Eliminare questa risorsa? I tipi di evento che la usano smetteranno di controllarla.
admin-name = Nome
admin-name-help = Lascia vuoto per prendere il nome dal feed.
admin-feed-url = URL del feed ICS (indirizzo di pubblicazione)
admin-feed-url-help = BlueMind: l'indirizzo di calendario pubblico o privato del calendario della risorsa.
admin-caldav-url = URL della collezione CalDAV (per la scrittura)
admin-caldav-url-help = Facoltativo. Con BlueMind viene ricavato automaticamente dall'URL del feed.
admin-caldav-username = Nome utente CalDAV
admin-caldav-password = Password CalDAV
admin-resource-teams = Team autorizzati a usare questa risorsa
admin-resource-teams-help = Gli amministratori di questi team possono collegare la risorsa ai loro tipi di evento di team. Vuoto: solo amministratori globali.
admin-no-teams = Ancora nessun team.
admin-save-resource = Salva la risorsa
admin-add-resource = Aggiungi una risorsa
admin-jitsi = Jitsi (link d'incontro generati automaticamente)
admin-jitsi-help = Quando il luogo di un tipo di evento è «Jitsi (stanza generata automaticamente)», calrs costruisce per ogni prenotazione un URL di stanza nuovo aggiungendo lo schema qui sotto al tuo URL di base Jitsi. Non serve alcuna chiamata a un'API esterna.
admin-display-name = Nome visualizzato
admin-jitsi-display-name-placeholder = per es. Meet DYB
admin-jitsi-display-name-help = Viene mostrato agli ospiti nel selettore degli slot e nel modulo di prenotazione. Se lasciato vuoto, si usa «Videochiamata».
admin-room-pattern = Schema del nome della stanza
admin-jitsi-disable-help = Lascia vuoto l'URL di base per disattivare la generazione automatica di Jitsi.
admin-save-jitsi = Salva le impostazioni di Jitsi
admin-meeting-webhook = Webhook incontri (fornitore personalizzato)
admin-webhook-url = URL del webhook
admin-webhook-display-name-placeholder = per es. Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Viene mostrato agli ospiti al posto dell'etichetta generica «Videochiamata».
admin-authentication = Autenticazione
admin-auth-none = Nessuna
admin-auth-hmac = HMAC-SHA256 (intestazione X-Calrs-Signature)
admin-shared-secret = Secret condiviso
admin-webhook-disable-help = Lascia vuoto l'URL per disattivare il webhook incontri.
admin-save-webhook = Salva le impostazioni del webhook
admin-smtp = Impostazioni SMTP
admin-smtp-test-sent = E-mail di prova inviata.
admin-smtp-test-failed = Non è stato possibile inviare l'e-mail di prova. Controlla i log del server e le tue impostazioni SMTP.
admin-smtp-env-error = Errore nella configurazione SMTP dell'ambiente:
admin-smtp-host = Host:
admin-smtp-from = Mittente:
admin-smtp-enabled = SMTP attivo
admin-host = Host
admin-port = Porta
admin-tls-mode = Modalità TLS
admin-tls-starttls = STARTTLS (porta 587)
admin-tls-implicit = TLS implicito (porta 465)
admin-tls-none = Nessuna, non cifrata (solo MTA locale)
admin-smtp-username-hint = (lascia vuoto per un relay senza autenticazione)
admin-from-email = E-mail del mittente
admin-from-name = Nome del mittente
admin-save-smtp = Salva le impostazioni SMTP
admin-send-test-email = Invia un'e-mail di prova a
admin-send-test-email-hint = (per impostazione predefinita, l'e-mail del tuo account)
admin-send-test-email-btn = Invia l'e-mail di prova
admin-smtp-clear-confirm = Eliminare la configurazione SMTP salvata nel database?
admin-clear-db-config = Cancella la configurazione dal database
admin-sms = Impostazioni SMS
admin-sms-help = Facoltativo. Gli SMS vengono inviati solo per le prenotazioni di tipi di evento con le «Notifiche SMS» attive, e solo se l'ospite ha lasciato un numero di telefono.
admin-sms-test-sent = Messaggio di prova inviato.
admin-sms-test-checked = Credenziali accettate.
admin-sms-test-error = Il gateway SMS ha rifiutato la richiesta.
admin-sms-captcha-warning = Il modulo di prenotazione è pubblico e il numero del destinatario lo indica l'ospite, quindi gli SMS senza captcha sono un relay aperto che qualcun altro può farti pagare. Configura il captcha qui sopra e limita i paesi di destinazione nelle impostazioni del tuo gateway.
admin-sms-sent-today = Inviati oggi:
admin-sms-of-cap = su { $cap }
admin-sms-config-error = Errore nella configurazione SMS:
admin-sms-gateway = Gateway:
admin-sms-account = Account:
admin-sms-sender = Mittente:
admin-sms-enabled = SMS attivi
admin-sms-gateway-label = Gateway
admin-required-on-switch = Obbligatorio quando si cambia gateway
admin-sms-docs = Documentazione dell'API di { $provider }
admin-sms-country = Prefisso internazionale predefinito
admin-sms-country-hint = (usato quando gli ospiti inseriscono un numero locale)
admin-sms-daily-cap = Limite giornaliero
admin-sms-daily-cap-hint = (messaggi al giorno per l'intera istanza, 0 per nessun limite)
admin-sms-daily-cap-help = Superato il limite, calrs smette di inviare SMS e continua a inviare e-mail, così nessuna prenotazione fallisce perché il budget SMS è esaurito.
admin-save-sms = Salva le impostazioni SMS
admin-send-test-sms = Invia un messaggio di prova a
admin-send-test-sms-hint-check = (lascia vuoto per verificare solo le credenziali)
admin-send-test-sms-hint-e164 = (formato E.164)
admin-test-gateway = Prova il gateway
admin-sms-clear-confirm = Eliminare la configurazione SMS salvata nel database?
admin-sms-allow-all = Consenti a ogni utente di attivare gli SMS sui propri tipi di evento
admin-sms-allow-all-help = Disattivato per impostazione predefinita: gli SMS consumano il credito dell'account configurato qui, quindi solo gli amministratori possono mettere un tipo di evento in modalità SMS.
admin-save-policy = Salva la politica
admin-page-of = Pagina %1 di %2
admin-show-more-js = Mostra altri %1
admin-show-fewer = Mostra meno

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Eliminare definitivamente l'utente { $email }?{"\u000A"}{"\u000A"}Verranno rimossi il suo account, il suo profilo di pianificazione, le sue origini calendario, i suoi tipi di evento e tutti i dati di sua esclusiva proprietà. Le prenotazioni passate verranno eliminate insieme ai suoi tipi di evento.{"\u000A"}{"\u000A"}Per gli utenti OIDC/SSO: se la registrazione automatica è attiva, questa persona verrà ricreata al prossimo accesso.{"\u000A"}{"\u000A"}L'operazione non può essere annullata.
admin-system-settings-help = URL pubblico e impostazioni di sicurezza di rete. Si possono definire anche con le variabili d'ambiente <code>CALRS_BASE_URL</code> e <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Quando una variabile d'ambiente è impostata, <strong>ha la precedenza</strong> sul valore qui sotto.
admin-set-by-env = — impostato dall'ambiente ({ $var }), ha la precedenza sul valore salvato
admin-google-help = Per attivare l'integrazione con Google Calendar, crea delle credenziali OAuth2 nella <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Attiva la <strong>Google Calendar API</strong>, poi aggiungi { $redirect_uri } come URI di reindirizzamento autorizzato.
admin-room-pattern-help = Segnaposto disponibili: <code>{"{"}username{"}"}</code> (organizzatore), <code>{"{"}event{"}"}</code> (identificativo del tipo di evento), <code>{"{"}date{"}"}</code> (AAAAMMGG), <code>{"{"}random{"}"}</code> (8 caratteri). Predefinito: { $default }.
admin-room-pattern-warning = Senza <code>{"{"}random{"}"}</code> il nome della stanza è prevedibile: due ospiti che prenotano lo stesso tipo di evento nello stesso giorno finiscono nella stessa stanza e possono vedere l'incontro dell'altro. Le stanze fisse sono ammesse (per esempio una stanza personale per ogni organizzatore), ma attivale solo se sei consapevole del compromesso.
admin-meeting-webhook-help = Quando il luogo di un tipo di evento è «Webhook (fornitore personalizzato)», al momento della conferma calrs invia i dati della prenotazione in POST a questo URL e si aspetta in risposta un corpo JSON <code>{"{"}"url": "https://..."{"}"}</code>.
admin-auth-hmac-help = Con HMAC, calrs invia <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> calcolato sul corpo grezzo della richiesta.
admin-tls-none-warning = Scegli <strong>Nessuna</strong> solo per un relay su questa macchina che non offre STARTTLS, o il cui certificato è autofirmato. La posta, e le eventuali credenziali, viaggeranno in chiaro.
admin-smtp-env-error-help = Correggi le variabili d'ambiente <code>CALRS_SMTP_*</code>, oppure rimuovile per gestire l'SMTP dal database qui.
admin-smtp-env-managed = Gestito tramite <strong>variabili d'ambiente</strong> (hanno la precedenza sul database). Modifica le variabili <code>CALRS_SMTP_*</code>, oppure rimuovile per gestire l'SMTP da qui.
admin-smtp-env-help = In alternativa puoi configurarlo con variabili d'ambiente (che hanno la precedenza su questo): <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> o <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Solo <code>CALRS_SMTP_HOST</code> e <code>CALRS_SMTP_FROM_EMAIL</code> sono obbligatorie; ometti nome utente e password per instradare tramite un MTA locale senza autenticazione.
admin-sms-env-error-help = Correggi le variabili d'ambiente <code>CALRS_SMS_*</code>, oppure rimuovile per gestire gli SMS dal database qui.
admin-sms-env-managed = Gestito tramite <strong>variabili d'ambiente</strong> (hanno la precedenza sul database). Modifica le variabili <code>CALRS_SMS_*</code>, oppure rimuovile per gestire gli SMS da qui.
admin-sms-env-help = In alternativa puoi configurarlo con variabili d'ambiente (che hanno la precedenza su questo): <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>La modalità di prova Twilio è attiva</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Gli ospiti ricevono il modello predefinito di Twilio <code>sms_appointment_reminders</code> invece del messaggio vero, e vengono raggiunti solo i numeri verificati nella tua console Twilio. È un ausilio per provare con account di prova. Rimuovi la variabile prima di accettare prenotazioni.

admin-show-more =
    { $count ->
        [one] Mostra altro { $count }
       *[other] Mostra altri { $count }
    }

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Scegli il protocollo parlato dal tuo server. EWS è pensato per Exchange 2019/2016/2013 installato in sede.

admin-sms-going-live = <strong>Prima di andare in produzione:</strong> limita i paesi di destinazione nel tuo gateway (in Twilio si chiama Geo Permissions), tieni l'account prepagato senza ricarica automatica, e lascia il captcha attivo. Insieme, queste tre misure limitano quanto può costarti un tentativo di SMS pumping.

troubleshoot-heading = Diagnostica della disponibilità

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = Nome e identificativo sono obbligatori.
form-error-team-name-length = Il nome non può superare i 255 caratteri.
form-error-team-description-length = La descrizione non può superare i 5000 caratteri.
form-error-slug-charset = L'identificativo può contenere solo lettere minuscole, cifre e trattini.
form-error-slug-reserved = Questo identificativo è riservato. Per favore, scegline un altro.
form-error-team-slug-taken = Esiste già un team con questo identificativo.
form-error-title-required = Serve un titolo per generare l'identificativo.
form-error-event-type-slug-taken = Esiste già un tipo di evento con questo identificativo.
form-error-event-type-slug-taken-team = In questo team esiste già un tipo di evento con questo identificativo.
form-error-location-required = I dettagli del luogo sono obbligatori (per esempio un link a una videochiamata, un numero di telefono o un indirizzo).
form-error-not-team-admin = Non sei amministratore di questo team.
form-error-no-account = Nessun profilo di pianificazione trovato. Per favore, rivolgiti all'amministrazione.
form-error-all-fields-required = Tutti i campi sono obbligatori.
form-error-encryption = Errore di cifratura.
form-error-connection-failed = Connessione non riuscita: { $error }. Controlla URL e credenziali, oppure seleziona «Salta la prova di connessione» per salvare comunque.

# Settings page flash (src/web/mod.rs)

settings-saved = Impostazioni salvate.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = Il nome deve essere lungo tra 1 e 255 caratteri.
settings-error-username-length = Il nome utente deve essere lungo almeno 2 caratteri.
settings-error-username-taken = Questo nome utente è già in uso.
settings-error-booking-email = Per favore, inserisci un indirizzo e-mail valido per le prenotazioni.
settings-error-save-failed = Non è stato possibile salvare le impostazioni.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = Team non trovato, oppure non ne sei amministratore.
error-team-not-found = Team non trovato.
error-event-type-not-found = Tipo di evento non trovato.
error-decrypt-failed = Non è stato possibile decifrare le credenziali salvate.
error-source-not-found = Origine non trovata.
error-source-no-password = Per questa origine non è salvata alcuna password.
error-oauth-invalid-state = Parametro di stato non valido. Per favore, riprova.
error-oauth-no-code = Nessun codice di autorizzazione ricevuto.
error-oauth-not-configured = Google OAuth2 non è configurato.
error-no-scheduling-account = Nessun profilo di pianificazione trovato.
error-private-event-type-not-found = Tipo di evento privato non trovato.
error-access-denied = Accesso negato.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = Questo slot non è più disponibile.
error-slot-too-soon = Questo slot non è più disponibile (troppo a ridosso).
error-slot-beyond-horizon = Questo slot è oltre la finestra di prenotazione.
error-invite-required = Questo tipo di evento richiede un link di invito.
error-invite-invalid = Link di invito non valido.
error-invite-expired = Questo link di invito è scaduto.
error-invite-used = Questo link di invito è già stato usato.
error-invalid-date = Data non valida.
error-invalid-time = Ora non valida.
error-invalid-date-format = Formato della data non valido.
error-invalid-time-format = Formato dell'ora non valido.
error-too-many-bookings = Troppi tentativi di prenotazione. Per favore, riprova tra qualche minuto.
error-too-many-requests = Troppe richieste. Per favore, riprova più tardi.
error-no-members-available = Nessun membro del team è disponibile per questo slot.
error-dynamic-group-public-only = I link di gruppo dinamici sono disponibili solo per i tipi di evento pubblici.
error-user-not-found = Utente non trovato.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Verifica captcha non riuscita
bae-title-invalid-booking = Dati della prenotazione non validi
bae-title-unavailable = Non disponibile al momento
bae-title-cannot-approve = Impossibile approvare questa prenotazione
bae-title-invalid-link = Link non valido
bae-title-invalid-or-expired = Link non valido o scaduto
bae-title-booking-not-found = Prenotazione non trovata
bae-title-already-approved = Già approvata
bae-title-already-declined = Già rifiutata
bae-title-already-cancelled = Già annullata
bae-title-booking-cancelled = Prenotazione annullata
bae-title-booking-declined = Prenotazione rifiutata

# Booking action error page: bodies

bae-body-go-back = Per favore, torna indietro e riprova.
bae-body-unavailable = L'organizzatore non accetta altre prenotazioni per questa data. Per favore, scegli un'altra data o riprova più tardi.
bae-body-resource-gone = Una risorsa necessaria non è più disponibile a quest'ora. Chiedi all'ospite di scegliere un altro slot.
bae-body-no-claim-token = Nessun token fornito.
bae-body-claim-invalid = Questo link non è più valido.
bae-body-booking-gone = Questa prenotazione non esiste più.
bae-body-decline-link-invalid = Questo link di rifiuto non è valido, è scaduto, oppure la prenotazione è già stata elaborata.
bae-body-cancel-link-invalid = Questo link di annullamento non è valido, è scaduto, oppure la prenotazione è già stata annullata.
bae-body-cancel-link-invalid-short = Questo link di annullamento non è valido o è scaduto.
bae-body-reschedule-link-invalid = Questo link di spostamento non è valido, è scaduto, oppure la prenotazione è già stata elaborata.
bae-body-approval-link-invalid = Questo link di approvazione non è valido o è scaduto.
bae-body-already-approved = Questa prenotazione è già stata approvata.
bae-body-already-declined = Questa prenotazione è già stata rifiutata.
bae-body-already-cancelled = Questa prenotazione è già stata annullata.
bae-body-was-cancelled = Questa prenotazione è stata annullata.
bae-body-declined-by-host = Questa prenotazione è stata rifiutata dall'organizzatore.

# Booking form validation (src/web/mod.rs)

validate-name-length = Il nome deve essere lungo tra 1 e 255 caratteri.
validate-email-length = L'e-mail deve essere lunga tra 1 e 255 caratteri.
validate-email-invalid = Per favore, inserisci un indirizzo e-mail valido.
validate-notes-length = Le note non possono superare i 5000 caratteri.
validate-date-too-far = Non è possibile prenotare con più di un anno di anticipo.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = Questo tipo di evento non ammette ospiti aggiuntivi.
guests-too-many =
    { $max ->
        [one] Puoi aggiungere al massimo un ospite aggiuntivo.
       *[other] Puoi aggiungere al massimo { $max } ospiti aggiuntivi.
    }
guests-invalid-email = E-mail dell'ospite aggiuntivo non valida: { $email }
dynamic-group-min-usernames = I link di gruppo dinamico richiedono almeno due nomi utente.
dynamic-group-user-not-found = Utente «{ $username }» non trovato.
dynamic-group-user-opted-out = L'utente «{ $username }» non ha attivato i link di gruppo dinamico.

error-slot-unavailable-member = Questo slot non è più disponibile ({ $username } ha un conflitto).
