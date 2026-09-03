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
slots-time-format-label = Format de l'heure

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

email-cancel-subject = Annulée : { $event } — { $date }
email-cancel-headline-by-host = Votre réservation a été annulée par { $host }.
email-cancel-headline-by-guest = Votre réservation a été annulée.
email-cancel-ics-attached-plain = Une annulation de calendrier est jointe.
email-cancel-ics-attached-html = Une annulation de calendrier est jointe à cet e-mail.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Note : l'annulation exige un préavis d'au moins { $minutes } minutes.
email-confirm-reschedule-notice = Note : la reprogrammation exige un préavis d'au moins { $minutes } minutes.

# Event type form: cancel/reschedule minimum notice (templates/event_type_form.html)


# Google Meet (English placeholders until translated)
event-type-form-location-google-meet = Google Meet (auto-generated link)
event-type-form-location-google-meet-hint = A unique Google Meet link is created on confirmation, owned by the assigned host. Every host (you, or every eligible team member) must have Google Calendar connected with a write-back calendar selected.
google-meet-prereq-no-host = Google Meet requires a host with Google Calendar connected.
google-meet-prereq-no-eligible = Google Meet requires at least one eligible team member with Google Calendar connected.
google-meet-prereq-missing = Google Meet requires every host to have Google Calendar connected with a write-back calendar selected. Still missing: { $names }. Connect them at Dashboard → Calendar sources.
google-meet-unavailable-title = Google Meet is not available
google-meet-dynamic-group-unavailable = The host needs Google Calendar connected with a write-back calendar selected.

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
overview-welcome = Bienvenue, { $name }
overview-public-page = Page publique :
overview-avail-banner-title = Disponibilité par défaut
overview-avail-banner-body = Vos horaires de travail par défaut ont été fixés du lundi au vendredi, de 9h00 à 17h00. Ils sont utilisés lorsque d'autres personnes vous incluent dans une réunion de groupe dynamique.
overview-avail-banner-cta = Vérifier vos disponibilités
overview-dismiss = Ignorer
overview-getting-started = Pour commencer
overview-getting-started-help = Suivez ces étapes pour commencer à accepter des réservations.
overview-step-connect-calendar = Connecter un agenda
overview-step-first-event-type = Créer votre premier type d'événement
overview-step-share-link = Partager votre lien de réservation
overview-pending-approval = En attente d'approbation
overview-booking-with = { $title } avec { $guest }
overview-badge-pending = en attente
overview-guest-booked = Réservé par l'invité :
overview-confirm = Confirmer
overview-decline = Refuser
overview-stat-event-types = Types d'événement
overview-stat-upcoming = Réservations à venir
overview-stat-pending = En attente d'approbation
overview-stat-sources = Sources d'agenda
overview-quick-actions = Créer un type d'événement
overview-action-public-title = Page de réservation publique
overview-action-public-desc = Partagez un lien — n'importe qui peut choisir un créneau et réserver avec vous.
overview-action-team-title = Planification d'équipe
overview-action-team-desc = Répartissez les réservations entre les membres de l'équipe ou trouvez un créneau où tout le monde est libre.
overview-action-team-desc-empty = Créez d'abord une équipe, puis configurez des types d'événement partagés.
overview-action-private-title = Privé, sur invitation
overview-action-private-desc = Générez des liens à usage unique pour des contacts précis. Personne d'autre ne peut réserver.
overview-action-shared-title = Liens d'invitation partagés
overview-action-shared-desc = Tout collègue de l'équipe peut générer des liens de réservation à partager à l'extérieur.
overview-action-reason-calendar = Connectez d'abord un agenda
overview-action-reason-ask-admin = Demandez à un administrateur de créer une équipe
overview-action-reason-team-admin = Nécessite une équipe — créez-en une d'abord
overview-action-reason-team-member = Nécessite une équipe — demandez à un administrateur

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Réservations
bookings-pending-approval = En attente d'approbation
bookings-available-to-claim = À prendre en charge
bookings-upcoming = Réservations à venir
bookings-with = { $title } avec { $guest }
bookings-guest-booked = Réservé par l'invité :
bookings-resource = Ressource :
bookings-confirm = Confirmer
bookings-reschedule = Reprogrammer
bookings-decline = Refuser
bookings-claim = Prendre en charge
bookings-badge-awaiting-reschedule = reprogrammation en attente
bookings-cancel = Annuler
bookings-reason-placeholder = Motif (facultatif)
bookings-confirm-cancel = Confirmer l'annulation
bookings-back = Retour
bookings-empty = Aucune réservation à venir pour le moment.<br>Partagez vos { $link } pour que l'on puisse réserver avec vous.
bookings-empty-link-label = liens de types d'événement

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
sources-write-bookings-to = Écrire les réservations dans :
sources-write-none = Aucun (ne pas écrire)
sources-empty = Aucune source d'agenda connectée. { $link } pour vérifier les disponibilités.
sources-empty-link-label = Ajoutez-en une

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Types d'événement
event-types-heading = Types d'événement
event-types-new = Nouveau
event-types-badge-disabled = désactivé
event-types-badge-internal = interne
event-types-badge-private = privé
event-types-badge-resources = ressources
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
event-types-empty = Aucun type d'événement pour le moment. { $link } pour commencer à accepter des réservations.
event-types-empty-link-label = Créez-en un

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
etf-conflict-calendars-help = Choisissez les agendas à consulter pour détecter les conflits. Si aucun n'est sélectionné, tous sont utilisés.
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
admin-heading = Tableau de bord d'administration
admin-action-refused = Action refusée :
admin-logo = Logo de l'entreprise
admin-logo-help = Affiché sur les pages de réservation publiques. Recommandé : PNG ou SVG, 2 Mo maximum.
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
admin-users = Utilisateurs ({ $count })
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
admin-groups = Groupes ({ $count })
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
admin-keep-current-set-hint = (laissez vide pour conserver l'actuel — actuellement défini)
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
admin-page-of = Page %1 sur %2
admin-show-more-js = Afficher %1 de plus
admin-show-fewer = Afficher moins

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Supprimer définitivement l'utilisateur { $email } ?{"\u000A"}{"\u000A"}Cela efface son compte, son profil de planification, ses sources d'agenda, ses types d'événement et toutes les données dont il est le seul propriétaire. Les réservations passées seront supprimées avec leurs types d'événement.{"\u000A"}{"\u000A"}Pour les utilisateurs OIDC/SSO : si l'inscription automatique est activée, la personne sera recréée à sa prochaine connexion.{"\u000A"}{"\u000A"}Cette action est irréversible.
admin-system-settings-help = URL publique et paramètres de sécurité réseau. Ils peuvent aussi être définis par les variables d'environnement <code>CALRS_BASE_URL</code> et <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Lorsqu'une variable d'environnement est définie, elle <strong>prime</strong> sur la valeur ci-dessous.
admin-set-by-env = — défini par l'environnement ({ $var }), prioritaire sur la valeur enregistrée
admin-google-help = Pour activer l'intégration Google Agenda, créez des identifiants OAuth2 sur la <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Activez l'<strong>API Google Calendar</strong>, puis ajoutez { $redirect_uri } comme URI de redirection autorisée.
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
form-error-location-required = Les détails du lieu sont obligatoires (lien de visioconférence, numéro de téléphone ou adresse).
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
