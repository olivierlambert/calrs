# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Reserva pendiente
confirmed-page-title-booked = Reserva confirmada

confirmed-heading-reschedule-requested = Reprogramación solicitada
confirmed-heading-rescheduled = ¡Reprogramado!
confirmed-heading-pending = Pendiente de confirmación
confirmed-heading-booked = ¡Listo, reservado!

confirmed-subtitle-reschedule-requested = Tu solicitud de reprogramación se ha enviado a { $host }. Recibirás un correo en { $email } una vez que se apruebe.
confirmed-subtitle-rescheduled = Tu reserva ha sido reprogramada. Se ha enviado un correo de confirmación a { $email }.
confirmed-subtitle-pending = Tu solicitud de reserva se ha enviado a { $host }. Recibirás un correo en { $email } una vez que se confirme.
confirmed-subtitle-booked = Se ha enviado un correo de confirmación a { $email }.

confirmed-detail-event = Evento:
confirmed-detail-date = Fecha:
confirmed-detail-time = Hora:
confirmed-detail-with = Con:
confirmed-detail-location = Lugar:
confirmed-detail-notes = Notas:
confirmed-detail-additional-guests = Invitados adicionales:

confirmed-book-another = Reservar otro horario

# Slot picker (templates/slots.html)

slots-location-video = Videollamada
slots-location-phone = Llamada telefónica

slots-tz-label = Tu zona horaria
slots-time-format-label = Formato de hora

slots-view-month = Vista mensual
slots-view-week = Vista semanal
slots-view-column = Vista en lista

slots-weekday-mon = Lun
slots-weekday-tue = Mar
slots-weekday-wed = Mié
slots-weekday-thu = Jue
slots-weekday-fri = Vie
slots-weekday-sat = Sáb
slots-weekday-sun = Dom

slots-weekday-mon-short = L
slots-weekday-tue-short = M
slots-weekday-wed-short = X
slots-weekday-thu-short = J
slots-weekday-fri-short = V
slots-weekday-sat-short = S
slots-weekday-sun-short = D

slots-select-date = Selecciona una fecha
slots-loading-availability = Cargando disponibilidad...
slots-click-highlighted = Haz clic en una fecha resaltada para ver los horarios disponibles
slots-no-times-month = No hay horarios disponibles este mes
slots-no-times-day = No hay horarios disponibles este día
slots-no-availability-participants = No se ha encontrado disponibilidad común para todos los participantes este mes
slots-week-more = más

# Booking form (templates/book.html)

book-page-title = Reservar { $title }
book-back-to-times = Volver a los horarios
book-name-label = Tu nombre
book-name-placeholder = Juana Pérez
book-email-label = Correo electrónico
book-email-placeholder = juana@example.com
book-notes-label = Notas
book-notes-optional = (opcional)
book-notes-placeholder = ¿Hay algún tema que te gustaría tratar?
book-additional-guests-label = Invitados adicionales
book-additional-guests-hint = (opcional, hasta { $max })
book-add-guest-btn = + Añadir invitado
book-guest-email-placeholder = colega@example.com
book-confirm-button = Confirmar reserva

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Invitado:
common-detail-reason = Motivo:
common-reason-optional = (opcional)
common-close-page = Puedes cerrar esta página.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Cancelar reserva
cancel-heading = Cancelar reserva
cancel-subtitle = Estás a punto de cancelar tu reserva.
cancel-reason-label = Motivo
cancel-reason-placeholder-host = Indícale al organizador el motivo...
cancel-button = Cancelar reserva
cancelled-heading = Reserva cancelada
cancelled-subtitle = Tu reserva se ha cancelado y se ha notificado al organizador.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Rechazar reserva
decline-heading = Rechazar reserva
decline-subtitle = Estás a punto de rechazar esta solicitud de reserva.
decline-reason-placeholder-guest = Indícale al invitado el motivo...
decline-button = Rechazar reserva
declined-heading = Reserva rechazada
declined-subtitle = La reserva se ha rechazado y se ha notificado al invitado.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Aprobar reserva
approve-heading = Aprobar reserva
approve-subtitle = Estás a punto de aprobar esta solicitud de reserva.
approve-button = Aprobar reserva
approved-heading = Reserva aprobada
approved-subtitle = La reserva se ha confirmado y se ha enviado un correo de confirmación a { $email }.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Tomar reserva
claim-heading = Tomar reserva
claim-subtitle = Estás a punto de tomar esta reserva. Serás añadido como participante.
claim-assigned-to = Asignada a:
claim-button = Tomar esta reserva
claimed-page-title = Reserva tomada
claimed-heading = Reserva tomada
claimed-subtitle = Has tomado esta reserva. Se ha enviado una invitación de calendario a tu correo.
already-claimed-page-title = Ya tomada
already-claimed-heading = Ya tomada
already-claimed-subtitle = Esta reserva ya ha sido tomada por { $name }.

# Generic error page (booking_action_error.html)

action-error-page-title = Error en la acción de reserva

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Reprogramar reserva — calrs
host-resched-heading = Reprogramar reserva
host-resched-subtitle = Esto enviará a { $guest } un correo pidiéndole que elija un nuevo horario.
host-resched-currently = Actualmente:
host-resched-button = Enviar solicitud de reprogramación
host-resched-cancel-link = Cancelar

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Confirmar reprogramación
resched-confirm-heading = Confirmar reprogramación
resched-confirm-subtitle = Estás a punto de mover tu reserva a un nuevo horario.
resched-was = Antes:
resched-new = Ahora:
resched-button = Confirmar reprogramación
resched-back-to-picker = Volver al selector de horarios

# Base layout chrome (templates/base.html)

base-loader-checking = Comprobando disponibilidad
base-loader-please-wait = Por favor espera, cargando los datos del calendario...
base-stop-impersonating = Dejar de suplantar
base-theme-toggle = Cambiar de tema
base-powered-by = Desarrollado por

# Month and weekday names + per-locale date format patterns.

common-month-1 = enero
common-month-2 = febrero
common-month-3 = marzo
common-month-4 = abril
common-month-5 = mayo
common-month-6 = junio
common-month-7 = julio
common-month-8 = agosto
common-month-9 = septiembre
common-month-10 = octubre
common-month-11 = noviembre
common-month-12 = diciembre

common-weekday-long-mon = lunes
common-weekday-long-tue = martes
common-weekday-long-wed = miércoles
common-weekday-long-thu = jueves
common-weekday-long-fri = viernes
common-weekday-long-sat = sábado
common-weekday-long-sun = domingo

# Spanish dates: "lunes, 12 de marzo de 2026"
common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday }, { $day } de { $month } de { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Reprogramar
email-action-cancel-booking = Cancelar reserva

# Email: guest booking confirmation

email-confirm-subject = Confirmada: { $event } — { $date }
email-confirm-greeting = Hola { $name },
email-confirm-headline = ¡Tu reserva se ha confirmado!
email-confirm-ics-attached-plain = Se adjunta una invitación de calendario.
email-confirm-ics-attached-html = Se adjunta una invitación de calendario a este correo.
email-confirm-need-to-cancel = ¿Necesitas cancelar? { $url }

# Email: guest reminder

email-reminder-subject = Recordatorio: { $event } a las { $time }
email-reminder-headline = Tu reunión está cerca.

# Email: guest cancellation

email-cancel-subject = Cancelada: { $event } — { $date }
email-cancel-headline-by-host = Tu reserva ha sido cancelada por { $host }.
email-cancel-headline-by-guest = Tu reserva ha sido cancelada.
email-cancel-ics-attached-plain = Se adjunta una cancelación de calendario.
email-cancel-ics-attached-html = Se adjunta una cancelación de calendario a este correo.
