# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Réservation en attente
confirmed-page-title-booked = Réservation confirmée

confirmed-heading-reschedule-requested = Reprogrammation demandée
confirmed-heading-rescheduled = Reprogrammé !
confirmed-heading-pending = En attente de confirmation
confirmed-heading-booked = C'est réservé !

confirmed-subtitle-reschedule-requested = Votre demande de reprogrammation a été envoyée à { $host }. Vous recevrez un e-mail à l'adresse { $email } une fois qu'elle sera approuvée.
confirmed-subtitle-rescheduled = Votre réservation a été reprogrammée. Un e-mail de confirmation a été envoyé à { $email }.
confirmed-subtitle-pending = Votre demande de réservation a été envoyée à { $host }. Vous recevrez un e-mail à l'adresse { $email } une fois qu'elle sera confirmée.
confirmed-subtitle-booked = Un e-mail de confirmation a été envoyé à { $email }.

confirmed-detail-event = Événement :
confirmed-detail-date = Date :
confirmed-detail-time = Heure :
confirmed-detail-with = Avec :
confirmed-detail-location = Lieu :
confirmed-detail-notes = Notes :
confirmed-detail-additional-guests = Invités supplémentaires :

confirmed-book-another = Réserver un autre créneau

confirmed-add-to-calendar = Ajouter à l'agenda

# Slot picker (templates/slots.html)

slots-location-video = Visioconférence
slots-location-phone = Appel téléphonique
slots-location-google-meet = Google Meet

slots-tz-label = Votre fuseau horaire
slots-tz-fixed = Heures affichées dans le fuseau { $tz }
slots-time-format-label = Format de l'heure
tz-group-common = Fuseaux horaires courants
tz-region-africa = Afrique
tz-region-america = Amériques
tz-region-antarctica = Antarctique
tz-region-arctic = Arctique
tz-region-asia = Asie
tz-region-atlantic = Océan Atlantique
tz-region-australia = Australie
tz-region-europe = Europe
tz-region-indian = Océan Indien
tz-region-pacific = Océan Pacifique
slots-offset-note-day = Les horaires du %1 sont en %2 (changement d'heure le %3).
slots-offset-note-range = Changement d'heure le %1 : les horaires sont ensuite en %2.

slots-view-month = Vue mensuelle
slots-view-week = Vue hebdomadaire
slots-view-column = Vue en liste

slots-weekday-mon = Lun
slots-weekday-tue = Mar
slots-weekday-wed = Mer
slots-weekday-thu = Jeu
slots-weekday-fri = Ven
slots-weekday-sat = Sam
slots-weekday-sun = Dim

slots-weekday-mon-short = L
slots-weekday-tue-short = M
slots-weekday-wed-short = M
slots-weekday-thu-short = J
slots-weekday-fri-short = V
slots-weekday-sat-short = S
slots-weekday-sun-short = D

slots-select-date = Choisissez une date
slots-loading-availability = Chargement des disponibilités...
slots-click-highlighted = Cliquez sur une date en surbrillance pour voir les créneaux disponibles
slots-bookable-range-both = Réservable du { $from } au { $until }
slots-bookable-range-from = Réservable à partir du { $from }
slots-bookable-range-until = Réservable jusqu'au { $until }
slots-no-longer-bookable = Cet événement n'est plus réservable.
slots-no-times-month = Aucun créneau disponible ce mois-ci
slots-no-times-day = Aucun créneau disponible ce jour
slots-no-availability-participants = Aucune disponibilité commune trouvée pour tous les participants ce mois-ci
slots-week-more = autres

# Booking form (templates/book.html)

book-page-title = Réserver { $title }
book-back-to-times = Retour aux créneaux
book-name-label = Votre nom
book-name-placeholder = Jeanne Dupont
book-email-label = Adresse e-mail
book-email-placeholder = jeanne@example.com
book-email-invalid = Veuillez saisir une adresse e-mail complète, avec le nom de domaine (par ex. jeanne@example.com).
book-notes-label = Notes
book-notes-optional = (facultatif)
book-notes-placeholder = Y a-t-il des points que vous aimeriez aborder ?
book-additional-guests-label = Invités supplémentaires
book-additional-guests-hint = (facultatif, jusqu'à { $max })
book-add-guest-btn = + Ajouter un invité
book-guest-email-placeholder = collegue@example.com
book-phone-label = Numéro de téléphone
book-phone-placeholder = 06 12 34 56 78
book-phone-help = Les numéros locaux fonctionnent ; sans le + initial, nous supposons { $country }.
book-phone-optional-consequence = Laissez vide si vous préférez ne pas recevoir de SMS au sujet de cette réservation.
book-phone-required = Un numéro de téléphone est obligatoire pour cette réservation.
book-phone-invalid-title = Numéro de téléphone invalide
book-phone-invalid = Veuillez saisir un numéro joignable par SMS, ou laisser le champ vide.
book-phone-country-search = Rechercher
book-phone-country-label = Choisir le pays
book-phone-country-none = Aucun pays sélectionné
book-phone-country-no-results = Aucun pays ne correspond à cette recherche
captcha-label = Vérification de sécurité
captcha-initial-state = Vérifiez que vous êtes humain
captcha-verifying = Vérification en cours...
captcha-solved = Vous êtes humain
captcha-error = Erreur
captcha-troubleshooting = Dépannage
captcha-wasm-disabled = Activez WASM pour une résolution plus rapide
captcha-verify-aria = Cliquez pour vérifier que vous êtes humain
captcha-verifying-aria = Vérification en cours, veuillez patienter
captcha-verified-aria = Vérifié
captcha-required = Veuillez vérifier que vous êtes humain
captcha-error-aria = Une erreur est survenue, veuillez réessayer
book-confirm-button = Confirmer la réservation

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Réservation confirmée : { $event }, le { $date } à { $time } ({ $tz }).
sms-cancelled = Réservation annulée : { $event }, le { $date } à { $time } ({ $tz }).
sms-rescheduled = Réservation déplacée : { $event } aura lieu le { $date } à { $time } ({ $tz }).
sms-reminder = Rappel : { $event } commence le { $date } à { $time } ({ $tz }).

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Invité :
common-detail-reason = Motif :
common-reason-optional = (facultatif)
common-close-page = Vous pouvez fermer cette page.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Annuler la réservation
cancel-heading = Annuler la réservation
cancel-subtitle = Vous êtes sur le point d'annuler votre réservation.
cancel-reason-label = Motif
cancel-reason-placeholder-host = Indiquez à l'organisateur la raison...
cancel-button = Annuler la réservation
cancelled-heading = Réservation annulée
cancelled-subtitle = Votre réservation a été annulée et l'organisateur a été informé.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Refuser la réservation
decline-heading = Refuser la réservation
decline-subtitle = Vous êtes sur le point de refuser cette demande de réservation.
decline-reason-placeholder-guest = Indiquez à l'invité la raison...
decline-button = Refuser la réservation
declined-heading = Réservation refusée
declined-subtitle = La réservation a été refusée et l'invité a été informé.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Approuver la réservation
approve-heading = Approuver la réservation
approve-subtitle = Vous êtes sur le point d'approuver cette demande de réservation.
approve-button = Approuver la réservation
approved-heading = Réservation approuvée
approved-subtitle = La réservation a été confirmée et un e-mail de confirmation a été envoyé à { $email }.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Prendre la réservation
claim-heading = Prendre la réservation
claim-subtitle = Vous êtes sur le point de prendre en charge cette réservation. Vous serez ajouté comme participant.
claim-assigned-to = Attribuée à :
claim-button = Prendre cette réservation
claimed-page-title = Réservation prise en charge
claimed-heading = Réservation prise en charge
claimed-subtitle = Vous avez pris en charge cette réservation. Une invitation a été envoyée à votre adresse e-mail.
already-claimed-page-title = Déjà prise en charge
already-claimed-heading = Déjà prise en charge
already-claimed-subtitle = Cette réservation a déjà été prise en charge par { $name }.

# Generic error page (booking_action_error.html)

action-error-page-title = Erreur d'action sur la réservation

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Reprogrammer la réservation — calrs
host-resched-heading = Reprogrammer la réservation
host-resched-subtitle = Cela enverra à { $guest } un e-mail lui demandant de choisir un nouveau créneau.
host-resched-currently = Actuellement :
host-resched-button = Envoyer la demande de reprogrammation
host-resched-cancel-link = Annuler

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Confirmer la reprogrammation
resched-confirm-heading = Confirmer la reprogrammation
resched-confirm-subtitle = Vous êtes sur le point de déplacer votre réservation à un nouveau créneau.
resched-was = Avant :
resched-new = Maintenant :
resched-button = Confirmer la reprogrammation
resched-back-to-picker = Retour au choix du créneau

# Base layout chrome (templates/base.html)

base-loader-checking = Vérification des disponibilités
base-loader-please-wait = Veuillez patienter, chargement des dernières données de calendrier...
base-stop-impersonating = Arrêter l'usurpation
base-theme-toggle = Changer de thème
base-powered-by = Propulsé par

# Profile (templates/profile.html)

profile-pick-event-type-invite = Choisissez un type d'événement pour réserver un créneau.
profile-no-event-type = Aucun type d'événement disponible pour le moment.

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

common-month-1 = janvier
common-month-2 = février
common-month-3 = mars
common-month-4 = avril
common-month-5 = mai
common-month-6 = juin
common-month-7 = juillet
common-month-8 = août
common-month-9 = septembre
common-month-10 = octobre
common-month-11 = novembre
common-month-12 = décembre
common-month-short-1 = janv.
common-month-short-2 = févr.
common-month-short-3 = mars
common-month-short-4 = avr.
common-month-short-5 = mai
common-month-short-6 = juin
common-month-short-7 = juil.
common-month-short-8 = août
common-month-short-9 = sept.
common-month-short-10 = oct.
common-month-short-11 = nov.
common-month-short-12 = déc.

common-weekday-long-mon = lundi
common-weekday-long-tue = mardi
common-weekday-long-wed = mercredi
common-weekday-long-thu = jeudi
common-weekday-long-fri = vendredi
common-weekday-long-sat = samedi
common-weekday-long-sun = dimanche

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
email-action-reschedule = Reprogrammer
email-action-cancel-booking = Annuler la réservation

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
email-confirm-greeting = Bonjour { $name },
email-confirm-headline = Votre réservation est confirmée !
email-confirm-ics-attached-plain = Une invitation est jointe à cet e-mail.
email-confirm-ics-attached-html = Une invitation est jointe à cet e-mail.
email-confirm-need-to-cancel = Besoin d'annuler ? { $url }

# Email: guest reminder

email-reminder-subject = Rappel : { $event } à { $time }
email-reminder-headline = Votre rendez-vous approche.

# Email: guest cancellation

email-cancel-subject = Annulation : { $event } — { $date }
email-cancel-headline-by-host = Votre réservation a été annulée par { $host }.
email-cancel-headline-by-guest = Votre réservation a été annulée.
email-cancel-ics-attached-plain = Une annulation de calendrier est jointe.
email-cancel-ics-attached-html = Une annulation de calendrier est jointe à cet e-mail.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Note : l'annulation exige un préavis d'au moins { $minutes } minutes.
email-confirm-reschedule-notice = Note : la reprogrammation exige un préavis d'au moins { $minutes } minutes.

# Event type form: cancel/reschedule minimum notice (templates/event_type_form.html)

# Google Meet (templates/event_type_form.html, src/google_meet.rs)
event-type-form-location-google-meet = Google Meet (lien généré automatiquement)
event-type-form-location-google-meet-hint = Un lien Google Meet unique est créé à la confirmation, au nom de l'organisateur assigné. Chaque organisateur (vous, ou chaque membre éligible de l'équipe) doit avoir Google Agenda connecté avec un agenda d'écriture sélectionné.
google-meet-prereq-no-host = Google Meet nécessite un organisateur avec Google Agenda connecté.
google-meet-prereq-no-eligible = Google Meet nécessite au moins un membre de l'équipe éligible avec Google Agenda connecté.
google-meet-prereq-missing = Google Meet exige que chaque organisateur ait Google Agenda connecté avec un agenda d'écriture sélectionné. Il manque encore : { $names }. Connectez-les depuis Tableau de bord → Sources d'agenda.
google-meet-unavailable-title = Google Meet n'est pas disponible
google-meet-dynamic-group-unavailable = L'organisateur doit avoir Google Agenda connecté avec un agenda d'écriture sélectionné.

event-type-form-cancel-notice-label = Préavis minimum pour annuler
event-type-form-reschedule-notice-label = Préavis minimum pour reprogrammer
event-type-form-notice-help = Laissez vide pour ne pas imposer de restriction.
event-type-form-resources-label = Ressources requises
event-type-form-resources-hint = Les créneaux ne sont proposés que si les ressources sélectionnées sont disponibles, selon le mode ci-dessous.
event-type-form-resources-mode-all = Toutes les ressources sélectionnées doivent être libres
event-type-form-resources-mode-round-robin = Une seule ressource libre suffit (elle est attribuée à la réservation)
event-type-form-notice-unit-minutes = minutes
event-type-form-notice-unit-hours = heures
event-type-form-notice-unit-days = jours
event-type-form-booking-horizon-label = Horizon de réservation
event-type-form-booking-horizon-help = Nombre de jours à l'avance pendant lesquels les invités peuvent réserver. Laissez vide pour aucune limite, 0 pour aujourd'hui seulement.
event-type-form-bookable-range-label = Période réservable
event-type-form-bookable-from = du
event-type-form-bookable-until = au
event-type-form-bookable-range-help = Seuls ces jours peuvent être réservés. Laissez vide pour aucune limite. Utilisez les exceptions pour d'autres horaires certains jours.
event-type-form-bookable-range-past-warning = Cette période est déjà terminée. Le type d'événement n'est plus affiché publiquement.
event-type-form-bookable-range-too-far-warning = Cette période commence dans plus d'un an. Les invités peuvent réserver au plus un an à l'avance, les réservations ouvrent donc le { $date }.
event-type-form-bookable-range-horizon-warning = L'horizon de réservation se termine avant le début de cette période. Les invités peuvent réserver à partir du { $date }.
event-type-form-bookable-range-bookings-outside =
    { $count ->
        [one] 1 réservation existante tombe en dehors de la période réservable.
       *[other] { $count } réservations existantes tombent en dehors de la période réservable.
    }
event-type-form-bookable-range-bookings-link = Voir les réservations
event-type-form-saved = Enregistré.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = L'annulation exige un préavis d'au moins { $minutes } minutes avant la réunion.
confirmed-reschedule-notice-info = La reprogrammation exige un préavis d'au moins { $minutes } minutes avant la réunion.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Cette réservation ne peut plus être annulée en ligne
booking-blocked-title-reschedule = Cette réservation ne peut plus être reprogrammée en ligne
booking-blocked-body = L'hôte exige un préavis d'au moins { $minutes } minutes. Si vous ne pouvez pas être présent, écrivez directement à <a href="mailto:{ $host_email }">{ $host_email }</a>.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Copier
dashboard-event-types-copied = Copié !
dashboard-event-types-copy-title = Copier le lien de réservation
dashboard-event-types-copy-failed = Échec de la copie

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Planification
nav-overview = Vue d'ensemble
nav-event-types = Types d'événement
nav-bookings = Réservations
nav-teams = Équipes
nav-section-shared-links = Liens partagés
nav-invite-links = Liens d'invitation
nav-section-calendars = Agendas
nav-sources = Sources
nav-section-personal = Personnel
nav-settings = Profil et paramètres
nav-troubleshoot = Diagnostic
nav-section-admin = Administration
nav-admin-panel = Panneau d'administration
nav-sign-out = Se déconnecter
nav-release-notes = Voir les notes de version
dash-skip-to-content = Aller au contenu
dash-open-menu = Ouvrir le menu
dash-shortcuts-title = Raccourcis clavier
dash-shortcut-search = Rechercher dans la page
dash-shortcut-new = Créer
dash-shortcut-save = Enregistrer le formulaire
dash-shortcut-close = Fermer les menus et les fenêtres
dash-shortcut-help = Afficher cette aide
dash-shortcut-go = Aller à : { $page }
dash-shortcut-then = puis
dash-close = Fermer
dash-copied = Lien copié dans le presse-papiers
dash-copy-manual = Copiez ce lien :
dash-confirm-again = Cliquez à nouveau pour confirmer
dash-unsaved = Modifications non enregistrées
dash-no-matches = Aucun résultat pour cette recherche.
dash-more-actions = Plus d'actions

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = Le fuseau horaire de votre navigateur est { $detected } alors que votre fuseau de réservation est { $current }.
tz-banner-update = Mettre à jour
tz-banner-dismiss = Ignorer

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = Saisissez l'URL :
editor-link-default-label = texte du lien
editor-placeholder-text = texte
editor-nothing-to-preview = Rien à prévisualiser

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Tableau de bord
overview-avail-banner-body = Vos horaires de travail par défaut ont été fixés du lundi au vendredi, de 9h00 à 17h00. Ils sont utilisés lorsque d'autres personnes vous incluent dans une réunion de groupe dynamique.
overview-avail-banner-cta = Vérifier vos disponibilités
overview-dismiss = Ignorer
overview-getting-started = Pour commencer
overview-step-connect-calendar = Connecter un agenda
overview-step-first-event-type = Créer votre premier type d'événement
overview-step-share-link = Partager votre lien de réservation
overview-pending-approval = En attente d'approbation
overview-confirm = Confirmer
overview-decline = Refuser
overview-copy-link = Copier le lien de réservation
overview-duration-h = { $hours } h
overview-duration-m = { $minutes } min
overview-duration-hm = { $hours } h { $minutes }
overview-percent = { $value } %
overview-chart-title = Rendez-vous par semaine
overview-chart-this-week = Cette semaine
overview-chart-week-of = Semaine du { $date }
overview-series-confirmed = Confirmés
overview-series-pending = En attente
overview-series-cancelled = Annulés ou refusés
overview-view-data = Voir sous forme de tableau
overview-table-week = Semaine
overview-types-title = Par type de rendez-vous
overview-types-other = Autres types
overview-agenda-title = À venir
overview-agenda-today = Aujourd’hui
overview-agenda-tomorrow = Demain
overview-agenda-now = En cours
overview-agenda-join = Rejoindre
overview-agenda-empty = Aucun rendez-vous dans les 14 prochains jours. Partagez votre lien de réservation pour en recevoir.
overview-agenda-all = Toutes les réservations
nav-main-label = Navigation principale
dash-shortcuts-short = Raccourcis
dash-hint-to-decide = { $count } à décider
dash-hint-failing = { $count } en échec
common-format-day-date = { $weekday } { $day } { $month }
common-format-short-date = { $day } { $month }
bookings-summary = À venir : { $upcoming }. En attente de votre décision : { $pending }. Heures en { $tz }.
sources-state-failing = En échec
troubleshoot-previous-day = Jour précédent
troubleshoot-next-day = Jour suivant
overview-new-meeting-type = Nouveau type d'événement
overview-step-connect = Connecter
overview-step-create = Créer
overview-links-title = Vos liens de réservation
overview-open-calendar = Ouvrir votre agenda
overview-week-title = Cette semaine
overview-week-closed = fermé
overview-week-total = { $booked } réservées
overview-week-number = Semaine { $week }
overview-week-short = S{ $week }
overview-time-range = de { $from } à { $to }
overview-until = jusqu'à { $time }
overview-expires = Échéance { $when }
overview-guest-and-more = { $name } (+{ $count })
overview-mast-now = En cours : { $title } avec { $guest }, jusqu'à { $time }
overview-mast-next = Ensuite : { $title } avec { $guest } à { $time }
overview-mast-next-day = Ensuite : { $title } avec { $guest }, { $day } à { $time }
overview-ruler-booked = Réservé
overview-ruler-pending = En attente de décision
overview-ruler-unavailable = Indisponible (votre agenda)
overview-key-open = Ouvert aux réservations
overview-ruler-aria = Aujourd'hui, de { $from } à { $to }
overview-chart-now = en cours
overview-figures-title = 30 derniers jours
overview-fig-booked = Rendez-vous réservés
overview-fig-time = Temps en rendez-vous
overview-fig-cancelled = Annulés
overview-fig-notice = Délai moyen
overview-fig-before = { $value } les 30 jours précédents
overview-fig-prev-key = Les 30 jours précédents
overview-fig-days =
    { $count ->
        [one] { $count } jour
       *[other] { $count } jours
    }

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Réservations
bookings-pending-approval = En attente d'approbation
bookings-available-to-claim = À prendre en charge
bookings-upcoming = Réservations à venir
bookings-guest-booked = Réservé par l'invité :
bookings-resource = Ressource :
bookings-confirm = Confirmer
bookings-reschedule = Reprogrammer
bookings-decline = Refuser
bookings-claim = Prendre en charge
bookings-badge-awaiting-reschedule = reprogrammation en attente
bookings-badge-writeback-failed = absent de votre agenda
bookings-writeback-failed-detail = Échec de l'écriture dans l'agenda : { $error }
bookings-cancel = Annuler
bookings-reason-placeholder = Motif (facultatif)
bookings-confirm-cancel = Confirmer l'annulation
bookings-back = Retour
bookings-empty = Aucune réservation à venir pour le moment.<br>Partagez vos { $link } pour que l'on puisse réserver avec vous.
bookings-empty-link-label = liens de types d'événement
bookings-search-placeholder = Rechercher un invité, un e-mail ou un rendez-vous
bookings-recent = 30 derniers jours
bookings-cancelled = Annulées et refusées

# Host notes on a booking (templates/booking_notes.html, bookings page, overview, host emails)

host-notes-title = Notes des hôtes
host-notes-hint = Visibles par les hôtes de cette réservation, par un membre d'équipe qui l'a prise en charge et par un administrateur qui agit en tant que l'un de ses hôtes. Jamais montrées à l'invité, ni ajoutées à l'invitation ou à l'événement du calendrier.
host-notes-placeholder = Par exemple : ce qui a été dit lors d'un appel, ce qu'il faut préparer.
host-notes-save = Enregistrer les notes
host-notes-saved = Notes enregistrées.
host-notes-too-long = Les notes des hôtes ne peuvent pas dépasser { $max } caractères.
host-notes-conflict = Ces notes ont été modifiées après l'ouverture de cette page. La version enregistrée figure ci-dessous ; enregistrer à nouveau la remplace par le texte du champ.
host-notes-current = Version enregistrée :
host-notes-last-edit = Dernière modification par { $name }, { $when }
host-notes-last-edit-anon = Dernière modification : { $when }
host-notes-edit = Modifier la note
host-notes-marker = Note des hôtes
host-notes-cleared = (effacées)
host-notes-last-edit-as = Dernière modification par { $name } (administrateur, en tant que { $host }), { $when }
dash-not-found-title = Page introuvable
dash-not-found-body = Cette page n'existe pas, ou ce compte n'y a pas accès.
host-notes-from-guest = De l'invité :
bookings-host-notes = Notes des hôtes :
email-detail-host-notes = Notes des hôtes :

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Équipes
teams-heading = Équipes
teams-new = Nouvelle
teams-badge-public = publique
teams-badge-private = privée
teams-settings = Paramètres
teams-view = Voir
teams-empty = Aucune équipe pour le moment.
teams-empty-admin = { $link } pour collaborer avec votre équipe.
teams-empty-admin-link-label = Créez-en une
teams-empty-member = Les équipes sont créées par les administrateurs. Demandez au vôtre d'en créer une et de vous y ajouter.
teams-subtitle = Des personnes qui partagent des rendez-vous, attribués à tour de rôle ou en commun.

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Liens d'invitation
invite-links-heading = Liens d'invitation
invite-links-new = Nouvel événement interne
invite-links-help = Générez des liens de réservation à usage unique pour les types d'événement internes. Tout collègue authentifié peut créer et partager des liens ici.
invite-links-duration = { $minutes } min
invite-links-hosted-by = Organisé par { $host }
invite-links-get-link = Obtenir un lien
invite-links-invites = Invitations
invite-links-empty = Aucun type d'événement interne pour le moment.<br>{ $link } avec la visibilité « Interne » pour permettre à tout collègue de générer des liens de réservation.
invite-links-empty-link-label = Créez un type d'événement
invite-links-js-generating = Génération...
invite-links-js-copied = Copié !
invite-links-js-error = Erreur

teams-member-count =
    { $count ->
        [one] { $count } membre
       *[other] { $count } membres
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Sources d'agenda
sources-heading = Sources d'agenda
sources-add = Ajouter
sources-last-sync = Dernière synchronisation :
sources-sync-failing = Synchronisation en échec depuis le { $since } UTC : { $error }
sources-sync = Synchroniser
sources-full-resync = Resynchronisation complète
sources-full-resync-title = Vider le cache et récupérer tous les événements depuis le serveur
sources-test = Tester
sources-reconnect = Reconnecter
sources-reconnect-title = Relancer le processus de consentement Google
sources-edit = Modifier
sources-remove = Supprimer
sources-remove-confirm = Supprimer la source « { $name } » ? Tous les événements synchronisés depuis cette source seront effacés.
sources-no-write-calendar = Aucun agenda d'écriture sélectionné. Les réservations confirmées restent dans calrs et ne sont pas envoyées vers cet agenda. Choisissez-en un ci-dessous pour activer l'écriture.
sources-write-calendar-read-only = L'agenda dans lequel les réservations sont écrites est désormais en lecture seule : elles ne peuvent plus y arriver. Choisissez ci-dessous un agenda modifiable.
sources-write-calendar-refused = Cet agenda ne peut pas recevoir de réservations : ce n'est pas un agenda de cette source, ou le compte ne peut pas y écrire. Choisissez-en un autre.
sources-microsoft-not-ready = calrs n'a pas encore fini de lire cet agenda. D'ici là, vous n'affichez aucun créneau libre sur vos pages de réservation. Cela prend jusqu'à environ une minute après la connexion ; si cela dure plus longtemps, l'erreur de synchronisation ci-dessus en donne la raison.
sources-write-bookings-to = Écrire les réservations dans :
sources-write-none = Aucun (ne pas écrire)
sources-empty = Aucune source d'agenda connectée. { $link } pour vérifier les disponibilités.
sources-empty-link-label = Ajoutez-en une
sources-subtitle = Les agendas que calrs lit pour savoir quand vous êtes occupé, et où il écrit les nouvelles réservations.
sources-never-synced = Jamais synchronisé
sources-calendars = Agendas
sources-calendars-synced = Agendas synchronisés : { $synced } sur { $total }
source-calendars-page-title = Agendas à synchroniser
source-calendars-heading = Agendas à synchroniser
source-calendars-back = Retour aux sources d'agenda
source-calendars-help = Choisissez les agendas de { $name } que calrs synchronise. Les agendas décochés ne sont jamais récupérés et ne bloquent pas vos disponibilités.
source-calendars-select-all = Tout sélectionner
source-calendars-select-none = Tout désélectionner
source-calendars-write-badge = Réservations écrites ici
source-calendars-write-hint = Les réservations confirmées sont écrites dans cet agenda, qu'il soit synchronisé ou non.
source-calendars-save = Enregistrer
source-calendars-saved = Enregistré. Les agendas nouvellement cochés seront récupérés à la prochaine synchronisation ; utilisez Synchroniser sur la page des sources pour les récupérer dès maintenant.
source-calendars-empty = Aucun agenda trouvé pour cette source pour l'instant. Lancez une synchronisation pour les découvrir.

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Types d'événement
event-types-heading = Types d'événement
event-types-new = Nouveau
event-types-badge-disabled = désactivé
event-types-badge-internal = interne
event-types-badge-private = privé
event-types-badge-resources = ressources
event-types-badge-active = actif
event-types-badge-expired = expiré
event-types-badge-range-from = à partir du { $date }
event-types-badge-range-until = jusqu'au { $date }
event-types-send-invites = Envoyer des invitations
event-types-duration = { $minutes } min
event-types-mode-collective = collectif
event-types-mode-round-robin = tour de rôle
event-types-edit = Modifier
event-types-disable = Désactiver
event-types-enable = Activer
event-types-embed = Intégrer
event-types-overrides = Exceptions
event-types-team-settings = Paramètres de l'équipe
event-types-invites = Invitations
event-types-view-public = Voir la page publique
event-types-view-page = Voir la page
event-types-delete = Supprimer
event-types-delete-confirm = Supprimer le type d'événement « { $title } » ? Cette action est irréversible.
event-types-duplicate = Dupliquer
event-types-duplicate-title = { $title } (copie)
event-types-empty = Aucun type d'événement pour le moment. { $link } pour commencer à accepter des réservations.
event-types-empty-link-label = Créez-en un
event-types-subtitle = Les rendez-vous que l'on peut réserver avec vous. Copiez un lien ou intégrez-le à votre site.
event-types-search-placeholder = Filtrer par titre ou lien
event-types-switch-label = Réservations ouvertes

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Gras (Ctrl+B)
editor-italic = Italique (Ctrl+I)
editor-strikethrough = Barré
editor-code = Code en ligne
editor-link = Insérer un lien (Ctrl+K)
editor-toggle-preview = Afficher ou masquer l'aperçu
editor-preview = Aperçu

# Profile and settings (templates/settings.html)

settings-page-title = Paramètres
settings-heading = Profil et paramètres
settings-subtitle = Votre profil public, votre langue, votre fuseau horaire et vos horaires par défaut.
settings-public-page-label = Votre page de réservation publique
settings-copy = Copier
settings-copied = Copié !
settings-open = Ouvrir
settings-avatar = Avatar
settings-upload = Téléverser
settings-remove = Supprimer
settings-display-name = Nom affiché
settings-display-name-placeholder = Votre nom
settings-username = Nom d'utilisateur
settings-username-hint = (utilisé dans votre URL de réservation)
settings-username-pattern-title = Minuscules, chiffres et tirets uniquement
settings-username-help = Votre page de réservation publique :
settings-title = Fonction
settings-title-placeholder = ex. Ingénieure logiciel, Chef de produit
settings-title-help = Affichée sur votre profil public et dans la barre latérale.
settings-bio = Biographie
settings-bio-placeholder = Présentez-vous en quelques mots...
settings-bio-help = Affichée sur votre page de réservation publique. Prend en charge **gras**, *italique*, ~~barré~~, `code` et [liens](url).
settings-booking-email = E-mail de réservation
settings-booking-email-help = Cette adresse apparaîtra sur vos pages de réservation publiques et dans les notifications. Laissez vide pour utiliser votre adresse de connexion.
settings-booking-email-warning = Assurez-vous que cette adresse existe chez votre fournisseur de messagerie. Sinon, les notifications ne seront pas remises.
settings-timezone = Fuseau horaire
settings-timezone-help = Vos règles de disponibilité et vos horaires de réservation sont calculés dans ce fuseau horaire.
settings-language = Langue
settings-language-auto = Auto (langue du navigateur)
settings-language-help = Choisissez une langue d'interface, ou laissez sur Auto pour suivre le réglage de votre navigateur.
settings-public-page-language = Langue des pages de réservation publiques
settings-public-page-language-visitor = La langue de chaque visiteur
settings-public-page-language-host = Toujours ma langue
settings-public-page-language-help = Langue de votre profil public et de vos pages de réservation, et des e-mails reçus par vos invités. « Toujours ma langue » utilise la langue choisie ci-dessus.
settings-error-public-page-language = Choisissez une langue valide pour les pages de réservation publiques.
settings-dynamic-group = Autoriser les autres à m'inclure dans les liens de groupe dynamiques
settings-dynamic-group-help = Une fois activé, d'autres utilisateurs peuvent créer des URL de réunion collective ponctuelles qui vous incluent (ex. { $example }).
settings-lend-resource = Prêter mon accès agenda pour les réservations de ressources
settings-lend-resource-help = Lorsqu'une réservation doit réserver une ressource partagée (laboratoire de démo, salle de réunion) accessible en écriture par votre compte agenda, autoriser calrs à utiliser vos identifiants enregistrés pour cette écriture.
settings-default-availability = Disponibilité par défaut
settings-default-availability-help = Vos horaires de travail par défaut. Utilisés pour les liens de groupe dynamiques lorsque d'autres vous incluent dans une réunion.
settings-copy-to-all = Copier sur tous les jours
settings-copy-to-all-title = Copier les plages du premier jour activé vers tous les autres jours activés
settings-add-window = Ajouter une plage horaire
settings-remove-window = Supprimer la plage
settings-save = Enregistrer les paramètres
settings-appearance = Apparence
settings-theme-system = Système
settings-theme-light = Clair
settings-theme-dark = Sombre

# Sign in (templates/auth/login.html)

login-page-title = Connexion
login-heading = Connexion
login-subtitle = Connectez-vous à votre compte calrs
login-sso = Se connecter avec le SSO
login-or = ou
login-email = E-mail
login-password = Mot de passe
login-submit = Se connecter par e-mail
login-no-account = Vous n'avez pas de compte ? { $link }
login-register-link = Inscrivez-vous

# Registration (templates/auth/register.html)

register-page-title = Inscription
register-heading = Créer un compte
register-subtitle = Créez un nouveau compte calrs
register-domains-limited = L'inscription est réservée à : { $domains }
register-name = Nom
register-name-placeholder = Votre nom
register-email = E-mail
register-password = Mot de passe
register-password-hint = (12 caractères minimum)
register-submit = Créer un compte
register-have-account = Vous avez déjà un compte ? { $link }
register-signin-link = Connectez-vous

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Trop de tentatives de connexion. Veuillez réessayer plus tard.
auth-error-invalid-credentials = Adresse e-mail ou mot de passe incorrect
auth-error-internal = Erreur interne
auth-error-registration-disabled = Les inscriptions sont désactivées.
auth-error-name-length = Le nom doit contenir entre 1 et 255 caractères
auth-error-email-length = L'adresse e-mail doit contenir entre 1 et 255 caractères
auth-error-email-invalid = Veuillez saisir une adresse e-mail valide
auth-error-email-domain = Domaine de messagerie non autorisé
auth-error-password-length = Le mot de passe doit contenir au moins 12 caractères
auth-error-email-taken = Cette adresse e-mail est déjà utilisée
auth-error-create-failed = Échec de la création du compte

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Source d'agenda
source-test-sync-heading = Synchronisation : { $name }
source-test-heading = Test de connexion
source-write-page-title = Configurer l'écriture dans l'agenda
source-write-back = Retour au tableau de bord
source-write-heading = Où enregistrer les réservations ?
source-write-help = Lorsqu'une personne réserve une réunion avec vous, calrs peut créer automatiquement l'événement dans votre agenda. Choisissez l'agenda dans lequel écrire les réservations pour { $name }.
source-write-save = Enregistrer
source-write-skip = Passer pour l'instant
source-write-sync-results = Résultats de la synchronisation

source-write-event-count =
    { $count ->
        [one] { $count } événement
       *[other] { $count } événements
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Exceptions de date
overrides-heading = Exceptions de date
overrides-back-teams = Retour aux équipes
overrides-back-event-types = Retour aux types d'événement
overrides-intro = Ajoutez des exceptions à des dates précises pour { $title }
overrides-add-heading = Ajouter une exception
overrides-date = Date
overrides-type = Type d'exception
overrides-type-blocked = Bloquer toute la journée
overrides-type-custom = Horaires personnalisés
overrides-start-time = Heure de début
overrides-end-time = Heure de fin
overrides-add-submit = Ajouter l'exception
overrides-existing = Exceptions existantes
overrides-badge-blocked = bloquée
overrides-badge-custom = horaires personnalisés
overrides-delete = Supprimer
overrides-delete-confirm = Supprimer cette exception ?
overrides-empty = Aucune exception de date pour le moment.<br>Utilisez le formulaire ci-dessus pour bloquer des dates précises (jours fériés, congés) ou définir des horaires personnalisés.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Choisissez un type d'événement pour réserver un créneau.
team-profile-empty = Aucun type d'événement disponible pour le moment.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Diagnostic
troubleshoot-empty = Aucun type d'événement trouvé. { $link } pour commencer à diagnostiquer vos disponibilités.
troubleshoot-empty-link-label = Créez-en un
troubleshoot-subtitle = Comprenez pourquoi les créneaux sont disponibles ou bloqués pour { $title }
troubleshoot-duration = { $minutes } min
troubleshoot-buffer-before = { $minutes } min de marge avant
troubleshoot-buffer-after = { $minutes } min de marge après
troubleshoot-min-notice = { $minutes } min de préavis
troubleshoot-blocked-override = Bloqué par une exception de date (jour de congé)
troubleshoot-custom-hours-active = Exception d'horaires personnalisés active (remplace les règles hebdomadaires)
troubleshoot-microsoft-not-ready = Votre calendrier Microsoft n'a pas terminé sa première synchronisation : toutes les heures apparaissent occupées d'ici là. Voir Sources d'agenda.
troubleshoot-legend-available = Disponible
troubleshoot-legend-calendar-event = Événement d'agenda
troubleshoot-legend-booking = Réservation
troubleshoot-legend-resource = Ressource occupée
troubleshoot-legend-outside = Hors horaires
troubleshoot-legend-buffer = Marge / préavis minimum
troubleshoot-blocked-slots = Créneaux bloqués
troubleshoot-none-date-blocked = Cette date est bloquée par une exception de disponibilité (jour de congé). Aucun créneau disponible.
troubleshoot-none-custom-hours = Exception d'horaires personnalisés active, mais aucune plage correspondante. Vérifiez le réglage de l'exception.
troubleshoot-none-no-rules = Aucune règle de disponibilité pour ce jour de la semaine. Ce type d'événement n'est pas réservable le { $date }.
troubleshoot-none-all-bookable = Aucun créneau bloqué pendant les horaires de disponibilité. Tous les horaires sont réservables.
troubleshoot-label-outside = Hors disponibilité
troubleshoot-label-available = Disponible
troubleshoot-label-min-notice = Préavis minimum ({ $minutes } min)
troubleshoot-label-beyond-horizon = Au-delà de l'horizon de réservation ({ $days } jours)
troubleshoot-label-outside-date-range = En dehors de la période réservable
troubleshoot-label-buffer = Marge ({ $minutes } min)
troubleshoot-label-resource-busy = Ressource occupée : { $names }
troubleshoot-detail-around = Autour de : { $label }
troubleshoot-detail-around-booking = Autour de la réservation de { $guest }
troubleshoot-reason-calendar-event = Événement d'agenda : { $label }
troubleshoot-reason-booking = Réservation : { $label }

# Invite management (templates/invite_form.html)

invites-heading = Invitations
invites-back-teams = Retour aux équipes
invites-back-event-types = Retour aux types d'événement
invites-intro = Envoyez des liens d'invitation pour { $title }
invites-capped = <strong>La saisie a été limitée à { $max } destinataires par envoi.</strong> Envoyez le reste dans un autre lot.
invites-failed-hint = — consultez les journaux du serveur pour en savoir plus.
invites-quick-link = Lien rapide
invites-quick-link-help = Générez un lien à usage unique et copiez-le dans votre presse-papiers.
invites-get-link = Obtenir un lien
invites-or-email = Ou envoyer par e-mail
invites-recipients = Destinataires
invites-recipients-hint = (une adresse par ligne, { $max } au maximum)
invites-message = Message personnel
invites-message-hint = (facultatif, envoyé à chaque destinataire)
invites-message-placeholder = Au plaisir de vous présenter une démo...
invites-expires-in = Expire dans
invites-expires-days = { $days } jours
invites-expires-never = Jamais
invites-allow-multiple = Autoriser plusieurs réservations par destinataire
invites-send = Envoyer les invitations
invites-sent-heading = Invitations envoyées
invites-badge-expired = expirée
invites-badge-used = utilisée
invites-badge-active = active
invites-sent-by = Envoyée par { $name }
invites-uses = { $used }/{ $max } utilisations
invites-expires-at = Expire le { $date }
invites-copy-link = Copier le lien
invites-delete = Supprimer
invites-delete-confirm = Supprimer cette invitation ?
invites-empty = Aucune invitation envoyée pour le moment. Utilisez le formulaire ci-dessus pour envoyer un lien de réservation.
invites-js-generating = Génération...
invites-js-copied = Copié !
invites-js-error = Erreur

invites-sent-count =
    { $count ->
        [one] { $count } invitation envoyée.
       *[other] { $count } invitations envoyées.
    }

invites-skipped-invalid =
    { $count ->
        [one] { $count } ligne invalide ignorée :
       *[other] { $count } lignes invalides ignorées :
    }

invites-skipped-duplicate =
    { $count ->
        [one] { $count } ligne en double ignorée :
       *[other] { $count } lignes en double ignorées :
    }

invites-failed =
    { $count ->
        [one] { $count } invitation en échec (BDD ou SMTP) :
       *[other] { $count } invitations en échec (BDD ou SMTP) :
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Modifier la source d'agenda
source-form-title-add = Ajouter un agenda
source-form-heading-edit = Modifier la source d'agenda
source-form-heading-add = Connecter un agenda
source-form-subtitle-edit = Mettez à jour la connexion. Laissez le mot de passe vide pour conserver l'actuel. Après avoir changé l'URL ou le nom d'utilisateur, lancez une synchronisation pour rafraîchir la liste des agendas détectés.
source-form-subtitle-add = Connectez un serveur CalDAV ou Microsoft Exchange (EWS) pour que calrs puisse vérifier vos disponibilités lors des réservations.
source-form-backend = Backend
source-form-preset = Préréglage
source-form-connect-google = Se connecter avec Google
source-form-google-unavailable = Google Agenda n'est pas disponible. Contactez votre administrateur.
source-form-name = Nom affiché
source-form-name-placeholder = Mon agenda
source-form-url-caldav = URL CalDAV
source-form-url-ews = URL du point de terminaison EWS
source-form-username = Nom d'utilisateur
source-form-password = Mot de passe
source-form-password-keep = Laissez vide pour conserver l'actuel
source-form-password-placeholder = Mot de passe d'application ou du compte
source-form-skip-test = Ignorer le test de connexion
source-form-skip-test-help = À utiliser si le test se bloque (fréquent sur certaines installations BlueMind/Zimbra). Vous pourrez tester la connexion plus tard.
source-form-save = Enregistrer les modifications
source-form-add = Ajouter la source d'agenda
source-form-help-google-configured = Cliquez sur le bouton ci-dessous pour autoriser calrs à accéder à votre Google Agenda.
source-form-help-google-unconfigured = L'intégration Google Agenda n'est pas encore configurée. Demandez à votre administrateur de renseigner les identifiants OAuth2 Google dans le panneau d'administration.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — Utilisez le point de terminaison DAV de votre serveur BlueMind.<br> En général : <code>https://mail.yourcompany.com/dav/</code><br> Le nom d'utilisateur est votre <strong>adresse e-mail</strong> (ex. <code>alice@yourcompany.com</code>), pas seulement l'identifiant.<br> Si le test de connexion se bloque, cochez « Ignorer le test de connexion » et lancez directement une synchronisation.
source-form-help-nextcloud = <strong>Nextcloud</strong> — Utilisez la racine WebDAV, pas l'URL d'un agenda précis.<br> En général : <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — Indiquez votre adresse complète dans le chemin de l'URL.<br> Exemple : <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Utilisez un mot de passe d'application (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — Utilisez <code>https://caldav.icloud.com/</code><br> Un mot de passe d'application est nécessaire, à créer sur <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Sécurité &rarr; Mots de passe pour application).
source-form-help-zimbra = <strong>Zimbra</strong> — Utilisez le point de terminaison DAV de votre serveur Zimbra.<br> En général : <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — Utilisez le point de terminaison DAV de SOGo.<br> En général : <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — Utilisez l'URL racine du serveur.<br> En général : <code>https://cal.example.com/</code>
source-form-help-baikal = <strong>Baikal</strong>: Utilisez le point de terminaison DAV de votre serveur Baikal.<br> En général : <code>https://cal.example.com/dav.php/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong>. Utilisez le point de terminaison SOAP :<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> Le nom d'utilisateur est l'adresse de la boîte aux lettres ; le mot de passe doit accepter l'authentification HTTP Basic sur TLS (à activer sur une boîte de service si votre tenant l'a désactivée).<br> Pensez également à choisir <strong>Microsoft Exchange (EWS)</strong> dans la liste Backend ci-dessus.
source-form-help-google = <strong>Google Agenda</strong> : connexion via OAuth2. Aucun mot de passe requis.<br>
source-form-help-other = Saisissez l'<strong>URL racine DAV</strong> de votre serveur CalDAV, pas celle d'un agenda précis ni un lien public.<br> calrs découvrira automatiquement vos agendas via PROPFIND (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Gras
editor-italic-short = Italique
editor-link-short = Insérer un lien

# Team creation (templates/team_form.html)

team-form-heading = Nouvelle équipe
team-form-name = Nom de l'équipe
team-form-name-placeholder = Ingénierie
team-form-slug = Identifiant
team-form-slug-hint = (identifiant compatible URL)
team-form-slug-pattern-title = Minuscules, chiffres et tirets uniquement
team-form-description = Description
team-form-optional = (facultatif)
team-form-description-placeholder = En quelques mots, le rôle de cette équipe...
team-form-description-help = Affichée sur la page de l'équipe. Prend en charge **gras**, *italique* et [liens](url).
team-form-visibility = Visibilité
team-form-public = Publique
team-form-private = Privée
team-form-visibility-help = Les équipes privées reçoivent un jeton d'invitation à partager. Les équipes publiques apparaissent sur la page de profil d'équipe.
team-form-members = Membres
team-form-members-help = Vous serez ajouté comme administrateur de l'équipe automatiquement. Ajoutez des utilisateurs ou liez des groupes OIDC.
team-form-search-placeholder = Rechercher des utilisateurs ou des groupes...
team-form-search-users = Utilisateurs
team-form-search-groups = Groupes OIDC
team-form-you = (vous)
team-form-submit = Créer l'équipe

# Team settings (templates/team_settings.html)

team-settings-page-title = Paramètres
team-settings-subtitle = Paramètres de l'équipe — les administrateurs de l'équipe peuvent les modifier.
team-settings-public-url = URL publique
team-settings-public-url-help = N'importe qui peut réserver via ce lien.
team-settings-invite-link = Lien d'invitation
team-settings-invite-link-help = Partagez ce lien pour donner accès à la page de réservation de cette équipe privée.
team-settings-avatar = Avatar de l'équipe
team-settings-profile = Profil
team-settings-description-placeholder = Présentez cette équipe...
team-settings-description-help = Affichée sur la page de réservation publique de l'équipe. Prend en charge **gras**, *italique* et [liens](url).
team-settings-visibility-help = Les équipes publiques sont listées sur la page de profil d'équipe. Les équipes privées nécessitent un lien d'invitation.
team-settings-members-help = Gérez les membres de cette équipe. Ajoutez des utilisateurs ou liez des groupes OIDC pour une synchronisation automatique.
team-settings-time-off-help = Absences prévues des membres, affichées dans votre fuseau horaire, { $timezone }. Leurs notes restent privées.
team-settings-role-member = Membre
team-settings-role-admin = Administrateur
team-settings-oidc-group = Groupe OIDC
team-settings-remove = Retirer
team-settings-save = Enregistrer les modifications
team-settings-danger-zone = Zone sensible
team-settings-danger-help = Supprimer définitivement cette équipe. Les types d'événement seront dissociés (pas supprimés). Cette action est irréversible.
team-settings-delete = Supprimer cette équipe
team-settings-delete-confirm = Supprimer l'équipe « { $name } » ? Cette action est irréversible.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Modifier le type d'événement
etf-heading-new = Nouveau type d'événement
etf-disabled-banner = Ce type d'événement est désactivé : personne ne peut encore le réserver.
etf-team = Équipe
etf-team-hint = (facultatif — laissez vide pour un type d'événement personnel)
etf-team-personal = Personnel
etf-scheduling-mode = Mode de planification
etf-mode-round-robin = Tour de rôle — attribuer à un membre disponible
etf-mode-collective = Collectif — tous les membres doivent être disponibles
etf-scheduling-mode-help = Le tour de rôle attribue la réservation à un membre disponible (le moins chargé d'abord). Le mode collectif exige que tous les membres soient libres en même temps.
etf-title = Titre
etf-title-placeholder = Appel de découverte de 30 min
etf-slug = Identifiant
etf-slug-placeholder = généré à partir du titre
etf-description-placeholder = Un court appel de découverte pour parler de...
etf-description-help = Affichée sur la page de réservation. Prend en charge **gras**, *italique* et [liens](url).
etf-location = Lieu
etf-location-link = Visioconférence (URL fixe)
etf-location-jitsi = Jitsi (salon généré automatiquement)
etf-location-webhook = Webhook (fournisseur personnalisé)
etf-location-phone = Téléphone
etf-location-in-person = En personne
etf-location-custom = Personnalisé
etf-location-details = Détails
etf-location-details-placeholder = https://meet.example.com/ma-salle
etf-pattern-placeholder = Laissez vide pour utiliser le motif par défaut de l'organisation
etf-duration = Durée (minutes)
etf-slot-interval = Intervalle entre créneaux (minutes)
etf-slot-interval-placeholder = Identique à la durée
etf-slot-interval-help = Fréquence de démarrage des créneaux. Laissez vide pour suivre la durée.
etf-required-members = Membres requis
etf-required-members-help = Tous les membres cochés doivent être libres pour qu'un créneau soit proposé. Décochez les membres à exclure (leur disponibilité sera ignorée).
etf-member-priority = Priorité des membres
etf-member-priority-help = Les membres les plus prioritaires reçoivent les réservations en premier lorsqu'ils sont disponibles. À priorité égale, la répartition suit le nombre de réservations récentes.
etf-member-timezone-title = Fuseau horaire du membre. Ses horaires de travail personnels sont interprétés dans ce fuseau.
etf-priority-high = Haute
etf-priority-medium = Moyenne
etf-priority-low = Basse
etf-section-availability = Disponibilité
etf-timezone-help = Les horaires ci-dessous sont interprétés dans ce fuseau. Pour un type d'événement d'équipe, choisissez le fuseau de travail de l'équipe (pas forcément celui du créateur).
etf-reset-default = Rétablir mes horaires par défaut
etf-reset-default-title = Remplacer ces horaires par la disponibilité par défaut de votre profil
etf-availability-prefilled = Pré-rempli depuis votre { $link }. Vous pouvez le remplacer ici pour ce type d'événement.
etf-availability-prefilled-link = disponibilité par défaut
etf-section-buffers = Marges et préavis
etf-buffer-before = Marge avant (min)
etf-buffer-after = Marge après (min)
etf-min-notice = Préavis minimum
etf-min-notice-help = Délai minimum entre la réservation et la réunion.
etf-section-limits = Limites de réservation
etf-first-slot-only = Un seul créneau par jour
etf-first-slot-only-help = N'afficher que le premier horaire disponible de chaque journée.
etf-freq-limit = Limiter la fréquence des réservations
etf-freq-limit-help = Limiter le nombre de réservations de cet événement par période.
etf-add-limit = Ajouter une limite
etf-section-options = Options de réservation
etf-requires-confirmation = Confirmation requise
etf-requires-confirmation-help = Les réservations resteront en attente jusqu'à votre validation depuis le tableau de bord.
etf-sms = Notifications SMS
etf-sms-off = Désactivé, aucun numéro demandé
etf-sms-optional = Facultatif, les invités peuvent laisser un numéro
etf-sms-required = Obligatoire, les invités doivent laisser un numéro
etf-sms-help = Envoie un SMS à l'invité lorsque sa réservation est confirmée, déplacée, annulée ou sur le point de commencer, en plus de l'e-mail. Un invité qui laisse le champ vide ne reçoit tout simplement pas de SMS. Nécessite une passerelle SMS dans le { $link }.
etf-admin-panel-link = panneau d'administration
etf-additional-guests = Invités supplémentaires
etf-guests-none = Les invités ne peuvent pas en ajouter d'autres
etf-additional-guests-help = Permettre à la personne qui réserve d'inviter d'autres participants, qui recevront l'invitation d'agenda.
etf-default-view = Vue d'agenda par défaut
etf-view-month = Mois — grille d'agenda avec liste de créneaux
etf-view-week = Semaine — colonnes sur 7 jours avec créneaux
etf-view-column = Colonne — jours listés avec leurs créneaux
etf-view-week-short = semaine
etf-view-column-short = colonne
etf-default-view-help = La vue affichée par défaut aux invités. Ils peuvent en changer à tout moment.
etf-conflict-calendars = Agendas de conflits
etf-conflict-calendars-help = Choisissez les agendas à consulter pour détecter les conflits. Si aucun n'est sélectionné, vos agendas occupés sont consultés, sans les agendas que des collègues vous ont partagés : sélectionnez tous les agendas qui doivent bloquer, partagés compris.
etf-no-resources = Aucune ressource partagée configurée pour le moment. Ajoutez-en une (laboratoire de démo, salle de réunion) dans le { $link } pour l'exiger ici.
etf-section-access = Accès et notifications
etf-visibility-public = Public — visible sur votre profil
etf-visibility-internal = Interne — tout collègue peut générer des liens d'invitation
etf-visibility-private = Privé — uniquement sur lien d'invitation
etf-visibility-help = Détermine qui peut voir et réserver ce type d'événement.
etf-vis-internal = Interne
etf-reminder = Rappel de réservation
etf-reminder-none = Aucun rappel
etf-reminder-help = Envoyer un e-mail de rappel à vous et à votre invité avant la réunion.
etf-guest-footer = Pied de page des e-mails
etf-guest-footer-placeholder = ex. Une question ? Téléphone : +33 1 23 45 67 89
etf-guest-footer-help = Ajouté à la fin de chaque e-mail que l'invité reçoit pour cette réservation, quelle que soit la langue. Remplace le pied de page par défaut défini par l'administrateur. Gras, italique et liens uniquement.
etf-dynamic-group = Lien de groupe dynamique
etf-dynamic-group-help = Créez un lien de réunion ponctuel qui vérifie vos disponibilités et celles d'autres utilisateurs.
etf-dynamic-group-search = Rechercher un utilisateur à ajouter...
etf-dynamic-group-note = Seuls les utilisateurs qui autorisent les liens de groupe dynamiques sont affichés.
etf-dynamic-group-url = URL du lien de groupe
etf-watcher-teams = Équipes observatrices
etf-watcher-teams-help = Les équipes sélectionnées seront notifiées à chaque réservation. Leurs membres peuvent prendre en charge une réservation pour y participer.
etf-save = Enregistrer les modifications
etf-create = Créer le type d'événement
etf-js-loading = Chargement...
etf-js-no-default = Aucun défaut défini
etf-js-reset-done = Rétabli !
etf-js-error = Erreur
etf-js-remove-limit = Supprimer la limite
etf-period-day = Par jour
etf-period-week = Par semaine
etf-period-month = Par mois
etf-period-year = Par an

# Event type form: runtime summary hints (templates/event_type_form.html)

# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = Aucun jour défini
etf-hint-every-day = Tous les jours
etf-fmt-day-one = %1 jour
etf-fmt-day-other = %1 jours
etf-fmt-hours = %1 h
etf-fmt-minutes = %1 min
etf-hint-buffer-both = %1 min avant, %2 min après
etf-hint-buffer-before = %1 min de marge avant
etf-hint-buffer-after = %1 min de marge après
etf-hint-notice = %1 de préavis
etf-hint-no-buffers = Aucune marge, réservation à tout moment
etf-hint-max = Max %1
etf-hint-period-day = /jour
etf-hint-period-week = /semaine
etf-hint-period-month = /mois
etf-hint-period-year = /an
etf-hint-no-limits = Aucune limite
etf-hint-confirmation-required = Confirmation requise
etf-hint-auto-confirmed = Confirmation automatique
etf-hint-extra-guests-one = jusqu'à %1 invité supplémentaire
etf-hint-extra-guests-other = jusqu'à %1 invités supplémentaires
etf-hint-view = vue %1
etf-hint-reminder = rappel %1 avant
etf-hint-no-reminder = aucun rappel

etf-guests-up-to =
    { $count ->
        [one] Jusqu'à { $count } invité supplémentaire
       *[other] Jusqu'à { $count } invités supplémentaires
    }

etf-reminder-hours =
    { $count ->
        [one] { $count } heure avant
       *[other] { $count } heures avant
    }

etf-reminder-days =
    { $count ->
        [one] { $count } jour avant
       *[other] { $count } jours avant
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Création d'un type d'événement <strong>public</strong> &mdash; toute personne disposant du lien peut réserver.
etf-preset-private = Création d'un type d'événement <strong>privé</strong> &mdash; seules les personnes que vous invitez peuvent réserver.
etf-preset-internal = Création d'un type d'événement <strong>interne</strong> &mdash; tout collègue peut partager le lien de réservation.
etf-preset-team = Création d'un type d'événement <strong>d'équipe</strong> &mdash; les réservations sont réparties entre les membres.
etf-pattern-hint = Motif personnalisé facultatif. Jetons : <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Laissez vide pour utiliser le motif par défaut configuré par un administrateur.
etf-pattern-random-warning = Ce motif ne contient pas de jeton <code>{"{"}random{"}"}</code>. Deux réservations de ce type d'événement le même jour partageront le même salon, et le second invité pourra entrer dans la réunion du premier. N'utilisez des salons fixes que si c'est bien l'effet recherché.
etf-webhook-hint = L'URL de réunion propre à chaque réservation est récupérée depuis le webhook configuré par un administrateur sous Administration &rarr; Webhook de réunion. Aucune URL n'est nécessaire ici.

# Admin panel (templates/admin.html)

admin-page-title = Administration
admin-action-refused = Action refusée :
admin-error-last-admin = C'est le dernier administrateur actif. Promouvez ou réactivez d'abord un autre administrateur.
admin-logo = Logo de l'entreprise
admin-logo-help = Affiché sur les pages de réservation publiques. PNG, JPEG, GIF ou WebP, 2 Mo maximum.
admin-logo-refused = Le logo n'a pas été enregistré. Envoyez une image PNG, JPEG, GIF ou WebP de 2 Mo maximum.
admin-favicon = Favicon
admin-favicon-help = Affiché dans les onglets du navigateur et les favoris. Recommandé : un PNG carré d'au moins 32×32 pixels, 512 Ko maximum. Sans favicon, le crabe de calrs est affiché.
admin-favicon-refused = Le favicon n'a pas été enregistré. Envoyez une image PNG, JPEG, GIF ou WebP de 512 Ko maximum.
admin-upload-save-failed = Le fichier n'a pas pu être écrit dans le répertoire de données du serveur. Le journal du serveur donne les détails.

# User and team avatar uploads (#307)
avatar-upload-refused = L'image n'a pas été enregistrée. Envoyez une image PNG, JPEG, GIF ou WebP de 2 Mo au maximum.
avatar-upload-save-failed = L'image n'a pas pu être enregistrée sur le serveur. Réessayez plus tard ou demandez à votre administrateur de consulter le journal du serveur.
admin-company-link = Lien de l'entreprise
admin-company-link-help = Le logo pointe vers cette URL sur les pages de réservation publiques. Laissez vide pour ne pas créer de lien.
admin-theme = Thème
admin-theme-help = Choisissez un thème de couleurs pour toutes les pages. Le basculement clair/sombre est indépendant : les thèmes s'adaptent aux deux modes.
admin-theme-default = Par défaut
admin-theme-default-desc = Bleu épuré
admin-theme-nord-desc = Givre arctique
admin-theme-dracula-desc = Violet sombre
admin-theme-gruvbox-desc = Rétro chaleureux
admin-theme-solarized-desc = Le classique d'Ethan
admin-theme-tokyo-desc = Ville au néon
admin-theme-custom = Personnalisé
admin-theme-custom-desc = Vos couleurs
admin-custom-colors = Couleurs personnalisées
admin-color-accent = Accent
admin-color-accent-hover = Accent au survol
admin-color-bg = Arrière-plan
admin-color-surface = Surface
admin-color-text = Texte
admin-save-theme = Enregistrer le thème
admin-users = Utilisateurs
admin-user-filter = Filtrer par nom ou e-mail…
admin-badge-admin = admin
admin-badge-disabled = désactivé
admin-impersonate = Prendre l'identité
admin-demote = Rétrograder
admin-promote = Promouvoir
admin-disable = Désactiver
admin-enable = Activer
admin-delete = Supprimer
admin-no-users-match = Aucun utilisateur ne correspond au filtre.
admin-no-users = Aucun utilisateur pour le moment.
admin-groups = Groupes
admin-group-filter = Filtrer par nom de groupe…
admin-group-name = Nom du groupe
admin-weight = poids :
admin-no-groups-match = Aucun groupe ne correspond au filtre.
admin-no-groups = Aucun groupe synchronisé pour le moment. Les groupes sont synchronisés automatiquement depuis votre fournisseur OIDC.
admin-auth-settings = Paramètres d'authentification
admin-registration-enabled = Inscriptions activées
admin-allowed-domains = Domaines d'e-mail autorisés
admin-allowed-domains-hint = (séparés par des virgules, vide pour tous)
admin-save-auth = Enregistrer les paramètres d'authentification
admin-system-settings = Paramètres système
admin-base-url = URL de base
admin-base-url-help = URL publique de cette instance. Utilisée pour les redirections OIDC et les liens dans les e-mails (approbation/refus, annulation, rappels).
admin-private-hosts = Liste d'hôtes privés autorisés
admin-private-hosts-help = Noms d'hôtes, séparés par des virgules, autorisés à résoudre vers des IP privées ou réservées pour les sources CalDAV/EWS (dérogation à la protection SSRF). N'ajoutez que des hôtes que vous contrôlez (par exemple un serveur d'agenda sur le même réseau Docker). Laissez vide pour conserver la protection sur tous les hôtes.
admin-german-formal = Forme d'adresse en allemand
admin-german-formal-du = Informelle (du)
admin-german-formal-sie = Formelle (Sie)
admin-german-formal-help = Comment les pages et les e-mails s'adressent aux visiteurs dont le navigateur demande l'allemand. Les utilisateurs qui ont choisi une langue dans Profil et paramètres gardent la leur.
admin-unset-env = Supprimez la variable d'environnement pour modifier ce réglage ici.
admin-save-system = Enregistrer les paramètres système
admin-status = État :
admin-status-enabled = activé
admin-status-disabled = désactivé
admin-status-disabled-paren = (désactivé)
admin-status-configured = configuré
admin-status-not-configured = non configuré
admin-via-environment = (via l'environnement)
admin-issuer = Émetteur :
admin-client-id = ID client :
admin-instance = Instance :
admin-oidc-settings = Paramètres OIDC
admin-oidc-enabled = OIDC activé
admin-issuer-url = URL de l'émetteur
admin-client-id-label = ID client
admin-client-secret = Secret client
admin-keep-current-hint = (laissez vide pour conserver l'actuel)
admin-keep-current-set-hint = (laissez vide pour conserver l'actuel, actuellement défini)
admin-keep-unchanged = Laissez vide pour ne rien changer
admin-oidc-auto-register = Inscrire automatiquement les nouveaux utilisateurs OIDC
admin-save-oidc = Enregistrer les paramètres OIDC
admin-google = Google Agenda (OAuth2)
admin-save-google = Enregistrer les paramètres OAuth2 Google
admin-captcha = Captcha
admin-instance-url = URL de l'instance
admin-site-key = Clé de site
admin-secret = Secret
admin-widget-url = URL du script du widget
admin-widget-url-help = À remplacer si le CDN est bloqué. Les modifications prennent effet dès l'enregistrement.
admin-wasm-url = URL du WASM du widget
admin-wasm-url-help = Adresse d'où le widget charge son solveur. Indiquez le fichier WASM servi par votre serveur Cap pour que les pages de réservation ne contactent plus cdn.jsdelivr.net. Laissez vide pour la valeur par défaut.
admin-captcha-wasm-url-invalid = L'URL du WASM du widget doit être une URL http(s) absolue.
admin-captcha-disable-help = Laissez l'URL de l'instance, la clé de site et le secret vides pour désactiver le captcha sur les pages de réservation.
admin-save-captcha = Enregistrer les paramètres du captcha
admin-resources = Ressources
admin-resources-help = Ressources partagées réservables (laboratoire de démo, salles de réunion) alimentées par un flux d'agenda. Rattachée à des types d'événement, une ressource occupée bloque les réservations.
admin-resource-stats = Événements en cache : { $events } &middot; Rattachée à { $attached } type(s) d'événement
admin-never = jamais
admin-resource-sync-failed = (échec de la dernière tentative : { $error })
admin-writeback-enabled = Écriture : activée ({ $via })
admin-writeback-readonly = Écriture : lecture seule
admin-teams-allowed = Équipes autorisées :
admin-teams-allowed-none = aucune (administrateurs globaux uniquement)
admin-sync-now = Synchroniser maintenant
admin-test-write = Tester l'écriture
admin-delete-resource-confirm = Supprimer cette ressource ? Les types d'événement qui l'utilisent cesseront de la vérifier.
admin-name = Nom
admin-name-help = Laissez vide pour récupérer le nom depuis le flux.
admin-feed-url = URL du flux ICS (adresse de publication)
admin-feed-url-help = BlueMind : l'adresse d'agenda publique ou privée de l'agenda de la ressource.
admin-caldav-url = URL de la collection CalDAV (pour l'écriture)
admin-caldav-url-help = Facultatif. Pour BlueMind, elle est déduite automatiquement de l'URL du flux.
admin-caldav-username = Nom d'utilisateur CalDAV
admin-caldav-password = Mot de passe CalDAV
admin-resource-teams = Équipes autorisées à utiliser cette ressource
admin-resource-teams-help = Les administrateurs de ces équipes peuvent rattacher cette ressource à leurs types d'événement d'équipe. Vide : administrateurs globaux uniquement.
admin-no-teams = Aucune équipe pour le moment.
admin-save-resource = Enregistrer la ressource
admin-add-resource = Ajouter une ressource
admin-jitsi = Jitsi (liens de réunion générés automatiquement)
admin-jitsi-help = Lorsque le lieu d'un type d'événement est « Jitsi (salon généré automatiquement) », calrs construit une URL de salon pour chaque réservation en ajoutant le motif ci-dessous à votre URL de base Jitsi. Aucun appel d'API externe n'est nécessaire.
admin-display-name = Nom affiché
admin-jitsi-display-name-placeholder = ex. Meet DYB
admin-jitsi-display-name-help = Affiché aux invités sur le sélecteur de créneaux et le formulaire de réservation. « Visioconférence » par défaut si vide.
admin-room-pattern = Motif de nom de salon
admin-jitsi-disable-help = Laissez l'URL de base vide pour désactiver la génération automatique Jitsi.
admin-save-jitsi = Enregistrer les paramètres Jitsi
admin-meeting-webhook = Webhook de réunion (fournisseur de votre choix)
admin-webhook-url = URL du webhook
admin-webhook-display-name-placeholder = ex. Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Affiché aux invités à la place du badge générique « Visioconférence ».
admin-authentication = Authentification
admin-auth-none = Aucune
admin-auth-hmac = HMAC-SHA256 (en-tête X-Calrs-Signature)
admin-shared-secret = Secret partagé
admin-webhook-disable-help = Laissez l'URL vide pour désactiver le webhook de réunion.
admin-save-webhook = Enregistrer les paramètres du webhook
admin-smtp = Paramètres SMTP
admin-smtp-test-sent = E-mail de test envoyé.
admin-smtp-test-failed = L'e-mail de test n'a pas pu être envoyé. Vérifiez les journaux du serveur et vos paramètres SMTP.
admin-smtp-env-error = Erreur de configuration SMTP par l'environnement :
admin-smtp-host = Hôte :
admin-smtp-from = Expéditeur :
admin-smtp-enabled = SMTP activé
admin-host = Hôte
admin-port = Port
admin-tls-mode = Mode TLS
admin-tls-starttls = STARTTLS (port 587)
admin-tls-implicit = TLS implicite (port 465)
admin-tls-none = Aucun, non chiffré (MTA local uniquement)
admin-smtp-username-hint = (laissez vide pour un relais sans authentification)
admin-from-email = Adresse d'expédition
admin-from-name = Nom d'expéditeur
admin-save-smtp = Enregistrer les paramètres SMTP
admin-send-test-email = Envoyer un e-mail de test à
admin-send-test-email-hint = (par défaut, l'adresse de votre compte)
admin-send-test-email-btn = Envoyer l'e-mail de test
admin-smtp-clear-confirm = Supprimer la configuration SMTP enregistrée en base ?
admin-clear-db-config = Effacer la configuration en base
admin-sms = Paramètres SMS
admin-sms-help = Facultatif. Les SMS ne sont envoyés que pour les réservations sur les types d'événement où les « Notifications SMS » sont activées, et uniquement si l'invité a laissé un numéro.
admin-sms-test-sent = Message de test envoyé.
admin-sms-test-checked = Identifiants acceptés.
admin-sms-test-error = La passerelle SMS a refusé la requête.
admin-sms-captcha-warning = Le formulaire de réservation est public et le numéro du destinataire vient de l'invité : sans captcha, les SMS forment un relais ouvert que quelqu'un peut vous facturer. Configurez le captcha ci-dessus et restreignez les pays de destination dans les réglages de votre passerelle.
admin-sms-sent-today = Envoyés aujourd'hui :
admin-sms-of-cap = sur { $cap }
admin-sms-config-error = Erreur de configuration SMS :
admin-sms-gateway = Passerelle :
admin-sms-account = Compte :
admin-sms-sender = Expéditeur :
admin-sms-enabled = SMS activés
admin-sms-gateway-label = Passerelle
admin-required-on-switch = Obligatoire lors du changement de passerelle
admin-sms-docs = Documentation de l'API { $provider }
admin-sms-country = Indicatif pays par défaut
admin-sms-country-hint = (utilisé quand un invité saisit un numéro local)
admin-sms-daily-cap = Limite quotidienne
admin-sms-daily-cap-hint = (messages par jour pour toute l'instance, 0 pour aucune limite)
admin-sms-daily-cap-help = Au-delà de la limite, calrs cesse d'envoyer des SMS et continue les e-mails : une réservation n'échoue jamais parce que le budget SMS est épuisé.
admin-save-sms = Enregistrer les paramètres SMS
admin-send-test-sms = Envoyer un message de test à
admin-send-test-sms-hint-check = (laissez vide pour vérifier seulement les identifiants)
admin-send-test-sms-hint-e164 = (format E.164)
admin-test-gateway = Tester la passerelle
admin-sms-clear-confirm = Supprimer la configuration SMS enregistrée en base ?
admin-sms-allow-all = Autoriser tout utilisateur à activer les SMS sur ses types d'événement
admin-sms-allow-all-help = Désactivé par défaut : les SMS consomment le crédit du compte configuré ici, donc seuls les administrateurs peuvent basculer un type d'événement en mode SMS.
admin-save-policy = Enregistrer la politique
admin-guest-footer = Pied de page des e-mails aux invités
admin-guest-footer-help = Un court texte ajouté à la fin de chaque e-mail envoyé aux invités et aux participants : confirmations, rappels, annulations, etc. Les e-mails destinés à l'hôte, et au membre d'équipe qui a pris en charge la réservation, ne sont pas modifiés.
admin-guest-footer-default = Pied de page par défaut
admin-guest-footer-default-help = Utilisé lorsqu'un type d'événement n'a pas de pied de page propre. Le même texte est envoyé dans toutes les langues. Gras, italique et liens uniquement.
admin-guest-footer-allow-all = Autoriser tout utilisateur à définir un pied de page sur ses propres types d'événement
admin-guest-footer-allow-all-help = Désactivé par défaut : le pied de page part au nom de cette instance, donc seuls les administrateurs peuvent en écrire un. Un pied de page défini sur un type d'événement remplace celui par défaut ci-dessus.
admin-save-guest-footer = Enregistrer le pied de page
admin-page-of = Page %1 sur %2
admin-show-more-js = Afficher %1 de plus
admin-show-fewer = Afficher moins

# Admin panel: overview, section tabs, users table (templates/admin.html)

admin-copy = Copier
admin-copied = Copié
admin-sections = Sections de l'administration
admin-tab-overview = Vue d'ensemble
admin-tab-users = Utilisateurs
admin-tab-access = Connexion et sécurité
admin-tab-notifications = E-mail et SMS
admin-tab-meetings = Liens de réunion
admin-tab-resources = Ressources
admin-tab-branding = Apparence
admin-tab-system = Système
admin-att-title = À traiter
admin-mast-users = { $count ->
    [one] { $count } utilisateur
   *[other] { $count } utilisateurs
}
admin-mast-bookings = { $count ->
    [one] { $count } réservation
   *[other] { $count } réservations
}
admin-mast-attention = { $count ->
    [one] { $count } point à traiter.
   *[other] { $count } points à traiter.
}
admin-mast-clear = Rien à traiter.
admin-sev-failing = En échec
admin-sev-missing = Manquant
admin-sev-off = Coupé
admin-sev-exposed = Exposé
admin-sev-expiring = Expire
admin-sev-stale = En retard
admin-fig-title = Chiffres clés
admin-chart-week-short = S{ $week }
admin-chart-now = en cours
admin-chart-ended-key = Annulées ou refusées, comptées sous chaque semaine
admin-pct = { $value } %
admin-att-review = Examiner
admin-att-smtp-error = La configuration SMTP fournie par l'environnement est invalide : aucun e-mail n'est envoyé.
admin-att-smtp-missing = L'e-mail n'est pas configuré : invités et hôtes ne reçoivent ni confirmation, ni rappel, ni annulation.
admin-att-smtp-disabled = L'envoi des e-mails est désactivé.
admin-att-sources-failing = Sources d'agenda en échec de synchronisation
admin-att-writeback = Réservations à venir absentes de l'agenda de l'hôte
admin-att-sms-captcha = Les SMS sont configurés sans captcha : le formulaire public de réservation peut servir à envoyer des messages payants.
admin-att-resources-failing = Flux de ressources en échec de synchronisation
admin-att-pending-expiring = Demandes en attente qui expirent dans les 24 heures
admin-att-sources-stale = Sources d'agenda non synchronisées depuis plus de 24 heures
admin-att-base-url = L'URL publique de base n'est pas définie : les liens des e-mails et les redirections de l'authentification unique peuvent ne pas fonctionner.
admin-kpi-bookings = Réservations, 30 derniers jours
admin-kpi-prev = 30 jours précédents : { $count }
admin-kpi-upcoming = Rendez-vous à venir
admin-kpi-upcoming-week = 7 prochains jours : { $count }
admin-kpi-pending = Demandes en attente
admin-kpi-pending-expiring = Expirent dans les 24 heures : { $count }
admin-kpi-pending-none = Rien n'attend d'approbation
admin-kpi-pending-ok = Aucune n'expire dans les 24 heures
admin-kpi-active = Utilisateurs actifs
admin-kpi-active-sub = Connectés dans les 30 derniers jours, sur l'ensemble des comptes activés
admin-chart-weekly = Réservations par semaine
admin-chart-weekly-help = Réservations effectuées chaque semaine sur les 12 dernières semaines, selon leur statut actuel.
admin-series-confirmed = Confirmées
admin-series-pending = En attente
admin-series-ended = Annulées ou refusées
admin-chart-bar-label = Semaine du { $week } : { $confirmed } confirmées, { $pending } en attente, { $ended } annulées ou refusées
admin-chart-table = Afficher sous forme de tableau
admin-chart-week = Semaine du
admin-chart-total = Total
admin-chart-empty = Aucune réservation sur les 12 dernières semaines.
admin-status-title = Réservations par statut
admin-status-empty = Aucune réservation pour l'instant.
admin-st-cancelled = Annulées
admin-st-declined = Refusées
admin-svc-title = Services
admin-svc-email = E-mail (SMTP)
admin-svc-sms = SMS
admin-svc-oidc = Authentification unique (OIDC)
admin-svc-google = Google Agenda
admin-svc-microsoft = Microsoft 365 / Outlook.com
admin-svc-registration = Inscription libre
admin-svc-jitsi = Liens de réunion Jitsi
admin-svc-webhook = Webhook de réunion
admin-svc-base-url = URL publique de base
admin-state-error = erreur
admin-top-event-types = Types d'événement les plus réservés
admin-top-hosts = Hôtes les plus sollicités
admin-top-help = Réservations confirmées effectuées au cours des 90 derniers jours.
admin-top-empty = Aucune réservation confirmée au cours des 90 derniers jours.
admin-top-team = équipe
admin-health-title = État des agendas
admin-health-help = État de synchronisation de chaque source d'agenda, et réservations qui n'ont pas pu être écrites dans l'agenda d'un hôte.
admin-health-healthy = En bon état
admin-health-failing = En échec
admin-health-stale = Non synchronisées depuis 24 h
admin-health-disabled = Désactivées
admin-health-writeback-count = Échecs d'écriture
admin-health-no-sources = Aucune source d'agenda n'est encore connectée.
admin-health-failing-title = Sources en échec de synchronisation
admin-health-col-source = Source
admin-health-col-owner = Propriétaire
admin-health-col-error = Dernière erreur
admin-health-col-since = En échec depuis
admin-health-attempts = Tentatives échouées : { $count }
admin-health-owner-hint = Les disponibilités restent calculées à partir de la dernière synchronisation réussie. Le propriétaire voit l'erreur sur sa page Sources d'agenda et reçoit un e-mail si elle persiste.
admin-health-writeback-title = Réservations absentes de l'agenda de l'hôte
admin-health-col-booking = Réservation
admin-health-col-host = Hôte
admin-health-col-retry = Nouvel essai
admin-health-retrying = nouvel essai prévu
admin-health-retry-exhausted = essais épuisés
admin-health-resources-title = Flux de ressources en échec de synchronisation
admin-inv-title = Inventaire
admin-inv-users = Comptes utilisateurs
admin-inv-users-sub = Admins : { $admins }, désactivés : { $disabled }
admin-inv-signin-sub = Mot de passe : { $local }, SSO : { $sso }
admin-inv-new-users = Nouveaux comptes, 30 jours
admin-inv-mfa-sub = Comptes locaux avec la double authentification activée
admin-inv-teams = Équipes
admin-inv-groups-sub = Groupes OIDC : { $count }
admin-inv-event-types = Types d'événement
admin-inv-event-types-sub = Activés : { $enabled }, équipe : { $team }
admin-inv-bookings = Réservations
admin-inv-bookings-sub = Confirmées : { $count }
admin-inv-sources = Sources d'agenda
admin-inv-sources-sub = Activées : { $count }
admin-inv-resources-failing = Flux en échec : { $count }
admin-inv-sms = SMS, 30 derniers jours
admin-inv-sms-today = aujourd'hui : { $count }
admin-users-filter-label = Filtrer les utilisateurs
admin-filter-all = Tous
admin-filter-admins = Admins
admin-filter-disabled = Désactivés
admin-filter-never = Jamais connectés
admin-provider-local = Mot de passe
admin-provider-oidc = SSO
admin-col-user = Utilisateur
admin-col-role = Rôle
admin-col-signin = Connexion
admin-col-last = Dernière connexion
admin-col-created = Inscrit
admin-col-actions = Actions
admin-user-actions = Actions pour { $name }
admin-role-user = membre
admin-badge-mfa = 2FA
admin-delete-confirm-btn = Supprimer définitivement
admin-users-shown = %1 utilisateurs sur %2

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Supprimer définitivement l'utilisateur { $email } ?{"\u000A"}{"\u000A"}Cela efface son compte, son profil de planification, ses sources d'agenda, ses types d'événement et toutes les données dont il est le seul propriétaire. Les réservations passées seront supprimées avec leurs types d'événement.{"\u000A"}{"\u000A"}Pour les utilisateurs OIDC/SSO : si l'inscription automatique est activée, la personne sera recréée à sa prochaine connexion.{"\u000A"}{"\u000A"}Cette action est irréversible.
admin-system-settings-help = URL publique et paramètres de sécurité réseau. Ils peuvent aussi être définis par les variables d'environnement <code>CALRS_BASE_URL</code> et <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Lorsqu'une variable d'environnement est définie, elle <strong>prime</strong> sur la valeur ci-dessous.
admin-set-by-env = défini par l'environnement ({ $var }), prioritaire sur la valeur enregistrée
admin-google-help = Pour activer l'intégration Google Agenda, créez des identifiants OAuth2 sur la <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Activez la <strong>CalDAV API</strong> (lecture des disponibilités et écriture des réservations) et la <strong>Google Calendar API</strong> (liens Google Meet), puis ajoutez { $redirect_uri } comme URI de redirection autorisée.
admin-room-pattern-help = Jetons disponibles : <code>{"{"}username{"}"}</code> (hôte), <code>{"{"}event{"}"}</code> (identifiant du type d'événement), <code>{"{"}date{"}"}</code> (AAAAMMJJ), <code>{"{"}random{"}"}</code> (8 caractères). Par défaut : { $default }.
admin-room-pattern-warning = Sans <code>{"{"}random{"}"}</code>, le nom du salon est prévisible : deux invités réservant le même type d'événement le même jour se retrouvent dans le même salon et peuvent voir la réunion de l'autre. Les salons fixes restent possibles (par exemple un salon personnel par hôte), mais n'activez cela qu'en connaissance de cause.
admin-meeting-webhook-help = Lorsque le lieu d'un type d'événement est « Webhook (fournisseur personnalisé) », calrs envoie la réservation en POST à cette URL à la confirmation et attend en retour un corps JSON <code>{"{"}"url": "https://..."{"}"}</code>.
admin-auth-hmac-help = Avec HMAC, calrs envoie <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> calculé sur le corps brut de la requête.
admin-tls-none-warning = Ne choisissez <strong>Aucun</strong> que pour un relais sur cette machine qui ne propose pas STARTTLS, ou dont le certificat est auto-signé. Le courrier, et les éventuels identifiants, circulent alors en clair.
admin-smtp-env-error-help = Corrigez les variables d'environnement <code>CALRS_SMTP_*</code>, ou supprimez-les pour gérer le SMTP depuis la base ici.
admin-smtp-env-managed = Géré par <strong>variables d'environnement</strong> (prioritaires sur la base). Modifiez les variables <code>CALRS_SMTP_*</code> pour changer ce réglage, ou supprimez-les pour gérer le SMTP ici.
admin-smtp-env-help = Vous pouvez aussi configurer par variables d'environnement (prioritaires sur ceci) : <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> ou <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Seules <code>CALRS_SMTP_HOST</code> et <code>CALRS_SMTP_FROM_EMAIL</code> sont obligatoires ; omettez le nom d'utilisateur et le mot de passe pour relayer via un MTA local sans authentification.
admin-sms-env-error-help = Corrigez les variables d'environnement <code>CALRS_SMS_*</code>, ou supprimez-les pour gérer les SMS depuis la base ici.
admin-sms-env-managed = Géré par <strong>variables d'environnement</strong> (prioritaires sur la base). Modifiez les variables <code>CALRS_SMS_*</code> pour changer ce réglage, ou supprimez-les pour gérer les SMS ici.
admin-sms-env-help = Vous pouvez aussi configurer par variables d'environnement (prioritaires sur ceci) : <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>Le mode d'essai Twilio est actif</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Les invités reçoivent le modèle prédéfini <code>sms_appointment_reminders</code> de Twilio, pas le vrai message, et seuls les numéros vérifiés dans votre console Twilio sont joignables. C'est une aide au test pour les comptes d'essai. Supprimez la variable avant d'accepter des réservations.

admin-show-more =
    { $count ->
        [one] Afficher { $count } de plus
       *[other] Afficher { $count } de plus
    }

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Choisissez le protocole parlé par votre serveur. EWS vise Exchange 2019/2016/2013 auto-hébergé.

admin-sms-going-live = <strong>Avant la mise en production :</strong> restreignez les pays de destination dans votre passerelle (Twilio parle de Geo Permissions), gardez le compte prépayé sans rechargement automatique, et laissez le captcha actif. Ces trois mesures bornent ensemble le coût d'une tentative de SMS pumping.

troubleshoot-heading = Diagnostic des disponibilités

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = Le nom et l'identifiant sont obligatoires.
form-error-team-name-length = Le nom ne doit pas dépasser 255 caractères.
form-error-team-description-length = La description ne doit pas dépasser 5000 caractères.
form-error-slug-charset = L'identifiant ne peut contenir que des minuscules, des chiffres et des tirets.
form-error-slug-reserved = Cet identifiant est réservé. Veuillez en choisir un autre.
form-error-team-slug-taken = Une équipe utilise déjà cet identifiant.
form-error-title-required = Le titre est obligatoire pour générer un identifiant.
form-error-event-type-slug-taken = Un type d'événement utilise déjà cet identifiant.
form-error-event-type-slug-taken-team = Un type d'événement de cette équipe utilise déjà cet identifiant.
form-error-event-type-title-required = Le titre est obligatoire.
form-error-event-type-title-length = Le titre doit faire 255 caractères au plus.
form-error-event-type-duration = La durée doit être un nombre entier de minutes, de 1 à 1440 (24 heures).
form-error-event-type-number = Les marges, le préavis minimum, le rappel et les invités supplémentaires doivent être des nombres entiers, 0 ou plus.
form-error-event-type-horizon = L'horizon de réservation doit être un nombre entier de jours, 0 ou plus. Laissez-le vide pour ne fixer aucune limite.
form-error-location-required = Les détails du lieu sont obligatoires (lien de visioconférence, numéro de téléphone ou adresse).
form-error-bookable-range-order = La date de fin doit être identique ou postérieure à la date de début.
form-error-bookable-range-invalid = Saisissez les dates au format AAAA-MM-JJ.
form-error-not-team-admin = Vous n'êtes pas administrateur de cette équipe.
form-error-no-account = Aucun profil de planification trouvé. Veuillez contacter un administrateur.
form-error-all-fields-required = Tous les champs sont obligatoires.
form-error-encryption = Erreur de chiffrement.
form-error-connection-failed = Échec de la connexion : { $error }. Vérifiez l'URL et les identifiants, ou cochez « Ignorer le test de connexion » pour enregistrer malgré tout.

# Settings page flash (src/web/mod.rs)

settings-saved = Paramètres enregistrés.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = Le nom doit contenir entre 1 et 255 caractères.
settings-error-username-length = Le nom d'utilisateur doit contenir au moins 2 caractères.
settings-error-username-taken = Ce nom d'utilisateur est déjà pris.
settings-error-booking-email = Veuillez saisir une adresse e-mail de réservation valide.
settings-error-save-failed = Échec de l'enregistrement des paramètres.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = Équipe introuvable, ou vous n'en êtes pas administrateur.
error-team-not-found = Équipe introuvable.
error-event-type-not-found = Type d'événement introuvable.
error-decrypt-failed = Impossible de déchiffrer les identifiants enregistrés.
error-source-not-found = Source introuvable.
error-source-no-password = Aucun mot de passe enregistré pour cette source.
error-oauth-invalid-state = Paramètre d'état invalide. Veuillez réessayer.
error-oauth-no-code = Aucun code d'autorisation reçu.
error-oauth-not-configured = OAuth2 Google n'est pas configuré.
error-no-scheduling-account = Aucun profil de planification trouvé.
error-private-event-type-not-found = Type d'événement privé introuvable.
error-access-denied = Accès refusé.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = Ce créneau n'est plus disponible.
error-slot-too-soon = Ce créneau n'est plus disponible (trop proche).
error-slot-beyond-horizon = Ce créneau dépasse la période de réservation.
error-slot-not-yet-bookable = Ce créneau n'est pas encore ouvert à la réservation.
error-invite-required = Ce type d'événement nécessite un lien d'invitation.
error-invite-invalid = Lien d'invitation invalide.
error-invite-expired = Ce lien d'invitation a expiré.
error-invite-used = Ce lien d'invitation a déjà été utilisé.
error-invalid-date = Date invalide.
error-invalid-time = Heure invalide.
error-invalid-date-format = Format de date invalide.
error-invalid-time-format = Format d'heure invalide.
error-too-many-bookings = Trop de tentatives de réservation. Veuillez réessayer dans quelques minutes.
error-too-many-requests = Trop de requêtes. Veuillez réessayer plus tard.
error-no-members-available = Aucun membre de l'équipe n'est disponible pour ce créneau.
error-dynamic-group-public-only = Les liens de groupe dynamiques ne sont disponibles que pour les types d'événement publics.
error-user-not-found = Utilisateur introuvable.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Échec de la vérification captcha
bae-title-invalid-booking = Détails de réservation invalides
bae-title-unavailable = Indisponible pour le moment
bae-title-cannot-approve = Impossible d'approuver cette réservation
bae-title-invalid-link = Lien invalide
bae-title-invalid-or-expired = Lien invalide ou expiré
bae-title-booking-not-found = Réservation introuvable
bae-title-already-approved = Déjà approuvée
bae-title-already-declined = Déjà refusée
bae-title-already-cancelled = Déjà annulée
bae-title-booking-cancelled = Réservation annulée
bae-title-booking-declined = Réservation refusée

# Booking action error page: bodies

bae-body-go-back = Veuillez revenir en arrière et réessayer.
bae-body-unavailable = L'hôte n'accepte plus de réservations pour cette date. Veuillez choisir une autre date, ou revenir plus tard.
bae-body-resource-gone = Une ressource requise n'est plus disponible sur ce créneau. Demandez à l'invité d'en choisir un autre.
bae-body-no-claim-token = Aucun jeton de prise en charge fourni.
bae-body-claim-invalid = Ce lien de prise en charge n'est plus valide.
bae-body-booking-gone = Cette réservation n'existe plus.
bae-body-decline-link-invalid = Ce lien de refus est invalide, a expiré, ou la réservation a déjà été traitée.
bae-body-cancel-link-invalid = Ce lien d'annulation est invalide, a expiré, ou la réservation a déjà été annulée.
bae-body-cancel-link-invalid-short = Ce lien d'annulation est invalide ou a expiré.
bae-body-reschedule-link-invalid = Ce lien de reprogrammation est invalide, a expiré, ou la réservation a déjà été traitée.
bae-body-approval-link-invalid = Ce lien d'approbation est invalide ou a expiré.
bae-body-already-approved = Cette réservation a déjà été approuvée.
bae-body-already-declined = Cette réservation a déjà été refusée.
bae-body-already-cancelled = Cette réservation a déjà été annulée.
bae-body-was-cancelled = Cette réservation a été annulée.
bae-body-declined-by-host = Cette réservation a été refusée par l'hôte.

# Booking form validation (src/web/mod.rs)

validate-name-length = Le nom doit contenir entre 1 et 255 caractères.
validate-email-length = L'adresse e-mail doit contenir entre 1 et 255 caractères.
validate-email-invalid = Veuillez saisir une adresse e-mail valide.
validate-notes-length = Les notes ne doivent pas dépasser 5000 caractères.
validate-date-too-far = Impossible de réserver plus d'un an à l'avance.
validate-guest-footer-too-long = Le pied de page des e-mails ne doit pas dépasser 500 caractères.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = Les invités supplémentaires ne sont pas autorisés pour ce type d'événement.
guests-too-many =
    { $max ->
        [one] Vous pouvez ajouter au plus un invité supplémentaire.
       *[other] Vous pouvez ajouter au plus { $max } invités supplémentaires.
    }
guests-invalid-email = Adresse e-mail d'invité supplémentaire invalide : { $email }
dynamic-group-min-usernames = Les liens de groupe dynamique nécessitent au moins deux noms d'utilisateur.
dynamic-group-user-not-found = Utilisateur « { $username } » introuvable.
dynamic-group-user-opted-out = L'utilisateur « { $username } » n'a pas activé les liens de groupe dynamique.

error-slot-unavailable-member = Ce créneau n'est plus disponible ({ $username } a un conflit).

# Pending requests: slot hold, expiry and approval re-check (#234)

sms-expired = Demande expirée : { $event }, le { $date } à { $time } ({ $tz }). Pas de réponse à temps, merci de choisir un autre créneau.
email-pending-expired-status = Demande expirée
email-pending-expired-headline = Votre demande de réservation a expiré : l'hôte n'a pas répondu à temps. Merci de choisir un autre créneau.
email-pending-expired-action = Choisir un autre créneau
bae-body-host-busy = L'hôte (ou un membre requis de l'équipe) n'est plus disponible sur ce créneau. Demandez à l'invité d'en choisir un autre.
bae-body-host-not-synced = Le calendrier Microsoft d'un hôte n'a pas terminé sa première synchronisation : calrs ne peut pas savoir si ce créneau est libre. Réessayez dans une minute, ou consultez l'erreur de synchronisation de la source dans Sources d'agenda.
bookings-error-generic = Une erreur est survenue.
bookings-error-no-bookable-dates = Ce type d'événement n'a plus de dates réservables : l'invité ne pourrait pas choisir un nouvel horaire. Prolongez d'abord sa période de réservation.
bookings-error-reschedule-pending = L'invité a déjà été invité à choisir un nouvel horaire et n'a pas encore répondu.
bookings-error-reschedule-resend-too-soon = La demande a été envoyée à l'invité il y a moins d'une heure. Réessayez plus tard.
bookings-resend-reschedule = Renvoyer la demande
bookings-resend-reschedule-hint = Renvoyer le même lien à l'invité par e-mail (une fois par heure au plus)
troubleshoot-detail-pending-request = { $title } (demande en attente)
etf-pending-blocks-slot = Les demandes en attente bloquent le créneau
etf-pending-blocks-slot-help = Tant qu'une demande attend votre réponse, personne d'autre ne peut réserver ce créneau, sur ce type d'événement comme sur les autres.
etf-pending-expiry = Expiration des demandes
etf-pending-expiry-help = Une demande restée sans réponse à cette échéance est refusée automatiquement et l'invité est prié de choisir un autre créneau. Elle arrive plus tôt si la réunion devient trop proche pour qu'un autre invité puisse réserver le créneau. Vous recevez un rappel à mi-parcours.
etf-pending-expiry-never = Jamais

etf-pending-expiry-hours =
    { $hours ->
        [one] Après { $hours } heure
       *[other] Après { $hours } heures
    }

# Host, watcher and remaining guest emails (src/email.rs, #209)

email-footer-sent-by = Envoyé par
email-sender-via = { $host } via { $instance }
email-date-at-time = { $date } à { $time }
email-detail-phone = Téléphone :
email-detail-expires = Expiration :
email-detail-new-date = Nouvelle date :
email-detail-new-time = Nouvel horaire :
email-detail-previous = Précédemment :
email-detail-requested = Demandé :
email-detail-originally = Initialement :
email-detail-organizer = Organisateur :
email-detail-booked-by = Réservé par :
email-detail-invited-by = Invité par :
email-detail-message = Message :
email-action-approve = Approuver
email-action-decline = Refuser
email-action-pick-new-time = Choisir un nouveau créneau
email-host-approve-line = Approuver : { $url }
email-host-decline-line = Refuser : { $url }
email-host-approval-dashboard = Connectez-vous à votre tableau de bord pour confirmer ou refuser cette réservation.
email-host-new-subject = Nouvelle réservation : { $event }, { $guest } ({ $date })
email-host-new-heading = Nouvelle réservation !
email-host-new-message = { $guest } a réservé un créneau avec vous.
email-host-confirmed-subject = Confirmation : { $event }, { $guest } ({ $date })
email-host-confirmed-heading = Réservation confirmée
email-host-confirmed-message = Vous avez approuvé la réservation de { $guest }.
email-host-confirmed-footer = L'événement a été ajouté à votre agenda.
email-host-sync-failure-subject = Action requise : agenda non mis à jour pour { $event } ({ $date })
email-host-sync-failure-heading = Votre agenda n'a pas été mis à jour
email-host-sync-failure-intro = Cette réservation a été déplacée, mais calrs n'a pas pu mettre à jour l'événement dans votre agenda Google. L'invité a le nouvel horaire ; votre agenda affiche toujours l'ancien.
email-host-sync-failure-action = Ouvrez l'événement dans Google Agenda et déplacez-le à l'horaire ci-dessus. Déplacez l'événement existant plutôt que de le recréer, sinon le lien Google Meet dont dispose déjà l'invité cessera de fonctionner.
email-writeback-failure-subject = Action requise : { $event } ({ $date }) n'est pas dans votre agenda
email-writeback-failure-heading = Une réservation manque dans votre agenda
email-writeback-failure-intro = calrs n'a pas pu ajouter cette réservation à votre agenda, même après plusieurs tentatives. La réservation est confirmée et l'invité a reçu l'invitation.
email-writeback-failure-action = Ajoutez vous-même l'événement à votre agenda pour ne pas le manquer, et vérifiez la source d'agenda dans calrs. calrs réessaie encore pendant quelques heures ; vous ne recevrez plus cet e-mail pour cette réservation.
email-detail-error = Erreur :
email-source-failure-subject = Action requise : la source d'agenda « { $source } » ne se synchronise plus
email-source-failure-heading = Une source d'agenda ne se synchronise plus
email-source-failure-intro = calrs n'arrive plus à synchroniser cette source d'agenda depuis plus d'une heure. Vos pages de réservation proposent toujours des créneaux, mais ils reposent sur la dernière synchronisation réussie : les événements ajoutés ou déplacés depuis ne sont pas pris en compte.
email-source-failure-action = Vérifiez la source sur la page Sources d'agenda : un mot de passe expiré ou un accès révoqué en est la cause habituelle. Vous ne recevrez de nouvelles de cette source que si elle se rétablit puis échoue à nouveau.
email-detail-source = Source d'agenda :
email-detail-failing-since = En échec depuis :
email-host-reminder-subject = Rappel : { $event }, { $guest } ({ $date })
email-host-reminder-heading = Réservation à venir
email-host-reminder-message = Rappel : vous avez bientôt une réservation avec { $guest }.
email-host-cancel-subject = Annulation : { $event }, { $guest } ({ $date })
email-host-cancel-message-by-host = Vous avez annulé cette réservation.
email-host-cancel-message-by-guest = { $guest } a annulé sa réservation.
email-host-approval-subject = Action requise : { $event }, { $guest } ({ $date })
email-host-approval-heading = Action requise
email-host-approval-message = { $guest } souhaite réserver un créneau avec vous.
email-host-approval-reminder-subject = Rappel : { $guest } attend votre réponse ({ $event }, { $date })
email-host-approval-reminder-heading = Rappel : action requise
email-host-approval-reminder-message = { $guest } attend toujours votre réponse. Sans réponse de votre part, la demande sera automatiquement refusée le { $deadline }.
email-host-approval-footer = Vous pouvez aussi gérer cette demande depuis votre tableau de bord.
email-host-expired-subject = Expiration : { $event } avec { $guest } ({ $date })
email-host-expired-heading = Demande de réservation expirée
email-host-expired-message = Personne n'a répondu à la demande de { $guest } avant l'échéance : elle a été refusée automatiquement et l'invité a été prié de choisir un autre créneau.
email-host-expired-footer = Vous pouvez modifier le délai d'attente des demandes dans les paramètres du type d'événement.
email-host-reschedule-request-subject = Demande de reprogrammation : { $event }, { $guest } ({ $email })
email-host-reschedule-request-message = { $guest } souhaite reprogrammer sa réservation.
email-watcher-claim-subject = À prendre en charge : { $event }, { $guest } ({ $date })
email-watcher-claim-message = Une nouvelle réservation peut être prise en charge. Cliquez ci-dessous pour y participer.
email-watcher-claim-link = Prendre en charge cette réservation : { $url }
email-watcher-claim-footer = Vous pouvez aussi la prendre en charge depuis votre tableau de bord.
email-claim-confirmed-subject = Réservation prise en charge : { $event }, { $guest } ({ $date })
email-claim-confirmed-footer = Vous serez ajouté comme participant à cette réunion.
email-attendee-greeting = Bonjour,
email-attendee-message = Vous avez été ajouté comme participant à une réservation.
email-attendee-rescheduled-message = Une réservation à laquelle vous participez a été déplacée à un nouvel horaire.
email-attendee-cancelled-message = Une réservation à laquelle vous participez a été annulée.
email-attendee-cancelled-subject = Annulation : { $event }
email-original-booking-cancelled = Votre réservation initiale est également annulée.
email-pick-new-time-subject = Reprogrammation : { $event }, merci de choisir un nouveau créneau
email-pick-new-time-message = { $host } doit reprogrammer votre réservation. Merci de choisir un nouveau créneau.
email-pick-new-time-link = Choisir un nouveau créneau : { $url }
email-rescheduled-message = Votre réservation a été reprogrammée par { $host }.
email-reschedule-approved-message = { $host } a confirmé le nouvel horaire de votre réservation.
email-rescheduled-ics-attached-plain = Une invitation mise à jour est jointe.
email-rescheduled-ics-attached-html = Une invitation mise à jour est jointe à cet e-mail.
email-need-to-reschedule = Besoin de reprogrammer ? { $url }
email-invite-subject = { $host } vous invite à réserver : { $event }
email-invite-message = { $host } vous invite à réserver : { $event }
email-invite-link = Cliquez sur le lien ci-dessous pour choisir un créneau :
email-invite-action = Choisir un créneau
email-cancel-reason-calendar-deleted = L'événement a été supprimé de l'agenda de l'hôte.

# Embed snippet generator (templates/embed.html)
embed-page-title = Intégration — { $title }
embed-heading = Intégration : { $title }
embed-subtitle = Générez un extrait de code pour intégrer ce type d'événement sur votre propre site web.
embed-warn-internal = Attention : ce type d'événement est <strong>interne</strong>. Les visiteurs du site intégré auront besoin d'un lien d'invitation valide pour réserver — l'extrait généré n'en contient pas. Envisagez de le rendre public pour une intégration directe.
embed-warn-private = Attention : ce type d'événement est <strong>privé</strong>. Les visiteurs du site intégré auront besoin d'un lien d'invitation valide pour réserver — l'extrait généré n'en contient pas. Envisagez de le rendre public pour une intégration directe.
embed-warn-private-team = Attention : cette équipe est <strong>privée</strong>. Les visiteurs du site intégré ne peuvent pas réserver sans le lien d'invitation de l'équipe — l'extrait généré n'en contient pas. Envisagez de rendre l'équipe publique pour une intégration directe.
embed-need-username = Vous devez définir un <strong>nom d'utilisateur</strong> avant de pouvoir intégrer ce type d'événement — le lien d'intégration est { $path }. { $link }, puis revenez ici.
embed-need-username-link = Définissez un nom d'utilisateur dans les paramètres
embed-username-placeholder = nom-utilisateur
embed-tab-inline = Intégré à la page
embed-tab-floating = Bouton flottant
embed-tab-element = Clic sur un élément
embed-layout = Disposition
embed-theme = Thème
embed-theme-auto = Automatique (selon le visiteur)
embed-brand-color = Couleur de la marque
embed-use-org-color = Utiliser la couleur de l'organisation
embed-fix-time-format = Format horaire fixe (masque le choix 12h/24h)
embed-fixed-tz = Afficher les heures dans le fuseau horaire de l'événement (masque le sélecteur de fuseau horaire)
embed-fixed-tz-hint = Idéal pour les rendez-vous sur place. Les visiteurs voient l'heure locale de l'événement, avec le fuseau horaire indiqué.
embed-button-text = Texte du bouton
embed-button-default = Prendre rendez-vous
embed-button-position = Position du bouton
embed-pos-bottom-right = En bas à droite
embed-pos-bottom-left = En bas à gauche
embed-pos-top-right = En haut à droite
embed-pos-top-left = En haut à gauche
embed-button-colors = Couleurs du bouton
embed-button-fg-placeholder = texte
embed-button-colors-hint = Fond / texte
embed-show-icon = Afficher l'icône de calendrier
embed-element-help = Insérez l'extrait une seule fois dans votre site, puis ajoutez les attributs data indiqués à tout élément cliquable (bouton, lien, div) qui doit ouvrir la fenêtre de réservation.
embed-snippet = Extrait de code
embed-loading = Chargement…
embed-preview = Aperçu en direct
embed-preview-back = retour à l'aperçu
embed-preview-floating-hint = Cliquez sur le bouton pour simuler la fenêtre de réservation
embed-preview-element-hint = Déclencheurs de démonstration — sur votre site, ajoutez les attributs data de l'extrait à n'importe quel élément. Cliquez sur l'un d'eux pour essayer la fenêtre :
embed-preview-text-link = ou cliquez sur ce lien texte
embed-snippet-attr-comment = Ajoutez ces attributs à tout élément cliquable de votre page :

# Leftovers in partly translated pages (host reschedule, impersonation banner, slots, team settings)
host-resched-when = { $date } à { $start } – { $end }
base-impersonating = Vous avez pris l'identité de { $name }
slots-rescheduling-banner = Reprogrammation : { $title } (actuellement : { $date } à { $time })
slots-invite-required-title = La réservation nécessite un lien d'invitation
team-settings-slug-url = Utilisé dans l'URL de l'équipe : { $url }
team-event-type-no-teams = Aucune équipe n'existe pour le moment.
team-event-type-not-team-admin = Vous n'êtes administrateur d'aucune équipe.

# Google Calendar connection errors (src/web/sources.rs)
google-error-not-configured = L'intégration Google Agenda n'est pas configurée. Demandez à votre administrateur de renseigner les identifiants OAuth2 Google dans le panneau d'administration.
google-error-no-base-url = L'URL publique de base n'est pas configurée. Définissez-la via la variable d'environnement CALRS_BASE_URL ou dans le panneau d'administration, section Paramètres système, avant d'utiliser les flux OAuth2.
google-error-authorization = L'autorisation Google a échoué : { $error }
google-error-token-exchange = L'échange de jeton a échoué : { $error }
google-error-account-email = Impossible de récupérer l'adresse e-mail du compte Google : { $error }

# Admin panel notices (src/web/admin.rs)
admin-resource-feed-invalid = URL de flux invalide : { $error }
admin-resource-feed-check-failed = La vérification du flux a échoué : { $error }
admin-resource-caldav-invalid = URL CalDAV invalide : { $error }
admin-resource-credentials-failed = Impossible d'enregistrer les identifiants : { $error }
admin-resource-added = Ressource « { $name } » ajoutée. Événements en cache : { $count }.
admin-resource-updated = Ressource mise à jour. Événements en cache : { $count }.
admin-resource-deleted = Ressource supprimée.
admin-resource-not-found = Ressource introuvable.
admin-resource-synced = Flux synchronisé. Événements en cache : { $count }.
admin-resource-sync-error = La synchronisation a échoué : { $error }
admin-resource-no-caldav-url = Aucune URL CalDAV n'est configurée et aucune n'a pu être déduite de l'URL du flux.
admin-resource-no-write-credentials = Aucun identifiant d'écriture disponible (pas de compte de service, et aucun membre ayant accepté le prêt ne possède de source de calendrier correspondante).
admin-resource-write-passed = Test d'écriture réussi avec { $user } (événement créé : { $created }, supprimé : { $cleaned }).
admin-resource-write-verified = vérifié
admin-resource-write-unverified = non vérifié
admin-resource-write-check-calendar = vérifiez le calendrier de la ressource
admin-resource-write-failed = Le test d'écriture a échoué. Dernière erreur : { $error }
admin-base-url-invalid = L'URL de base doit être une URL http(s) absolue, par ex. https://cal.example.com
admin-company-link-invalid = Le lien de l'entreprise doit commencer par http:// ou https://
admin-smtp-host-required = L'hôte SMTP est obligatoire.
admin-smtp-from-invalid = Veuillez saisir une adresse e-mail d'expéditeur valide.
admin-smtp-port-invalid = Le port SMTP doit être un nombre compris entre 1 et 65535.
admin-smtp-tls-invalid = Le mode TLS doit être « starttls », « tls » ou « none ».
admin-smtp-password-required = Un mot de passe est requis lors de la première configuration SMTP.
admin-sms-switch-needs-secret = Pour changer de passerelle SMS, vous devez saisir l'identifiant secret de la nouvelle passerelle.
admin-sms-settings-invalid = Paramètres SMS : { $error }.
admin-sms-sender-format = Un expéditeur SMS numérique doit être au format international, par ex. +33612345678.
admin-sms-not-configured = Les SMS ne sont pas configurés.
admin-sms-load-failed = Impossible de charger la configuration SMS.
admin-sms-check-accepted = Identifiants acceptés par la passerelle.
admin-sms-bad-number = Ce numéro de téléphone n'est pas utilisable.
admin-sms-test-body = Message de test calrs. Votre passerelle SMS fonctionne.
admin-sms-test-accepted = Message de test accepté par la passerelle.

## Local multi-factor authentication

mfa-title = Authentification à deux facteurs
mfa-invalid = La vérification a échoué. Vérifiez votre mot de passe ou votre code. Chaque code ne fonctionne qu’une fois ; attendez un nouveau code si nécessaire. Après trop de tentatives, attendez 15 minutes. Si la configuration a expiré, recommencez.
mfa-restart = Retour à la connexion
mfa-manage = Gérer l’authentification à deux facteurs
mfa-recovery-help = Conservez ces codes de récupération en lieu sûr. Ils ne sont affichés qu’une fois. Chaque code remplace un code de votre application pour une seule utilisation. Les nouveaux codes remplacent tous les anciens.
mfa-continue = Continuer vers le tableau de bord
mfa-continue-consent = Continuer vers la page d'autorisation
mfa-enabled = L’authentification à deux facteurs est activée.
mfa-disabled = L’authentification à deux facteurs est désactivée.
mfa-required-help = Votre administrateur impose l’authentification à deux facteurs aux comptes locaux. Vous ne pouvez pas la désactiver.
mfa-code = Code de l’application ou code de récupération
mfa-regenerate = Générer de nouveaux codes de récupération
mfa-disable = Désactiver l’authentification à deux facteurs
mfa-enable = Configurer l’authentification à deux facteurs
mfa-changes-help = Les modifications nécessitent votre mot de passe. La désactivation ou le remplacement des codes de récupération nécessite aussi un code et déconnecte vos autres sessions.
mfa-back = Retour aux paramètres
mfa-setup-help = Scannez ce QR code avec votre application d’authentification, ou saisissez la clé ci-dessous. Saisissez un code à six chiffres pour terminer. La configuration expire après 10 minutes.
mfa-qr-alt = QR code de configuration de l’application
mfa-login-help = Saisissez un code de votre application d’authentification ou un de vos codes de récupération. Cette connexion expire après 10 minutes.
mfa-totp-code = Code à six chiffres de l’application
mfa-verify = Vérifier
mfa-policy-help = Cette règle concerne les comptes locaux. Le SSO utilise l’authentification multifacteur du fournisseur d’identité. Les administrateurs locaux doivent configurer la leur avant de modifier cette règle. La rendre obligatoire déconnecte les sessions locales sans validation multifacteur et impose la configuration à la prochaine connexion.
mfa-policy = Règle MFA des comptes locaux
mfa-optional = Facultative
mfa-required = Obligatoire
mfa-save-policy = Enregistrer la règle MFA

# First day of the week (#330)
week-start-label = Premier jour de la semaine
week-start-inherit-site = Utiliser le réglage du site
week-start-inherit-owner = Utiliser le réglage du propriétaire

## Booking directory and personal favorites

directory-title = Annuaire des rendez-vous
directory-help = Trouvez le lien de rendez-vous d’un collègue ou d’une équipe. Ajoutez les rendez-vous que vous partagez souvent aux favoris pour les retrouver sur votre tableau de bord.
directory-search = Rechercher par personne, équipe ou titre de rendez-vous
directory-find = Rechercher
directory-favorites-only = Favoris uniquement
directory-favorites = Rendez-vous favoris
directory-browse = Parcourir l’annuaire
directory-generate = Générer un lien d’invitation
directory-invite-help = Invitation à usage unique, valable 7 jours.
directory-add = Ajouter { $title } aux favoris
directory-remove = Retirer { $title } des favoris
directory-added = Ajouté aux favoris.
directory-removed = Retiré des favoris.
directory-error = Cette action n’a pas pu être effectuée. Actualisez la page et réessayez.
directory-manual-copy = Copiez ce lien manuellement :
directory-empty = Aucun rendez-vous trouvé. Essayez une autre recherche.
directory-favorites-empty = Aucun favori trouvé. Ajoutez des rendez-vous aux favoris depuis l’annuaire pour les retrouver ici.
directory-with-hosts = Avec { $names }

microsoft-calendar-title = Calendrier Microsoft 365 / Outlook.com
microsoft-calendar-help = Connectez votre calendrier Microsoft avec un consentement délégué. Votre compte Microsoft peut différer du compte de connexion calrs. Pour Exchange sur site, utilisez EWS ci-dessous.
microsoft-calendar-connect = Connecter le calendrier Microsoft
microsoft-calendar-unconfigured = Un administrateur doit d'abord configurer l'application Microsoft.
microsoft-admin-help = Enregistrez une application Web avec cette URI de redirection exacte. Demandez les autorisations déléguées Calendars.ReadWrite, User.Read et offline_access, pas des autorisations d'application. Chaque utilisateur connecte son propre compte. Aucun flux implicite ni client public n'est nécessaire.
microsoft-tenant-label = Locataire Microsoft
microsoft-tenant-help = common autorise délibérément les comptes professionnels, scolaires et personnels si l'application prend en charge les deux catégories. Utilisez organizations pour les comptes professionnels/scolaires, consumers pour les comptes personnels, ou un UUID de locataire / domaine vérifié pour restreindre l'accès.
microsoft-admin-save = Enregistrer les paramètres Microsoft
microsoft-error-impersonation = Le calendrier Microsoft ne peut pas être connecté lorsque vous utilisez l'identité d'un autre utilisateur.
microsoft-error-start = Impossible de démarrer l'autorisation Microsoft. Vérifiez la configuration de l'application, l'URL publique HTTPS et que cette source appartient à votre compte.
microsoft-error-state = L'état d'autorisation Microsoft est invalide, expiré ou déjà utilisé, ou votre session a changé. Recommencez la connexion.
microsoft-error-consent = L'autorisation Microsoft n'a pas abouti. Recommencez et autorisez la lecture et l'écriture du calendrier, le profil et l'accès hors ligne.
microsoft-error-connect = Impossible de connecter le calendrier Microsoft. Vérifiez les paramètres et les autorisations. Pour reconnecter la source, choisissez le compte Microsoft initial ; pour un autre compte, ajoutez une source.
microsoft-error-sync = Microsoft est connecté, mais la synchronisation a échoué. Vérifiez que le compte possède un calendrier Exchange Online ou Outlook.com. Revenez aux sources d'agenda pour réessayer ou reconnecter le compte.
microsoft-error-selection = Microsoft est connecté, mais la sélection du calendrier n'a pas pu être chargée. Revenez aux sources d'agenda.
microsoft-error-settings = Les paramètres Microsoft n'ont pas été enregistrés. Vérifiez l'UUID du client, le locataire et l'accès à la base de données. Un secret vide conserve le secret existant.
microsoft-error-heading = Calendrier Microsoft non connecté
microsoft-error-settings-heading = Paramètres Microsoft non enregistrés
microsoft-error-back-admin = Retour au panneau d'administration
source-test-microsoft-ok = Connexion au calendrier Microsoft réussie.
source-test-microsoft-failed = La connexion au calendrier Microsoft a échoué. Vérifiez l'accès au calendrier ou reconnectez la source.
source-test-microsoft-credentials = Les identifiants du calendrier Microsoft n'ont pas pu être chargés. Reconnectez votre compte.
microsoft-callback-needs-https = Définissez comme URL publique de base une origine HTTPS, sans chemin, pour afficher l'URL de rappel à enregistrer.
source-form-backend-ews = Microsoft Exchange sur site (EWS)

# Global Exchange (EWS) impersonation: admin card and managed sources
sources-admin-managed = Géré par l'admin
sources-admin-managed-title = Provisionné par l'administrateur
sources-locked-notice = Votre administrateur gère Exchange de façon centralisée. L'ajout d'autres sources de calendrier est désactivé.
sources-locked-by-admin = L'ajout de sources de calendrier personnelles est désactivé par l'administrateur. Votre calendrier Exchange est géré de façon centralisée.
sources-managed-no-edit = Cette source de calendrier est provisionnée par l'administrateur et ne peut pas être modifiée.
sources-managed-no-remove = Cette source de calendrier est provisionnée par l'administrateur et ne peut pas être supprimée.
sources-managed-global-disabled = Cette source est gérée par l'administrateur, mais Exchange global est désactivé.
admin-ews-title = Exchange (EWS) : usurpation d'identité globale
admin-ews-help = Configurez un seul point d'accès Exchange Web Services et un compte de service disposant du rôle RBAC <code>ApplicationImpersonation</code>. Le calendrier de chaque utilisateur est alors lu par usurpation d'identité, sans qu'il ait à saisir d'identifiants. PowerShell sur Exchange : <code>New-ManagementRoleAssignment -Name CalrsImpersonation -Role ApplicationImpersonation -User svc-calrs@votredomaine</code>.
admin-ews-endpoint = Point d'accès
admin-ews-managed-count = Sources gérées :
admin-ews-enable = Activer Exchange global par usurpation d'identité
admin-ews-url = URL du point d'accès EWS
admin-ews-username = Nom d'utilisateur du compte de service
admin-ews-password = Mot de passe du compte de service
admin-ews-password-keep = (laisser vide pour conserver l'actuel)
admin-ews-password-unchanged = (inchangé)
admin-ews-password-placeholder = mot de passe du compte de service
admin-ews-domain = Domaine des boîtes
admin-ews-optional = (facultatif)
admin-ews-domain-help = Renseignez les deux lorsque les adresses connues de calrs (UPN AD, e-mail OIDC) sont sur un autre domaine que les boîtes Exchange. Seule une adresse dans exactement le domaine des utilisateurs est réécrite, en gardant sa partie locale : <code>alice@example.local</code> → <code>alice@example.com</code>. Les adresses de tout autre domaine sont utilisées telles quelles. Laissez les deux vides pour usurper l'adresse calrs de chaque utilisateur telle quelle.
admin-ews-auto = Provisionner pour tous les utilisateurs (existants + futurs)
admin-ews-auto-help = Si cochée, chaque utilisateur actif dont l'adresse est de confiance reçoit une source Exchange gérée : comptes créés avec calrs user create, comptes SSO dont le fournisseur d'identité a vérifié l'e-mail, et adresses confirmées ci-dessous. L'enregistrement lance immédiatement le traitement.
admin-ews-lock = Empêcher les utilisateurs standard d'ajouter leurs propres sources de calendrier
admin-ews-lock-help = Les administrateurs ne sont pas concernés. Les sources déjà ajoutées par les utilisateurs restent jusqu'à leur suppression.
admin-ews-disable-note = Tant que la fonctionnalité est désactivée, les sources gérées sont conservées mais ne sont plus synchronisées, et aucune réservation n'y est écrite. La réactiver les relance.
admin-ews-remove = Supprimer les sources gérées et effacer le mot de passe du compte de service
admin-ews-remove-confirm = Supprimer toutes les sources Exchange gérées, avec les calendriers et les plages occupées que calrs en a gardés, ainsi que le mot de passe du compte de service enregistré ? Exchange global est désactivé. Les réservations déjà écrites dans les boîtes aux lettres y restent.
admin-ews-notice-removed = Sources gérées supprimées : { $count }. Le mot de passe du compte de service est effacé et Exchange global est désactivé. Les réservations déjà écrites dans les boîtes aux lettres y restent.
admin-ews-notice-https-required = Le point de terminaison EWS doit utiliser https:// : le mot de passe du compte de service est envoyé à chaque requête. Rien n'a été enregistré.
admin-ews-notice-password-cleared = Paramètres enregistrés. Le point de terminaison ou le compte de service a changé, le mot de passe enregistré a donc été effacé : il doit être saisi à nouveau pour qu'Exchange global fonctionne.
admin-ews-notice-source-removed = Source gérée supprimée, avec ses calendriers et plages occupées en cache.
admin-ews-row-remove = Supprimer
admin-ews-row-remove-confirm = Supprimer la source Exchange gérée de { $email }, avec ses calendriers et plages occupées en cache ? Le provisionnement automatique la recrée au prochain enregistrement, à la prochaine confirmation ou connexion SSO ; suspendre le compte l'en empêche.
admin-ews-trust-no-mailbox = aucune (l'adresse ne désigne aucune boîte aux lettres)
admin-ews-notice-selections-reset = La boîte aux lettres lue par certaines sources gérées a changé, leurs calendriers sont donc redécouverts. Types d'événement dont la sélection de calendriers a été réinitialisée : { $count } (liste dans le journal du serveur).
admin-ews-notice-reconfirm = Le remplacement de domaine associe désormais certaines adresses confirmées à une autre boîte aux lettres. Adresses à confirmer de nouveau : { $count } (voir la liste ci-dessous).
admin-ews-row-taken-over = Reprise de { $user } le { $when }
admin-ews-notice-trust-changed = Rien n'a été confirmé : l'adresse de ce compte, ou la boîte aux lettres qu'elle ouvre, a changé depuis l'affichage de la page. Vérifiez de nouveau la liste.
sources-managed-user-untrusted = Cette source est gérée par l'administrateur, mais l'adresse du compte n'est pas encore confirmée : elle n'est donc pas synchronisée.
sources-managed-user-suspended = Cette source est gérée par l'administrateur, mais le compte est désactivé : elle n'est donc pas synchronisée.
admin-ews-save = Enregistrer les paramètres EWS
admin-ews-save-provision = Enregistrer &amp; provisionner tous les utilisateurs maintenant
admin-ews-status-title = État de synchronisation par utilisateur
admin-ews-status-summary = { $ok } boîtes gérées sur { $total } se synchronisent. Un utilisateur sans boîte Exchange (par ex. un compte admin local) affichera toujours une erreur. C'est normal.
admin-ews-col-user = Utilisateur
admin-ews-col-mailbox = Boîte aux lettres
admin-ews-col-last-sync = Dernière synchro
admin-ews-col-status = Statut
admin-ews-row-disabled = désactivé
admin-ews-row-synced = Synchronisé
admin-ews-row-never = Jamais synchronisé
admin-ews-row-no-mailbox = Aucune boîte Exchange
admin-ews-notice-saved = Paramètres Exchange enregistrés.
admin-ews-notice-provisioned = Paramètres Exchange enregistrés. Sources gérées créées : { $count }.
admin-ews-notice-invalid-url = L'URL du point d'accès EWS n'est pas valide (http(s) uniquement, pas d'adresse privée hors liste autorisée). Rien n'a été enregistré.
admin-ews-notice-incomplete = Pour activer Exchange global, renseignez l'URL du point d'accès, le compte de service et son mot de passe. Rien n'a été enregistré.
admin-ews-notice-provision-failed = Paramètres enregistrés, mais le provisionnement des sources gérées a échoué. Consultez le journal du serveur.
admin-ews-registration-warning = L'inscription libre est ouverte. Les comptes créés ainsi ne reçoivent jamais de source Exchange gérée, pas même avec « provisionner tous les utilisateurs maintenant » : rien ne prouve qu'ils possèdent leur adresse. Pour donner une boîte à l'un d'eux, confirmez son adresse ci-dessous, ou lancez calrs user trust-email, une fois certain qu'elle appartient à cette personne.
admin-ews-from-domain = Domaine des utilisateurs
admin-ews-notice-override-domains = Le remplacement de domaine a besoin des deux domaines, chacun un simple domaine comme example.com : celui avec lequel vos utilisateurs se connectent et celui de leurs boîtes. Rien n'a été enregistré.
admin-ews-notice-trusted = Adresse confirmée. Tant qu'Exchange global est actif, la source gérée de l'utilisateur est créée maintenant : voir le tableau d'état.
admin-ews-row-suspended = Compte désactivé : non synchronisé
admin-ews-row-untrusted = Adresse non confirmée : non synchronisée
admin-ews-row-conflict = Non provisionné : la source d'un autre utilisateur lit déjà cette boîte
admin-ews-trust-title = Adresses à confirmer
admin-ews-trust-help = Ces comptes actifs des domaines autorisés ont une adresse que personne n'a garantie (inscription libre, ou création avant cette vérification) : ils ne reçoivent donc aucune source Exchange gérée. Ne confirmez une adresse que si vous savez qu'elle appartient à cette personne : la confirmer permet à calrs de lire pour elle le calendrier de cette boîte.
admin-ews-trust-confirm = Confirmer
admin-ews-search = Rechercher par utilisateur ou boîte aux lettres
admin-ews-no-match = Aucune boîte gérée ni adresse à confirmer ne correspond à cette recherche.
admin-ews-conflicts-more = Boîte aux lettres déjà utilisée par un autre utilisateur : { $shown } affichées sur { $total }.
admin-ews-http-warning = Le point de terminaison utilise http:// : le mot de passe du compte de service circule en clair. Préférez https://.
# Time off (templates/time_off.html, src/web/time_off.rs, troubleshoot view)

nav-time-off = Absences
time-off-page-title = Absences
time-off-heading = Absences
time-off-subtitle = Les périodes où vous ne prenez aucune réservation. Tous les types d'événement que vous proposez les suivent, personnels comme d'équipe. Les invités voient seulement que ces horaires ne sont pas disponibles.
time-off-upcoming = Prévues
time-off-empty = Aucune absence prévue.
time-off-kept-notice = Les réservations déjà faites pendant une période sont conservées : ajouter une absence n'annule rien. Annulez-les ou déplacez-les depuis Réservations si nécessaire.
time-off-pending-notice = Les demandes en attente de votre validation pendant une période ne peuvent plus être acceptées : refusez-les, ou supprimez d'abord l'absence.
time-off-in-progress = En cours
time-off-entered-in = Saisie en { $timezone }
time-off-delete = Supprimer
time-off-delete-confirm = Supprimer cette absence ? Ses horaires redeviennent réservables.
time-off-add-heading = Ajouter une absence
time-off-from = Du
time-off-to = Au
time-off-time-optional = Heure (facultative)
time-off-times-help = Laissez les heures vides pour bloquer des journées entières, les deux dates comprises. Les heures sont dans votre fuseau horaire, { $timezone }.
time-off-label = Note (facultative)
time-off-label-help = Elle n'est visible que par vous, ici et dans Diagnostic. Les invités et les membres de vos équipes ne la voient jamais.
time-off-team-admins-notice = Les administrateurs de vos équipes et ceux de l'instance voient les dates et heures de vos absences, jamais la note.
time-off-add-submit = Ajouter l'absence
time-off-added = Absence ajoutée.
time-off-deleted = Absence supprimée.
time-off-range = Du { $start } au { $end }
time-off-at = { $date } à { $time }
time-off-same-day = { $date }, de { $start } à { $end }
time-off-from-time = { $date }, à partir de { $time }
time-off-until-time = { $date }, jusqu'à { $time }
time-off-impersonating = Vous consultez cette page en tant que { $name }. Les absences sont ici en lecture seule pour vous, et les notes sont masquées.
time-off-error-date = Saisissez les dates au format AAAA-MM-JJ et les heures au format HH:MM.
time-off-error-order = La fin doit être après le début.
time-off-error-too-long = Une période ne peut pas dépasser un an.
time-off-error-past = Cette période est déjà terminée.
time-off-error-label = La note ne peut pas dépasser 200 caractères.
time-off-error-too-many = Vous avez déjà 100 périodes prévues. Supprimez-en avant d'en ajouter d'autres.
time-off-hint = Pour bloquer des dates sur tous les types d'événement à la fois, utilisez { $link }.
troubleshoot-label-time-off = Absence
troubleshoot-legend-time-off = Absence

directory-all-favorites = Tous les favoris
pager-label = Pages
pager-range = { $from } à { $to } sur { $total }
pager-previous = Précédent
pager-next = Suivant
admin-users-search = Rechercher
admin-figures-cached = Chiffres des réservations calculés il y a { $seconds } s.

# OAuth connection consent (templates/auth/consent.html)
oauth-title = Connecter une application
oauth-intro = Autoriser cette application à accéder à calrs en tant que { $email } ? Le nom de l'application est fourni par son développeur. Vérifiez l'adresse de retour avant d'autoriser l'accès.
oauth-events = Consulter les types d'événements auxquels vous avez accès
oauth-blocks = Consulter vos plages occupées
oauth-invites = Créer des liens d'invitation avec vos autorisations
oauth-allow = Autoriser l'accès
oauth-deny = Refuser

# Connected apps (templates/dashboard_apps.html, admin user menu)

apps-page-title = Applications connectées
apps-heading = Applications connectées
apps-subtitle = Les applications que vous avez autorisées à agir en votre nom, comme les assistants IA connectés par MCP. En révoquer une coupe son accès immédiatement.
apps-approved = Autorisée le { $date }
apps-last-used = Dernière utilisation le { $date }
apps-never-used = Pas encore utilisée
apps-revoke = Révoquer
apps-revoke-confirm = Révoquer l'accès de { $name } ? L'application devra être autorisée à nouveau pour se reconnecter.
apps-revoked = Accès révoqué.
apps-empty = Aucune application connectée. Un client MCP demande votre autorisation la première fois qu'il se connecte.
oauth-slots = Lire les créneaux réservables des types d'événement que vous gérez
oauth-bookings-read = Lire les réservations dont vous êtes l'hôte, avec les coordonnées des invités
oauth-bookings-write = Réserver, annuler et reprogrammer sur les types d'événement que vous gérez, en écrivant aux invités
oauth-api-closed = Cette application demande à utiliser l'API REST, réservée aux administrateurs sur cette instance. Elle ne peut pas être autorisée.
api-tokens-page-title = Jetons d'API
api-tokens-heading = Jetons d'API
api-tokens-subtitle = Des jetons qui permettent à vos propres scripts d'appeler l'API REST de calrs en votre nom. Un jeton fonctionne jusqu'à son expiration ou sa révocation.
api-tokens-not-allowed = Seuls les administrateurs peuvent créer des jetons d'API sur cette instance.
api-tokens-new = Nouveau jeton
api-tokens-name = Nom
api-tokens-name-hint = À quoi il sert, par exemple Synchronisation CRM
api-tokens-scopes = Autorisations
api-tokens-expiry = Expiration
api-tokens-expiry-7 = Dans 7 jours
api-tokens-expiry-30 = Dans 30 jours
api-tokens-expiry-90 = Dans 90 jours
api-tokens-expiry-365 = Dans un an
api-tokens-expiry-never = Jamais
api-tokens-create = Créer le jeton
api-tokens-created = Copiez ce jeton maintenant : il ne sera plus affiché.
api-tokens-created-on = Créé le { $date }
api-tokens-expires-on = Expire le { $date }
api-tokens-never-expires = N'expire jamais
api-tokens-revoke-confirm = Révoquer { $name } ? Les scripts qui l'utilisent cessent aussitôt de fonctionner.
api-tokens-revoked = Jeton révoqué.
api-tokens-empty = Aucun jeton d'API pour l'instant.
api-tokens-error-name = Donnez au jeton un nom de 100 caractères au plus.
api-tokens-error-scope = Choisissez au moins une autorisation.
api-tokens-error-limit = Vous avez déjà 20 jetons. Révoquez-en un d'abord.
api-tokens-docs = L'utilisation d'un jeton est décrite dans le chapitre API REST de la documentation de calrs.
api-docs-title = Référence de l'API
api-docs-subtitle = Tous les points d'accès de l'API REST de calrs, décrits par cette instance elle-même.
api-docs-closed = Sur cette instance, seuls les administrateurs peuvent utiliser l'API REST.
api-docs-try = Pour essayer une requête, choisissez Authorize et collez un jeton d'accès personnel. Cette page n'envoie jamais votre session à l'API.
api-docs-tokens = Jetons d'API
api-docs-spec = Document OpenAPI (JSON)
api-docs-guide = Guide de l'API REST
api-docs-noscript = La référence interactive nécessite JavaScript. Le document OpenAPI décrit la même API.
admin-api-tokens = Jetons d'API
admin-api-tokens-allow-all = Permettre à tous les utilisateurs d'utiliser l'API REST
admin-api-tokens-allow-all-help = Désactivé par défaut : seuls les administrateurs peuvent créer des jetons d'accès personnels ou connecter des applications à l'API REST par OAuth, car les deux lisent les données des invités. Le désactiver suspend les jetons et les applications d'API des autres utilisateurs jusqu'à ce qu'il soit réactivé. Les clients MCP ne sont pas concernés.
admin-revoke-apps = Révoquer les applications connectées ({ $count })

# Write destination per event type (#47)
etf-write-destination = Écrire les réservations confirmées dans
etf-write-destination-default = Par défaut (réglage de la source)
etf-write-destination-help = Pour ce type d'événement uniquement. Par défaut, elles sont écrites dans l'agenda d'écriture de chacune de vos sources. Sur un type d'événement d'équipe, ce choix ne concerne que les réservations dont vous êtes l'hôte.
etf-write-destination-team = Chaque membre choisit où sont écrites les réservations dont il est l'hôte, dans { $link }.
sources-destinations-heading = Réservations par type d'événement
sources-destinations-help = Choisissez l'agenda dans lequel sont écrites les réservations confirmées de chaque type d'événement. Sur un type d'événement d'équipe, ce choix concerne les réservations dont vous êtes l'hôte ; les autres membres choisissent le leur. Les réservations déjà écrites suivent à leur prochaine écriture : report, approbation, nouvelle tentative ou prise en charge.
sources-destinations-saved = Enregistré. Les nouvelles réservations vont dans les agendas choisis ici.
write-destination-not-yours = Rien n'a été enregistré : choisissez l'un de vos propres agendas, pour un type d'événement dont vous êtes l'hôte.
write-destination-unavailable = { $name } (indisponible)
write-destination-refused = Cet agenda a refusé la dernière réservation ({ $error }), qui a été écrite selon le réglage de la source. La prochaine réservation le réessaiera.
# Booking webhooks (templates/webhooks.html, admin card, #168)
webhooks-title = Webhooks
webhooks-back = Retour aux types d'événement
webhooks-subtitle = calrs envoie un message JSON signé à ces adresses lorsqu'une réservation est faite, demandée, approuvée, refusée, expire, est annulée ou déplacée.
webhooks-not-allowed = Les webhooks sont réservés aux administrateurs sur cette instance.
webhooks-endpoints = Points de terminaison
webhooks-empty = Aucun webhook pour l'instant. Ajoutez-en un ci-dessous pour que calrs informe un autre système de vos réservations.
webhooks-scope = Envoyer les événements de
webhooks-scope-personal = Tous mes types d'événement personnels
webhooks-scope-event-type = Type d'événement : { $title }
webhooks-scope-team-event-type = Type d'événement : { $title } ({ $team })
webhooks-added-by = Ajouté par { $name }
webhooks-state-off = Désactivé
webhooks-never-sent = Rien n'a encore été envoyé
webhooks-state-ok = Dernier envoi réussi
webhooks-state-failing = Dernier envoi en échec
webhooks-queued =
    { $count ->
        [one] 1 envoi en attente d'une nouvelle tentative
       *[other] { $count } envois en attente d'une nouvelle tentative
    }
webhooks-gave-up =
    { $count ->
        [one] 1 envoi abandonné après toutes les tentatives
       *[other] { $count } envois abandonnés après toutes les tentatives
    }
webhooks-send-test = Envoyer un test
webhooks-enable = Activer
webhooks-disable = Désactiver
webhooks-delete = Supprimer
webhooks-delete-confirm = Supprimer ce webhook ? Les envois encore en attente sont abandonnés.
webhooks-edit = Modifier
webhooks-url = URL
webhooks-url-hint = (https)
webhooks-secret = Secret de signature
webhooks-secret-hint = (au moins { $min } caractères, signe chaque requête, n'est plus jamais affiché)
webhooks-save = Enregistrer
webhooks-add = Ajouter un webhook
webhooks-add-submit = Ajouter le webhook
webhooks-privacy-note = Chaque message contient le nom, l'e-mail, le numéro de téléphone et les notes de l'invité. Ne l'envoyez qu'à un système à qui vous confiez ces données.
webhooks-notice-created = Webhook ajouté. Utilisez « Envoyer un test » pour vérifier qu'il arrive.
webhooks-notice-updated = Webhook enregistré.
webhooks-notice-deleted = Webhook supprimé.
webhooks-notice-enabled = Webhook activé.
webhooks-notice-disabled = Webhook désactivé. Les envois encore en attente ont été abandonnés.
webhooks-notice-test-ok = Test livré : le point de terminaison a répondu avec un statut 2xx.
webhooks-error-url = Saisissez une URL complète, par exemple https://hooks.example.com/calrs.
webhooks-error-https = L'URL doit commencer par https://. Le http simple n'est possible que pour un hôte de la liste des hôtes privés autorisés.
webhooks-error-credentials = Retirez le nom d'utilisateur et le mot de passe de l'URL : le secret de signature authentifie calrs.
webhooks-error-secret = Le secret de signature doit compter entre 16 et 256 caractères.
webhooks-error-scope = Vous ne pouvez pas ajouter de webhook à ce type d'événement.
webhooks-error-limit = Cette portée compte déjà le maximum de 10 webhooks.
webhooks-error-test = Le test n'a pas été livré. La raison est affichée avec le webhook.
admin-webhooks = Webhooks de réservation
admin-webhooks-help =
    { $count ->
        [one] 1 webhook est configuré sur cette instance.
       *[other] { $count } webhooks sont configurés sur cette instance.
    }
admin-webhooks-allow-all = Autoriser tout utilisateur à ajouter des webhooks
admin-webhooks-allow-all-help = Désactivé par défaut : un webhook envoie le nom, l'e-mail et le téléphone des invités à une adresse choisie par l'utilisateur, seuls les administrateurs peuvent donc en ajouter. Désactiver cette option arrête les webhooks ajoutés par les autres utilisateurs.
admin-webhooks-manage = Gérer vos webhooks
webhooks-events = Événements
webhooks-events-all = Tous les événements
webhooks-event-created = Réservé (confirmé)
webhooks-event-requested = Demandé (en attente d'approbation)
webhooks-event-confirmed = Approuvé
webhooks-event-declined = Refusé
webhooks-event-expired = Expiré sans réponse
webhooks-event-cancelled = Annulé
webhooks-event-rescheduled = Déplacé
webhooks-error-events = Choisissez au moins un événement.
webhooks-error-test-limit = Trop de tests en peu de temps. Patientez quelques minutes avant de réessayer.
webhooks-added-by-former = Ajouté par un ancien membre
webhooks-signature-help = Chaque requête est signée : <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> est un HMAC-SHA256 du corps brut, avec le secret de signature pour clé. Le contenu et les vérifications à faire côté récepteur sont décrits à la page Webhooks de la documentation de calrs.
webhooks-state-suspended = Suspendu : la personne qui l'a ajouté ne peut plus gérer ce type d'événement. Reprenez-le pour le réactiver.
webhooks-adopt = Reprendre
webhooks-notice-adopted = Webhook repris et réactivé.
webhooks-event-claimed = Prise par un observateur
webhooks-error-address = Cette adresse ne peut pas être utilisée : calrs n'envoie qu'à des adresses publiques d'Internet, sauf si l'hôte figure dans la liste des hôtes privés autorisés.
webhooks-adopt-help = Le reprendre en fait le vôtre et le relance. Confirmez l'adresse à laquelle il envoie et définissez un nouveau secret de signature : la personne qui l'a ajouté connaît peut-être l'ancien. Les envois encore en attente sont abandonnés.
webhooks-error-not-suspended = Seul un webhook suspendu peut être repris.
webhooks-error-new-secret = L'URL pointe maintenant vers un autre hôte : saisissez aussi un nouveau secret de signature.
webhooks-url-masked-hint = (seule la personne qui l'a ajouté voit l'URL complète : saisissez-la à nouveau pour enregistrer)

# Booking questions (#210): booking form, answers, event type editor

book-question-choose = Choisissez une option
book-question-pick-one = Cochez au moins une case.
book-question-read-policy = Lire la politique
book-question-new-tab = (s'ouvre dans un nouvel onglet)
answer-yes = Oui
answer-no = Non
answer-agreed = Accepté
answer-not-agreed = Non accepté
answer-you-agreed = Vous avez accepté
answer-you-did-not-agree = Vous n'avez pas accepté
answer-agreed-recorded-by = Accepté, enregistré par { $name }
answer-recorded-for-you-by = Enregistré en votre nom par { $name }
bookings-answers = Réponses
validate-notes-required = Veuillez ajouter une note pour l'hôte.
validate-question-required = Veuillez répondre à « { $question } ».
validate-question-too-long = La réponse à « { $question } » est trop longue.
validate-question-unknown-option = La réponse à « { $question } » ne fait pas partie des choix proposés.
validate-questions-invalid = Les questions de réservation n'ont pas pu être lues. Rechargez la page et réessayez.
validate-questions-too-many = Un type de rendez-vous peut poser au plus { $max } questions de réservation.
validate-question-kind = Question { $position } : ce type de question n'est pas disponible.
validate-question-kind-change = Question { $position } : une question enregistrée garde son type. Supprimez-la et ajoutez-en une nouvelle.
validate-question-label-empty = Question { $position } : la question est vide.
validate-question-label-too-long = Question { $position } : la question est trop longue (200 caractères, 1000 pour une case à cocher).
validate-question-help-too-long = Question { $position } : le texte d'aide est trop long (500 caractères au plus).
validate-question-options-few = Question { $position } : indiquez au moins deux choix.
validate-question-options-many = Question { $position } : 30 choix au plus.
validate-question-option-empty = Question { $position } : un choix est vide.
validate-question-option-too-long = Question { $position } : un choix est trop long (200 caractères au plus).
validate-question-option-duplicate = Question { $position } : deux choix ont le même texte.
validate-question-link = Question { $position } : le lien vers la politique doit être une adresse complète commençant par https:// ou http://.
etf-section-booking-form = Formulaire de réservation
etf-booking-form-help = Les invités indiquent toujours leur nom et leur e-mail. Ajoutez les questions dont ce type de rendez-vous a besoin : les invités y répondent en réservant, et les réponses vous parviennent avec la réservation.
etf-notes-mode = Champ de notes
etf-notes-mode-off = Masqué
etf-notes-mode-optional = Facultatif
etf-notes-mode-required = Obligatoire
etf-q-empty = Aucune question pour l'instant.
etf-q-add = Ajouter une question
etf-q-kind = Type
etf-q-kind-short-text = Texte court
etf-q-kind-long-text = Texte long
etf-q-kind-single-choice = Choix unique
etf-q-kind-multiple-choice = Choix multiple
etf-q-kind-checkbox = Case à cocher
etf-q-label = Question
etf-q-help = Texte d'aide
etf-q-required = Obligatoire
etf-q-options = Choix
etf-q-option = Choix %1
etf-q-add-option = Ajouter un choix
etf-q-remove-option = Supprimer ce choix
etf-q-consent = Cette case recueille un consentement (politique de confidentialité, enregistrement)
etf-q-link = Lien vers la politique
etf-q-link-help = Facultatif. Affiché sous la case, s'ouvre dans un nouvel onglet.
etf-q-in-calendar = Inscrire la réponse dans l'événement du calendrier
etf-q-in-calendar-help = Désactivez pour les réponses sensibles : toute personne qui peut lire votre calendrier les verrait.
etf-q-not-in-calendar = Absent de l'événement du calendrier
etf-q-consent-word = Consentement
etf-q-edit = Modifier
etf-q-done = Terminé
etf-q-move-up = Monter
etf-q-move-down = Descendre
etf-q-remove = Supprimer
etf-q-untitled = Question sans titre
etf-q-kind-locked = Une question enregistrée garde son type.
etf-q-limit = %1 questions au plus.
etf-q-preview = Prévisualiser la page de réservation
etf-q-key = Clé pour les webhooks : %1
etf-hint-questions-one = 1 question
etf-hint-questions-other = %1 questions
etf-hint-no-questions = Aucune question
