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

# Slot picker (templates/slots.html)

slots-location-video = Visioconférence
slots-location-phone = Appel téléphonique

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
book-notes-label = Notes
book-notes-optional = (facultatif)
book-notes-placeholder = Y a-t-il des points que vous aimeriez aborder ?
book-additional-guests-label = Invités supplémentaires
book-additional-guests-hint = (facultatif, jusqu'à { $max })
book-add-guest-btn = + Ajouter un invité
book-guest-email-placeholder = collegue@example.com
book-confirm-button = Confirmer la réservation

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
