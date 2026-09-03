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

confirmed-add-to-calendar = Añadir al calendario

# Slot picker (templates/slots.html)

slots-location-video = Videollamada
slots-location-phone = Llamada telefónica
slots-location-google-meet = Google Meet

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
book-email-invalid = Por favor, introduce una dirección de correo completa, incluido el dominio (p. ej. jane@example.com).
book-notes-label = Notas
book-notes-optional = (opcional)
book-notes-placeholder = ¿Hay algún tema que te gustaría tratar?
book-additional-guests-label = Invitados adicionales
book-additional-guests-hint = (opcional, hasta { $max })
book-add-guest-btn = + Añadir invitado
book-guest-email-placeholder = colega@example.com
book-phone-label = Número de teléfono
book-phone-placeholder = 612 34 56 78
book-phone-help = Los números locales valen; si no empiezas por +, se asume { $country }.
book-phone-optional-consequence = Déjalo vacío si prefieres no recibir mensajes de texto sobre esta reserva.
book-phone-required = Esta reserva requiere un número de teléfono.
book-phone-invalid-title = Número de teléfono no válido
book-phone-invalid = Por favor, introduce un número al que podamos enviar SMS, o deja el campo vacío.
book-phone-country-search = Buscar
book-phone-country-label = Selecciona un país
book-phone-country-none = Ningún país seleccionado
book-phone-country-no-results = Ningún país coincide con esa búsqueda
captcha-label = Verificación de seguridad
captcha-initial-state = Verifique que es humano
captcha-verifying = Verificando...
captcha-solved = Eres humano
captcha-error = Error
captcha-troubleshooting = Solución de problemas
captcha-wasm-disabled = Active WASM para una resolución significativamente más rápida
captcha-verify-aria = Haga clic para verificar que es humano
captcha-verifying-aria = Verificando, por favor espere
captcha-verified-aria = Verificado
captcha-required = Por favor, verifique que es humano
captcha-error-aria = Se ha producido un error, por favor inténtelo de nuevo
book-confirm-button = Confirmar reserva

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Reserva confirmada: { $event }, { $date } a las { $time } ({ $tz }).
sms-cancelled = Reserva cancelada: { $event }, { $date } a las { $time } ({ $tz }).
sms-rescheduled = Reserva movida: { $event } pasa al { $date } a las { $time } ({ $tz }).
sms-reminder = Recordatorio: { $event } empieza el { $date } a las { $time } ({ $tz }).

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

# Profile (templates/profile.html)

profile-pick-event-type-invite = Elige un tipo de evento para reservar un hueco.
profile-no-event-type = Aún no hay tipos de evento disponibles.

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

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

# Format patterns are parametric per locale to handle word order. Translators
# pick where each placeholder lands. Example outputs:
#   EN: April 2026  /  Tuesday, March 12, 2026
#   FR: avril 2026  /  mardi 12 mars 2026
#   ES: abril 2026  /  martes, 12 de marzo de 2026
common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday }, { $day } de { $month } de { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Reprogramar
email-action-cancel-booking = Cancelar reserva

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
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

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Nota: cancelar requiere al menos { $minutes } minutos de antelación.
email-confirm-reschedule-notice = Nota: reprogramar requiere al menos { $minutes } minutos de antelación.

# Event type form: cancel/reschedule minimum notice (templates/event_type_form.html)


# Google Meet (English placeholders until translated)
event-type-form-location-google-meet = Google Meet (auto-generated link)
event-type-form-location-google-meet-hint = A unique Google Meet link is created on confirmation, owned by the assigned host. Every host (you, or every eligible team member) must have Google Calendar connected with a write-back calendar selected.
google-meet-prereq-no-host = Google Meet requires a host with Google Calendar connected.
google-meet-prereq-no-eligible = Google Meet requires at least one eligible team member with Google Calendar connected.
google-meet-prereq-missing = Google Meet requires every host to have Google Calendar connected with a write-back calendar selected. Still missing: { $names }. Connect them at Dashboard → Calendar sources.
google-meet-unavailable-title = Google Meet is not available
google-meet-dynamic-group-unavailable = The host needs Google Calendar connected with a write-back calendar selected.

event-type-form-cancel-notice-label = Antelación mínima para cancelar
event-type-form-reschedule-notice-label = Antelación mínima para reprogramar
event-type-form-notice-help = Déjalo vacío para no poner límite.
event-type-form-resources-label = Recursos necesarios
event-type-form-resources-hint = Solo se ofrecen huecos cuando los recursos seleccionados están disponibles, según el modo de abajo.
event-type-form-resources-mode-all = Todos los recursos seleccionados deben estar libres
event-type-form-resources-mode-round-robin = Basta con un recurso libre (se asigna a la reserva)
event-type-form-notice-unit-minutes = minutos
event-type-form-notice-unit-hours = horas
event-type-form-notice-unit-days = días
event-type-form-booking-horizon-label = Horizonte de reserva
event-type-form-booking-horizon-help = Con cuántos días de antelación pueden reservar los invitados. Vacío para no poner límite, 0 para solo hoy.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = Cancelar requiere al menos { $minutes } minutos de antelación antes de la reunión.
confirmed-reschedule-notice-info = Reprogramar requiere al menos { $minutes } minutos de antelación antes de la reunión.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Esta reserva ya no se puede cancelar en línea
booking-blocked-title-reschedule = Esta reserva ya no se puede reprogramar en línea
booking-blocked-body = El anfitrión exige al menos { $minutes } minutos de antelación. Si no puedes asistir, escribe directamente a <a href="mailto:{ $host_email }">{ $host_email }</a>.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Copiar
dashboard-event-types-copied = ¡Copiado!
dashboard-event-types-copy-title = Copiar enlace de reserva
dashboard-event-types-copy-failed = No se pudo copiar

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Programación
nav-overview = Resumen
nav-event-types = Tipos de evento
nav-bookings = Reservas
nav-teams = Equipos
nav-section-shared-links = Enlaces compartidos
nav-invite-links = Enlaces de invitación
nav-section-calendars = Calendarios
nav-sources = Fuentes
nav-section-personal = Personal
nav-settings = Perfil y ajustes
nav-troubleshoot = Diagnóstico
nav-section-admin = Administración
nav-admin-panel = Panel de administración
nav-sign-out = Cerrar sesión
nav-release-notes = Ver las notas de la versión

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = La zona horaria de tu navegador es { $detected }, pero tu zona horaria de reservas es { $current }.
tz-banner-update = Actualizar
tz-banner-dismiss = Descartar

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = Introduce la URL:
editor-link-default-label = texto del enlace
editor-placeholder-text = texto
editor-nothing-to-preview = Nada que previsualizar

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Panel
overview-welcome = Hola, { $name }
overview-public-page = Página pública:
overview-avail-banner-title = Disponibilidad por defecto
overview-avail-banner-body = Tu horario laboral por defecto se ha fijado de lunes a viernes, de 9:00 a 17:00. Se usa cuando otras personas te incluyen en reuniones de grupo dinámicas.
overview-avail-banner-cta = Revisa tu disponibilidad
overview-dismiss = Descartar
overview-getting-started = Primeros pasos
overview-getting-started-help = Sigue estos pasos para empezar a aceptar reservas.
overview-step-connect-calendar = Conectar un calendario
overview-step-first-event-type = Crear tu primer tipo de evento
overview-step-share-link = Compartir tu enlace de reserva
overview-pending-approval = Pendiente de aprobación
overview-booking-with = { $title } con { $guest }
overview-badge-pending = pendiente
overview-guest-booked = Reservado por el invitado:
overview-confirm = Confirmar
overview-decline = Rechazar
overview-stat-event-types = Tipos de evento
overview-stat-upcoming = Próximas reservas
overview-stat-pending = Pendientes de aprobación
overview-stat-sources = Fuentes de calendario
overview-quick-actions = Crear un tipo de evento
overview-action-public-title = Página de reserva pública
overview-action-public-desc = Comparte un enlace: cualquiera puede elegir un hueco y reservar contigo.
overview-action-team-title = Programación de equipo
overview-action-team-desc = Reparte las reservas entre los miembros del equipo o encuentra un hueco en el que todos estén libres.
overview-action-team-desc-empty = Crea primero un equipo y luego configura tipos de evento compartidos.
overview-action-private-title = Privado, solo con invitación
overview-action-private-desc = Genera enlaces de un solo uso para contactos concretos. Nadie más puede reservar.
overview-action-shared-title = Enlaces de invitación compartidos
overview-action-shared-desc = Cualquier compañero del equipo puede generar enlaces de reserva para compartirlos fuera.
overview-action-reason-calendar = Conecta primero un calendario
overview-action-reason-ask-admin = Pide a una persona administradora que cree un equipo
overview-action-reason-team-admin = Requiere un equipo: crea uno primero
overview-action-reason-team-member = Requiere un equipo: pídeselo a administración

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Reservas
bookings-pending-approval = Pendiente de aprobación
bookings-available-to-claim = Disponibles para asumir
bookings-upcoming = Próximas reservas
bookings-with = { $title } con { $guest }
bookings-guest-booked = Reservado por el invitado:
bookings-resource = Recurso:
bookings-confirm = Confirmar
bookings-reschedule = Reprogramar
bookings-decline = Rechazar
bookings-claim = Asumir
bookings-badge-awaiting-reschedule = reprogramación pendiente
bookings-cancel = Cancelar
bookings-reason-placeholder = Motivo (opcional)
bookings-confirm-cancel = Confirmar la cancelación
bookings-back = Volver
bookings-empty = Aún no hay reservas próximas.<br>Comparte tus { $link } para que puedan reservar contigo.
bookings-empty-link-label = enlaces de tipos de evento

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Equipos
teams-heading = Equipos
teams-new = Nuevo
teams-badge-public = público
teams-badge-private = privado
teams-settings = Ajustes
teams-view = Ver
teams-empty = Aún no hay equipos.
teams-empty-admin = { $link } para colaborar con tu equipo.
teams-empty-admin-link-label = Crea uno
teams-empty-member = Los equipos los crea administración. Pídeles que creen uno y te añadan como miembro.

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Enlaces de invitación
invite-links-heading = Enlaces de invitación
invite-links-new = Nuevo evento interno
invite-links-help = Genera enlaces de reserva de un solo uso para tipos de evento internos. Cualquier compañero autenticado puede crear y compartir enlaces aquí.
invite-links-duration = { $minutes } min
invite-links-hosted-by = Organiza { $host }
invite-links-get-link = Obtener enlace
invite-links-invites = Invitaciones
invite-links-empty = Aún no hay tipos de evento internos.<br>{ $link } con visibilidad «Interno» para que cualquier compañero pueda generar enlaces de reserva.
invite-links-empty-link-label = Crea un tipo de evento
invite-links-js-generating = Generando...
invite-links-js-copied = ¡Copiado!
invite-links-js-error = Error

teams-member-count =
    { $count ->
        [one] { $count } miembro
       *[other] { $count } miembros
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Fuentes de calendario
sources-heading = Fuentes de calendario
sources-add = Añadir
sources-last-sync = Última sincronización:
sources-sync = Sincronizar
sources-full-resync = Resincronización completa
sources-full-resync-title = Vaciar la caché y volver a descargar todos los eventos del servidor
sources-test = Probar
sources-reconnect = Volver a conectar
sources-reconnect-title = Repetir el flujo de consentimiento de Google
sources-edit = Editar
sources-remove = Eliminar
sources-remove-confirm = ¿Eliminar la fuente «{ $name }»? Se borrarán todos los eventos sincronizados desde ella.
sources-no-write-calendar = No hay calendario de escritura seleccionado. Las reservas confirmadas se quedan en calrs y no se envían a este calendario. Elige uno abajo para activar la escritura.
sources-write-bookings-to = Escribir las reservas en:
sources-write-none = Ninguno (no escribir)
sources-empty = No hay fuentes de calendario conectadas. { $link } para comprobar la disponibilidad.
sources-empty-link-label = Añade una

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Tipos de evento
event-types-heading = Tipos de evento
event-types-new = Nuevo
event-types-badge-disabled = desactivado
event-types-badge-internal = interno
event-types-badge-private = privado
event-types-badge-resources = recursos
event-types-send-invites = Enviar invitaciones
event-types-duration = { $minutes } min
event-types-mode-collective = colectivo
event-types-mode-round-robin = por turnos
event-types-edit = Editar
event-types-disable = Desactivar
event-types-enable = Activar
event-types-embed = Insertar
event-types-overrides = Excepciones
event-types-team-settings = Ajustes del equipo
event-types-invites = Invitaciones
event-types-view-public = Ver la página pública
event-types-view-page = Ver la página
event-types-delete = Eliminar
event-types-delete-confirm = ¿Eliminar el tipo de evento «{ $title }»? Esta acción no se puede deshacer.
event-types-empty = Aún no hay tipos de evento. { $link } para empezar a aceptar reservas.
event-types-empty-link-label = Crea uno

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Negrita (Ctrl+B)
editor-italic = Cursiva (Ctrl+I)
editor-strikethrough = Tachado
editor-code = Código en línea
editor-link = Insertar enlace (Ctrl+K)
editor-toggle-preview = Mostrar u ocultar la vista previa
editor-preview = Vista previa

# Profile and settings (templates/settings.html)

settings-page-title = Ajustes
settings-heading = Perfil y ajustes
settings-public-page-label = Tu página de reserva pública
settings-copy = Copiar
settings-copied = ¡Copiado!
settings-open = Abrir
settings-avatar = Avatar
settings-upload = Subir
settings-remove = Eliminar
settings-display-name = Nombre visible
settings-display-name-placeholder = Tu nombre
settings-username = Nombre de usuario
settings-username-hint = (se usa en tu URL de reserva)
settings-username-pattern-title = Solo minúsculas, números y guiones
settings-username-help = Tu página de reserva pública:
settings-title = Cargo
settings-title-placeholder = p. ej. Ingeniera de software, Jefe de producto
settings-title-help = Se muestra en tu perfil público y en la barra lateral.
settings-bio = Biografía
settings-bio-placeholder = Cuenta algo sobre ti...
settings-bio-help = Se muestra en tu página de reserva pública. Admite **negrita**, *cursiva*, ~~tachado~~, `código` y [enlaces](url).
settings-booking-email = Correo de reservas
settings-booking-email-help = Esta dirección aparecerá en tus páginas de reserva públicas y en las notificaciones por correo. Déjala vacía para usar tu correo de acceso.
settings-booking-email-warning = Asegúrate de que esta dirección existe en tu proveedor de correo. Si no, las notificaciones no se entregarán.
settings-timezone = Zona horaria
settings-timezone-help = Tus reglas de disponibilidad y tus horas de reserva se calculan en esta zona horaria.
settings-language = Idioma
settings-language-auto = Automático (idioma del navegador)
settings-language-help = Elige un idioma de la interfaz, o déjalo en Automático para seguir el ajuste de tu navegador.
settings-dynamic-group = Permitir que otras personas me incluyan en enlaces de grupo dinámicos
settings-dynamic-group-help = Si lo activas, otros usuarios pueden crear URL de reunión colectiva improvisadas que te incluyan (p. ej. { $example }).
settings-lend-resource = Prestar mi acceso al calendario para reservar recursos
settings-lend-resource-help = Cuando una reserva necesite reservar un recurso compartido (laboratorio de demostraciones, sala de reuniones) en el que tu cuenta de calendario puede escribir, permite que calrs use tus credenciales guardadas para esa escritura.
settings-default-availability = Disponibilidad por defecto
settings-default-availability-help = Tu horario laboral por defecto. Se usa en los enlaces de grupo dinámicos cuando otras personas te incluyen en una reunión.
settings-copy-to-all = Copiar a todos los días
settings-copy-to-all-title = Copiar las franjas del primer día activado al resto de días activados
settings-add-window = Añadir franja horaria
settings-remove-window = Quitar franja
settings-save = Guardar ajustes
settings-appearance = Apariencia
settings-theme-system = Sistema
settings-theme-light = Claro
settings-theme-dark = Oscuro

# Sign in (templates/auth/login.html)

login-page-title = Iniciar sesión
login-heading = Iniciar sesión
login-subtitle = Inicia sesión en tu cuenta de calrs
login-sso = Iniciar sesión con SSO
login-or = o
login-email = Correo electrónico
login-password = Contraseña
login-submit = Iniciar sesión con correo
login-no-account = ¿Aún no tienes cuenta? { $link }
login-register-link = Regístrate

# Registration (templates/auth/register.html)

register-page-title = Registro
register-heading = Crear una cuenta
register-subtitle = Regístrate para tener una cuenta de calrs
register-domains-limited = El registro está limitado a: { $domains }
register-name = Nombre
register-name-placeholder = Tu nombre
register-email = Correo electrónico
register-password = Contraseña
register-password-hint = (mín. 12 caracteres)
register-submit = Crear una cuenta
register-have-account = ¿Ya tienes una cuenta? { $link }
register-signin-link = Inicia sesión

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Demasiados intentos de inicio de sesión. Por favor, inténtalo de nuevo más tarde.
auth-error-invalid-credentials = Correo o contraseña incorrectos
auth-error-internal = Error interno
auth-error-registration-disabled = El registro está desactivado.
auth-error-name-length = El nombre debe tener entre 1 y 255 caracteres
auth-error-email-length = El correo debe tener entre 1 y 255 caracteres
auth-error-email-invalid = Por favor, introduce una dirección de correo válida
auth-error-email-domain = Dominio de correo no permitido
auth-error-password-length = La contraseña debe tener al menos 12 caracteres
auth-error-email-taken = Ese correo ya está registrado
auth-error-create-failed = No se pudo crear la cuenta

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Fuente de calendario
source-test-sync-heading = Sincronización: { $name }
source-test-heading = Prueba de conexión
source-write-page-title = Configurar la escritura en el calendario
source-write-back = Volver al panel
source-write-heading = ¿Dónde deben guardarse las reservas?
source-write-help = Cuando alguien reserve una reunión contigo, calrs puede crear el evento automáticamente en tu calendario. Elige en qué calendario escribir las reservas de { $name }.
source-write-save = Guardar
source-write-skip = Omitir por ahora
source-write-sync-results = Resultados de la sincronización

source-write-event-count =
    { $count ->
        [one] { $count } evento
       *[other] { $count } eventos
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Excepciones por fecha
overrides-heading = Excepciones por fecha
overrides-back-teams = Volver a los equipos
overrides-back-event-types = Volver a los tipos de evento
overrides-intro = Añade excepciones para fechas concretas en { $title }
overrides-add-heading = Añadir una excepción
overrides-date = Fecha
overrides-type = Tipo de excepción
overrides-type-blocked = Bloquear todo el día
overrides-type-custom = Horario personalizado
overrides-start-time = Hora de inicio
overrides-end-time = Hora de fin
overrides-add-submit = Añadir la excepción
overrides-existing = Excepciones existentes
overrides-badge-blocked = bloqueado
overrides-badge-custom = horario personalizado
overrides-delete = Eliminar
overrides-delete-confirm = ¿Eliminar esta excepción?
overrides-empty = Aún no hay excepciones por fecha.<br>Usa el formulario de arriba para bloquear fechas concretas (festivos, días libres) o fijar un horario personalizado.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Elige un tipo de evento para reservar un hueco.
team-profile-empty = Aún no hay tipos de evento disponibles.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Diagnóstico
troubleshoot-empty = No se han encontrado tipos de evento. { $link } para empezar a diagnosticar la disponibilidad.
troubleshoot-empty-link-label = Crea uno
troubleshoot-subtitle = Descubre por qué los huecos de { $title } están disponibles o bloqueados
troubleshoot-duration = { $minutes } min
troubleshoot-buffer-before = { $minutes } min de margen antes
troubleshoot-buffer-after = { $minutes } min de margen después
troubleshoot-min-notice = { $minutes } min de antelación
troubleshoot-blocked-override = Bloqueado por una excepción de fecha (día libre)
troubleshoot-custom-hours-active = Excepción de horario personalizado activa (sustituye a las reglas semanales)
troubleshoot-legend-available = Disponible
troubleshoot-legend-calendar-event = Evento de calendario
troubleshoot-legend-booking = Reserva
troubleshoot-legend-resource = Recurso ocupado
troubleshoot-legend-outside = Fuera de horario
troubleshoot-legend-buffer = Margen / antelación mínima
troubleshoot-blocked-slots = Huecos bloqueados
troubleshoot-none-date-blocked = Esta fecha está bloqueada por una excepción de disponibilidad (día libre). No hay huecos disponibles.
troubleshoot-none-custom-hours = Hay una excepción de horario personalizado activa, pero ninguna franja coincide. Revisa los ajustes de la excepción.
troubleshoot-none-no-rules = No hay reglas de disponibilidad para este día de la semana. Este tipo de evento no se puede reservar el { $date }.
troubleshoot-none-all-bookable = No hay huecos bloqueados dentro del horario de disponibilidad. Se puede reservar a cualquier hora.
troubleshoot-label-outside = Fuera de la disponibilidad
troubleshoot-label-available = Disponible
troubleshoot-label-min-notice = Antelación mínima ({ $minutes } min)
troubleshoot-label-beyond-horizon = Más allá del horizonte de reserva ({ $days } días)
troubleshoot-label-buffer = Margen ({ $minutes } min)
troubleshoot-label-resource-busy = Recurso ocupado: { $names }
troubleshoot-detail-around = Alrededor de: { $label }
troubleshoot-detail-around-booking = Alrededor de la reserva de { $guest }
troubleshoot-reason-calendar-event = Evento de calendario: { $label }
troubleshoot-reason-booking = Reserva: { $label }

# Invite management (templates/invite_form.html)

invites-heading = Invitaciones
invites-back-teams = Volver a los equipos
invites-back-event-types = Volver a los tipos de evento
invites-intro = Envía enlaces de invitación para { $title }
invites-capped = <strong>La entrada se ha limitado a { $max } destinatarios por envío.</strong> Envía el resto en otra tanda.
invites-failed-hint = — consulta los registros del servidor para más detalles.
invites-quick-link = Enlace rápido
invites-quick-link-help = Genera un enlace de un solo uso y cópialo al portapapeles.
invites-get-link = Obtener enlace
invites-or-email = O enviar por correo
invites-recipients = Destinatarios
invites-recipients-hint = (un correo por línea, máximo { $max })
invites-message = Mensaje personal
invites-message-hint = (opcional, se envía a todos los destinatarios)
invites-message-placeholder = Tengo ganas de enseñarte una demostración...
invites-expires-in = Caduca en
invites-expires-days = { $days } días
invites-expires-never = Nunca
invites-allow-multiple = Permitir varias reservas por destinatario
invites-send = Enviar invitaciones
invites-sent-heading = Invitaciones enviadas
invites-badge-expired = caducada
invites-badge-used = usada
invites-badge-active = activa
invites-sent-by = Enviada por { $name }
invites-uses = { $used }/{ $max } usos
invites-expires-at = Caduca el { $date }
invites-copy-link = Copiar enlace
invites-delete = Eliminar
invites-delete-confirm = ¿Eliminar esta invitación?
invites-empty = Aún no has enviado invitaciones. Usa el formulario de arriba para mandar a alguien un enlace de reserva.
invites-js-generating = Generando...
invites-js-copied = ¡Copiado!
invites-js-error = Error

invites-sent-count =
    { $count ->
        [one] { $count } invitación enviada.
       *[other] { $count } invitaciones enviadas.
    }

invites-skipped-invalid =
    { $count ->
        [one] { $count } fila no válida omitida:
       *[other] { $count } filas no válidas omitidas:
    }

invites-skipped-duplicate =
    { $count ->
        [one] { $count } fila duplicada omitida:
       *[other] { $count } filas duplicadas omitidas:
    }

invites-failed =
    { $count ->
        [one] { $count } invitación fallida (BD o SMTP):
       *[other] { $count } invitaciones fallidas (BD o SMTP):
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Editar la fuente de calendario
source-form-title-add = Añadir un calendario
source-form-heading-edit = Editar la fuente de calendario
source-form-heading-add = Conectar un calendario
source-form-subtitle-edit = Actualiza la conexión. Deja la contraseña vacía para conservar la actual. Si cambias la URL o el usuario, sincroniza para actualizar la lista de calendarios detectados.
source-form-subtitle-add = Conecta un servidor CalDAV o Microsoft Exchange (EWS) para que calrs pueda comprobar tu disponibilidad cuando alguien reserve.
source-form-backend = Backend
source-form-preset = Preajuste
source-form-connect-google = Conectar con Google
source-form-google-unavailable = Google Calendar no está disponible. Habla con tu administración.
source-form-name = Nombre visible
source-form-name-placeholder = Mi calendario
source-form-url-caldav = URL de CalDAV
source-form-url-ews = URL del punto de conexión EWS
source-form-username = Usuario
source-form-password = Contraseña
source-form-password-keep = Déjala vacía para conservar la actual
source-form-password-placeholder = Contraseña de aplicación o de la cuenta
source-form-skip-test = Omitir la prueba de conexión
source-form-skip-test-help = Úsalo si la prueba se queda colgada (pasa en algunas instalaciones de BlueMind o Zimbra). Puedes probar la conexión más tarde.
source-form-save = Guardar los cambios
source-form-add = Añadir la fuente de calendario
source-form-help-google-configured = Pulsa el botón de abajo para autorizar a calrs a acceder a tu Google Calendar.
source-form-help-google-unconfigured = La integración con Google Calendar aún no está configurada. Pide a tu administración que configure las credenciales OAuth2 de Google en el panel de administración.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — Usa el punto de conexión DAV de tu servidor BlueMind.<br> Normalmente: <code>https://mail.yourcompany.com/dav/</code><br> El usuario es tu <strong>dirección de correo</strong> (p. ej. <code>alice@yourcompany.com</code>), no solo el nombre de acceso.<br> Si la prueba de conexión se queda colgada, marca «Omitir la prueba de conexión» y sincroniza directamente.
source-form-help-nextcloud = <strong>Nextcloud</strong> — Usa la raíz WebDAV, no la URL de un calendario concreto.<br> Normalmente: <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — Usa tu dirección completa en la ruta de la URL.<br> Ejemplo: <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Usa una contraseña de aplicación (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — Usa <code>https://caldav.icloud.com/</code><br> Necesitas una contraseña de aplicación de <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Seguridad &rarr; Contraseñas de aplicación).
source-form-help-zimbra = <strong>Zimbra</strong> — Usa el punto de conexión DAV de tu servidor Zimbra.<br> Normalmente: <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — Usa el punto de conexión DAV de SOGo.<br> Normalmente: <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — Usa la URL raíz del servidor.<br> Normalmente: <code>https://cal.example.com/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong>. Usa el punto de conexión SOAP:<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> El usuario es la dirección del buzón; la contraseña debe admitir HTTP Basic sobre TLS (actívalo en un buzón de servicio si tu organización lo tiene desactivado).<br> Elige además <strong>Microsoft Exchange (EWS)</strong> en el desplegable Backend de arriba.
source-form-help-google = <strong>Google Calendar</strong>: conexión mediante OAuth2. No hace falta contraseña.<br>
source-form-help-other = Introduce la <strong>URL raíz DAV</strong> de tu servidor CalDAV, no la de un calendario concreto ni un enlace público.<br> calrs detectará tus calendarios automáticamente mediante PROPFIND (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Negrita
editor-italic-short = Cursiva
editor-link-short = Insertar enlace

# Team creation (templates/team_form.html)

team-form-heading = Nuevo equipo
team-form-name = Nombre del equipo
team-form-name-placeholder = Ingeniería
team-form-slug = Identificador
team-form-slug-hint = (identificador apto para URL)
team-form-slug-pattern-title = Solo minúsculas, números y guiones
team-form-description = Descripción
team-form-optional = (opcional)
team-form-description-placeholder = De qué se ocupa este equipo...
team-form-description-help = Se muestra en la página del equipo. Admite **negrita**, *cursiva* y [enlaces](url).
team-form-visibility = Visibilidad
team-form-public = Público
team-form-private = Privado
team-form-visibility-help = Los equipos privados reciben un token de invitación para compartir. Los públicos aparecen en la página de perfil del equipo.
team-form-members = Miembros
team-form-members-help = Se te añadirá automáticamente como administrador del equipo. Añade usuarios concretos o vincula grupos OIDC.
team-form-search-placeholder = Buscar usuarios o grupos...
team-form-search-users = Usuarios
team-form-search-groups = Grupos OIDC
team-form-you = (tú)
team-form-submit = Crear el equipo

# Team settings (templates/team_settings.html)

team-settings-page-title = Ajustes
team-settings-subtitle = Ajustes del equipo: los administradores del equipo pueden editarlos.
team-settings-public-url = URL pública
team-settings-public-url-help = Cualquiera puede reservar con este enlace.
team-settings-invite-link = Enlace de invitación
team-settings-invite-link-help = Comparte este enlace para dar acceso a la página de reserva de este equipo privado.
team-settings-avatar = Avatar del equipo
team-settings-profile = Perfil
team-settings-description-placeholder = Habla de este equipo...
team-settings-description-help = Se muestra en la página de reserva pública del equipo. Admite **negrita**, *cursiva* y [enlaces](url).
team-settings-visibility-help = Los equipos públicos aparecen en la página de perfil del equipo. Los privados necesitan un enlace de invitación.
team-settings-members-help = Gestiona quién pertenece a este equipo. Añade usuarios concretos o vincula grupos OIDC para sincronizarlos automáticamente.
team-settings-role-member = Miembro
team-settings-role-admin = Administrador
team-settings-oidc-group = Grupo OIDC
team-settings-remove = Quitar
team-settings-save = Guardar los cambios
team-settings-danger-zone = Zona de peligro
team-settings-danger-help = Elimina este equipo de forma permanente. Los tipos de evento se desvincularán, no se borrarán. Esta acción no se puede deshacer.
team-settings-delete = Eliminar este equipo
team-settings-delete-confirm = ¿Eliminar el equipo «{ $name }»? Esta acción no se puede deshacer.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Editar el tipo de evento
etf-heading-new = Nuevo tipo de evento
etf-team = Equipo
etf-team-hint = (opcional: déjalo vacío para un tipo de evento personal)
etf-team-personal = Personal
etf-scheduling-mode = Modo de asignación
etf-mode-round-robin = Por turnos: asignar a un miembro disponible
etf-mode-collective = Colectivo: todos los miembros deben estar disponibles
etf-scheduling-mode-help = «Por turnos» asigna la reserva a un miembro disponible (primero el menos ocupado). «Colectivo» exige que todos los miembros estén libres a la vez.
etf-title = Título
etf-title-placeholder = Llamada de presentación de 30 min
etf-slug = Identificador
etf-slug-placeholder = se genera a partir del título
etf-description-placeholder = Una llamada breve de presentación para hablar de...
etf-description-help = Se muestra en la página de reserva. Admite **negrita**, *cursiva* y [enlaces](url).
etf-location = Ubicación
etf-location-link = Videollamada (URL fija)
etf-location-jitsi = Jitsi (sala generada automáticamente)
etf-location-webhook = Webhook (proveedor propio)
etf-location-phone = Teléfono
etf-location-in-person = Presencial
etf-location-custom = Personalizada
etf-location-details = Detalles
etf-location-details-placeholder = https://meet.example.com/mi-sala
etf-pattern-placeholder = Déjalo vacío para usar el patrón por defecto de la organización
etf-duration = Duración (minutos)
etf-slot-interval = Intervalo entre huecos (minutos)
etf-slot-interval-placeholder = Igual que la duración
etf-slot-interval-help = Cada cuánto empiezan los huecos. Déjalo vacío para que siga a la duración.
etf-required-members = Miembros necesarios
etf-required-members-help = Todos los miembros marcados deben estar libres para que se ofrezca un hueco. Desmarca a quienes quieras excluir (se ignorará su disponibilidad).
etf-member-priority = Prioridad de los miembros
etf-member-priority-help = Los miembros con más prioridad reciben antes las reservas cuando están disponibles. A igual prioridad, se equilibra según las reservas recientes.
etf-member-timezone-title = Zona horaria del miembro. Su horario laboral personal se interpreta en esta zona.
etf-priority-high = Alta
etf-priority-medium = Media
etf-priority-low = Baja
etf-section-availability = Disponibilidad
etf-timezone-help = Las horas de abajo se interpretan en esta zona horaria. En los tipos de evento de equipo, elige la zona horaria de trabajo del equipo (no necesariamente la de quien lo crea).
etf-reset-default = Restablecer mis valores por defecto
etf-reset-default-title = Sustituir estas horas por la disponibilidad por defecto de tu perfil
etf-availability-prefilled = Rellenado a partir de tu { $link }. Puedes cambiarlo aquí para este tipo de evento.
etf-availability-prefilled-link = disponibilidad por defecto
etf-section-buffers = Márgenes y antelación
etf-buffer-before = Margen antes (min)
etf-buffer-after = Margen después (min)
etf-min-notice = Antelación mínima
etf-min-notice-help = Con cuánta antelación hay que reservar.
etf-section-limits = Límites de reserva
etf-first-slot-only = Un hueco al día
etf-first-slot-only-help = Mostrar solo la primera hora disponible de cada día.
etf-freq-limit = Limitar la frecuencia de reservas
etf-freq-limit-help = Limitar cuántas veces se puede reservar este evento por periodo.
etf-add-limit = Añadir un límite
etf-section-options = Opciones de reserva
etf-requires-confirmation = Requiere confirmación
etf-requires-confirmation-help = Las reservas quedarán pendientes hasta que las apruebes desde el panel.
etf-sms = Notificaciones por SMS
etf-sms-off = Desactivadas, no se pide el teléfono
etf-sms-optional = Opcional, los invitados pueden dejar un número
etf-sms-required = Obligatorio, los invitados deben dejar un número
etf-sms-help = Envía un SMS al invitado, además del correo, cuando su reserva se confirma, se mueve, se cancela o está a punto de empezar. Quien deje el campo vacío simplemente no recibe SMS. Requiere una pasarela de SMS en el { $link }.
etf-admin-panel-link = panel de administración
etf-additional-guests = Invitados adicionales
etf-guests-none = Los invitados no pueden añadir a nadie
etf-additional-guests-help = Permitir que quien reserva invite a más asistentes, que recibirán la invitación de calendario.
etf-default-view = Vista de calendario por defecto
etf-view-month = Mes: cuadrícula de calendario con lista de huecos
etf-view-week = Semana: columnas de 7 días con sus huecos
etf-view-column = Columna: días en lista con sus huecos
etf-view-week-short = semanal
etf-view-column-short = de columna
etf-default-view-help = La vista que ven los invitados al entrar. Pueden cambiarla cuando quieran.
etf-conflict-calendars = Calendarios para conflictos
etf-conflict-calendars-help = Elige qué calendarios se consultan para detectar conflictos. Si no eliges ninguno, se usan todos.
etf-no-resources = Aún no hay recursos compartidos configurados. Añade uno (laboratorio de demostraciones, sala de reuniones) en el { $link } para exigirlo aquí.
etf-section-access = Acceso y notificaciones
etf-visibility-public = Público: visible en tu perfil
etf-visibility-internal = Interno: cualquier compañero puede generar enlaces de invitación
etf-visibility-private = Privado: solo con enlace de invitación
etf-visibility-help = Controla quién puede ver y reservar este tipo de evento.
etf-vis-internal = Interno
etf-reminder = Recordatorio de la reserva
etf-reminder-none = Sin recordatorio
etf-reminder-help = Enviar un correo de recordatorio a ti y a tu invitado antes de la reunión.
etf-dynamic-group = Enlace de grupo dinámico
etf-dynamic-group-help = Crea un enlace de reunión improvisado que comprueba tu disponibilidad y la de otros usuarios.
etf-dynamic-group-search = Busca un usuario para añadirlo...
etf-dynamic-group-note = Solo se muestran los usuarios que permiten enlaces de grupo dinámicos.
etf-dynamic-group-url = URL del enlace de grupo
etf-watcher-teams = Equipos observadores
etf-watcher-teams-help = Los equipos seleccionados recibirán un aviso con cada reserva. Sus miembros pueden asumir una reserva para participar en ella.
etf-save = Guardar los cambios
etf-create = Crear el tipo de evento
etf-js-loading = Cargando...
etf-js-no-default = Sin valor por defecto
etf-js-reset-done = ¡Restablecido!
etf-js-error = Error
etf-js-remove-limit = Quitar el límite
etf-period-day = Al día
etf-period-week = A la semana
etf-period-month = Al mes
etf-period-year = Al año

# Event type form: runtime summary hints (templates/event_type_form.html)


# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = Ningún día definido
etf-hint-every-day = Todos los días
etf-fmt-day-one = %1 día
etf-fmt-day-other = %1 días
etf-fmt-hours = %1 h
etf-fmt-minutes = %1 min
etf-hint-buffer-both = %1 min antes, %2 min después
etf-hint-buffer-before = %1 min de margen antes
etf-hint-buffer-after = %1 min de margen después
etf-hint-notice = %1 de antelación
etf-hint-no-buffers = Sin márgenes, se puede reservar a cualquier hora
etf-hint-max = Máx. %1
etf-hint-period-day = /día
etf-hint-period-week = /semana
etf-hint-period-month = /mes
etf-hint-period-year = /año
etf-hint-no-limits = Sin límites
etf-hint-confirmation-required = Requiere confirmación
etf-hint-auto-confirmed = Confirmación automática
etf-hint-extra-guests-one = hasta %1 invitado más
etf-hint-extra-guests-other = hasta %1 invitados más
etf-hint-view = vista %1
etf-hint-reminder = recordatorio %1 antes
etf-hint-no-reminder = sin recordatorio

etf-guests-up-to =
    { $count ->
        [one] Hasta { $count } invitado adicional
       *[other] Hasta { $count } invitados adicionales
    }

etf-reminder-hours =
    { $count ->
        [one] { $count } hora antes
       *[other] { $count } horas antes
    }

etf-reminder-days =
    { $count ->
        [one] { $count } día antes
       *[other] { $count } días antes
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Estás creando un tipo de evento <strong>público</strong> &mdash; cualquiera con el enlace puede reservar.
etf-preset-private = Estás creando un tipo de evento <strong>privado</strong> &mdash; solo pueden reservar las personas a las que invites.
etf-preset-internal = Estás creando un tipo de evento <strong>interno</strong> &mdash; cualquier compañero puede compartir el enlace de reserva.
etf-preset-team = Estás creando un tipo de evento <strong>de equipo</strong> &mdash; las reservas se reparten entre los miembros del equipo.
etf-pattern-hint = Patrón propio opcional. Comodines: <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Déjalo vacío para usar el valor por defecto de la organización configurado por administración.
etf-pattern-random-warning = Este patrón no lleva el comodín <code>{"{"}random{"}"}</code>. Dos reservas de este tipo de evento el mismo día compartirán sala, y el segundo invitado puede entrar en la reunión del primero. Usa salas fijas solo si es justo lo que quieres.
etf-webhook-hint = La URL de reunión de cada reserva se obtiene del webhook que haya configurado administración en Administración &rarr; Webhook de reunión. Aquí no hace falta ninguna URL.

# Admin panel (templates/admin.html)

admin-page-title = Administración
admin-heading = Panel de administración
admin-action-refused = Acción rechazada:
admin-logo = Logotipo de la empresa
admin-logo-help = Se muestra en las páginas de reserva públicas. Recomendado: PNG o SVG, máx. 2 MB.
admin-company-link = Enlace de la empresa
admin-company-link-help = En las páginas de reserva públicas, el logotipo enlazará a esta URL. Déjalo vacío para no poner enlace.
admin-theme = Tema
admin-theme-help = Elige un tema de color para todas las páginas. El cambio entre claro y oscuro es independiente: los temas se adaptan a ambos modos.
admin-theme-default = Por defecto
admin-theme-default-desc = Azul limpio
admin-theme-nord-desc = Escarcha ártica
admin-theme-dracula-desc = Morado oscuro
admin-theme-gruvbox-desc = Retro cálido
admin-theme-solarized-desc = El clásico de Ethan
admin-theme-tokyo-desc = Ciudad de neón
admin-theme-custom = Personalizado
admin-theme-custom-desc = Tus colores
admin-custom-colors = Colores personalizados
admin-color-accent = Color de acento
admin-color-accent-hover = Acento al pasar el cursor
admin-color-bg = Fondo
admin-color-surface = Superficie
admin-color-text = Texto
admin-save-theme = Guardar el tema
admin-users = Usuarios ({ $count })
admin-user-filter = Filtrar por nombre o correo…
admin-badge-admin = administración
admin-badge-disabled = desactivado
admin-impersonate = Suplantar
admin-demote = Degradar
admin-promote = Ascender
admin-disable = Desactivar
admin-enable = Activar
admin-delete = Eliminar
admin-no-users-match = Ningún usuario coincide con el filtro.
admin-no-users = Aún no hay usuarios.
admin-groups = Grupos ({ $count })
admin-group-filter = Filtrar por nombre de grupo…
admin-group-name = Nombre del grupo
admin-weight = peso:
admin-no-groups-match = Ningún grupo coincide con el filtro.
admin-no-groups = Aún no se ha sincronizado ningún grupo. Los grupos se sincronizan automáticamente desde tu proveedor OIDC.
admin-auth-settings = Ajustes de acceso
admin-registration-enabled = Registro activado
admin-allowed-domains = Dominios de correo permitidos
admin-allowed-domains-hint = (separados por comas, vacío para permitir todos)
admin-save-auth = Guardar los ajustes de acceso
admin-system-settings = Ajustes del sistema
admin-base-url = URL base
admin-base-url-help = URL pública de esta instancia. Se usa en las redirecciones OIDC y en los enlaces de los correos (aprobar/rechazar, cancelar, recordatorios).
admin-private-hosts = Lista de hosts privados permitidos
admin-private-hosts-help = Nombres de host, separados por comas, que pueden resolverse a IP privadas o reservadas en las fuentes CalDAV/EWS (excepción a la protección SSRF). Añade solo hosts que controles (por ejemplo, un servidor de calendario en la misma red Docker). Déjalo vacío para mantener la protección activa en todos los hosts.
admin-unset-env = Quita la variable de entorno para poder editar esto desde aquí.
admin-save-system = Guardar los ajustes del sistema
admin-status = Estado:
admin-status-enabled = activado
admin-status-disabled = desactivado
admin-status-disabled-paren = (desactivado)
admin-status-configured = configurado
admin-status-not-configured = sin configurar
admin-via-environment = (mediante el entorno)
admin-issuer = Emisor:
admin-client-id = ID de cliente:
admin-instance = Instancia:
admin-oidc-settings = Ajustes de OIDC
admin-oidc-enabled = OIDC activado
admin-issuer-url = URL del emisor
admin-client-id-label = ID de cliente
admin-client-secret = Secreto de cliente
admin-keep-current-hint = (déjalo vacío para conservar el actual)
admin-keep-current-set-hint = (déjalo vacío para conservar el actual: ya hay uno guardado)
admin-keep-unchanged = Déjalo vacío para no cambiarlo
admin-oidc-auto-register = Registrar automáticamente a los usuarios nuevos de OIDC
admin-save-oidc = Guardar los ajustes de OIDC
admin-google = Google Calendar (OAuth2)
admin-save-google = Guardar los ajustes de OAuth2 de Google
admin-captcha = Captcha
admin-instance-url = URL de la instancia
admin-site-key = Clave del sitio
admin-secret = Secreto
admin-widget-url = URL del script del widget
admin-widget-url-help = Cámbiala si el CDN está bloqueado. Los cambios se aplican nada más guardar.
admin-captcha-disable-help = Deja vacías la URL de la instancia, la clave del sitio y el secreto para desactivar el captcha en las páginas de reserva.
admin-save-captcha = Guardar los ajustes del captcha
admin-resources = Recursos
admin-resources-help = Recursos compartidos reservables (laboratorio de demostraciones, salas de reuniones) basados en un feed de calendario. Al asociarlos a tipos de evento, un recurso ocupado bloquea las reservas.
admin-resource-stats = Eventos en caché: { $events } &middot; Asociado a { $attached } tipo(s) de evento
admin-never = nunca
admin-resource-sync-failed = (el último intento falló: { $error })
admin-writeback-enabled = Escritura: activada ({ $via })
admin-writeback-readonly = Escritura: solo lectura
admin-teams-allowed = Equipos permitidos:
admin-teams-allowed-none = ninguno (solo administración global)
admin-sync-now = Sincronizar ahora
admin-test-write = Probar la escritura
admin-delete-resource-confirm = ¿Eliminar este recurso? Los tipos de evento que lo usan dejarán de comprobarlo.
admin-name = Nombre
admin-name-help = Déjalo vacío para tomar el nombre del feed.
admin-feed-url = URL del feed ICS (dirección de publicación)
admin-feed-url-help = BlueMind: la dirección de calendario pública o privada del calendario del recurso.
admin-caldav-url = URL de la colección CalDAV (para la escritura)
admin-caldav-url-help = Opcional. En BlueMind se deduce automáticamente de la URL del feed.
admin-caldav-username = Usuario de CalDAV
admin-caldav-password = Contraseña de CalDAV
admin-resource-teams = Equipos que pueden usar este recurso
admin-resource-teams-help = Los administradores de estos equipos pueden asociar el recurso a sus tipos de evento de equipo. Vacío: solo administración global.
admin-no-teams = Aún no hay equipos.
admin-save-resource = Guardar el recurso
admin-add-resource = Añadir un recurso
admin-jitsi = Jitsi (enlaces de reunión generados automáticamente)
admin-jitsi-help = Cuando la ubicación de un tipo de evento es «Jitsi (sala generada automáticamente)», calrs crea una URL de sala nueva para cada reserva añadiendo el patrón de abajo a tu URL base de Jitsi. No hace falta ninguna llamada a una API externa.
admin-display-name = Nombre visible
admin-jitsi-display-name-placeholder = p. ej. Meet DYB
admin-jitsi-display-name-help = Se muestra a los invitados en el selector de huecos y en el formulario de reserva. Si lo dejas vacío, se usa «Videollamada».
admin-room-pattern = Patrón del nombre de sala
admin-jitsi-disable-help = Deja vacía la URL base para desactivar la generación automática de Jitsi.
admin-save-jitsi = Guardar los ajustes de Jitsi
admin-meeting-webhook = Webhook de reunión (proveedor propio)
admin-webhook-url = URL del webhook
admin-webhook-display-name-placeholder = p. ej. Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Se muestra a los invitados en lugar de la etiqueta genérica «Videollamada».
admin-authentication = Autenticación
admin-auth-none = Ninguna
admin-auth-hmac = HMAC-SHA256 (cabecera X-Calrs-Signature)
admin-shared-secret = Secreto compartido
admin-webhook-disable-help = Deja vacía la URL para desactivar el webhook de reunión.
admin-save-webhook = Guardar los ajustes del webhook
admin-smtp = Ajustes de SMTP
admin-smtp-test-sent = Correo de prueba enviado.
admin-smtp-test-failed = No se ha podido enviar el correo de prueba. Revisa los registros del servidor y tus ajustes de SMTP.
admin-smtp-env-error = Error en la configuración de SMTP del entorno:
admin-smtp-host = Servidor:
admin-smtp-from = Remitente:
admin-smtp-enabled = SMTP activado
admin-host = Servidor
admin-port = Puerto
admin-tls-mode = Modo TLS
admin-tls-starttls = STARTTLS (puerto 587)
admin-tls-implicit = TLS implícito (puerto 465)
admin-tls-none = Ninguno, sin cifrar (solo MTA local)
admin-smtp-username-hint = (déjalo vacío para un relay sin autenticación)
admin-from-email = Correo del remitente
admin-from-name = Nombre del remitente
admin-save-smtp = Guardar los ajustes de SMTP
admin-send-test-email = Enviar un correo de prueba a
admin-send-test-email-hint = (por defecto, el correo de tu cuenta)
admin-send-test-email-btn = Enviar el correo de prueba
admin-smtp-clear-confirm = ¿Eliminar la configuración de SMTP guardada en la base de datos?
admin-clear-db-config = Borrar la configuración de la base de datos
admin-sms = Ajustes de SMS
admin-sms-help = Opcional. Solo se envían SMS en las reservas de tipos de evento con las «Notificaciones por SMS» activadas, y solo si el invitado ha dejado un número de teléfono.
admin-sms-test-sent = Mensaje de prueba enviado.
admin-sms-test-checked = Credenciales aceptadas.
admin-sms-test-error = La pasarela de SMS ha rechazado la petición.
admin-sms-captcha-warning = El formulario de reserva es público y el número de destino lo pone el invitado, así que un SMS sin captcha es un relay abierto que otra persona puede pagarte. Configura el captcha de arriba y restringe los países de destino en los ajustes de tu pasarela.
admin-sms-sent-today = Enviados hoy:
admin-sms-of-cap = de { $cap }
admin-sms-config-error = Error en la configuración de SMS:
admin-sms-gateway = Pasarela:
admin-sms-account = Cuenta:
admin-sms-sender = Remitente:
admin-sms-enabled = SMS activados
admin-sms-gateway-label = Pasarela
admin-required-on-switch = Obligatorio al cambiar de pasarela
admin-sms-docs = Documentación de la API de { $provider }
admin-sms-country = Prefijo de país por defecto
admin-sms-country-hint = (se usa cuando los invitados escriben un número local)
admin-sms-daily-cap = Límite diario
admin-sms-daily-cap-hint = (mensajes al día para toda la instancia, 0 para no poner límite)
admin-sms-daily-cap-help = Superado el límite, calrs deja de enviar SMS y sigue enviando correos, de modo que ninguna reserva falle porque se haya agotado el presupuesto de SMS.
admin-save-sms = Guardar los ajustes de SMS
admin-send-test-sms = Enviar un mensaje de prueba a
admin-send-test-sms-hint-check = (déjalo vacío para comprobar solo las credenciales)
admin-send-test-sms-hint-e164 = (formato E.164)
admin-test-gateway = Probar la pasarela
admin-sms-clear-confirm = ¿Eliminar la configuración de SMS guardada en la base de datos?
admin-sms-allow-all = Permitir que cualquier usuario active los SMS en sus tipos de evento
admin-sms-allow-all-help = Desactivado por defecto: los SMS gastan saldo de la cuenta configurada aquí, así que solo administración puede poner un tipo de evento en modo SMS.
admin-save-policy = Guardar la política
admin-page-of = Página %1 de %2
admin-show-more-js = Mostrar %1 más
admin-show-fewer = Mostrar menos

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = ¿Eliminar de forma permanente al usuario { $email }?{"\u000A"}{"\u000A"}Se borrarán su cuenta de usuario, su perfil de agenda, sus fuentes de calendario, sus tipos de evento y todos los datos que le pertenezcan en exclusiva. Las reservas pasadas se borrarán junto con sus tipos de evento.{"\u000A"}{"\u000A"}Para usuarios de OIDC/SSO: si el registro automático está activado, esta persona se volverá a crear la próxima vez que inicie sesión.{"\u000A"}{"\u000A"}Esta acción no se puede deshacer.
admin-system-settings-help = URL pública y ajustes de seguridad de red. También se pueden definir con las variables de entorno <code>CALRS_BASE_URL</code> y <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Si una variable de entorno está definida, <strong>tiene prioridad</strong> sobre el valor de abajo.
admin-set-by-env = — definido por el entorno ({ $var }), tiene prioridad sobre el valor guardado
admin-google-help = Para activar la integración con Google Calendar, crea credenciales OAuth2 en la <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Activa la <strong>API de Google Calendar</strong> y añade { $redirect_uri } como URI de redirección autorizada.
admin-room-pattern-help = Comodines disponibles: <code>{"{"}username{"}"}</code> (anfitrión), <code>{"{"}event{"}"}</code> (identificador del tipo de evento), <code>{"{"}date{"}"}</code> (AAAAMMDD), <code>{"{"}random{"}"}</code> (8 caracteres). Por defecto: { $default }.
admin-room-pattern-warning = Sin <code>{"{"}random{"}"}</code> el nombre de la sala es predecible: dos invitados que reserven el mismo tipo de evento el mismo día acabarán en la misma sala y podrán ver la reunión del otro. Las salas fijas están permitidas (por ejemplo, una sala personal por anfitrión), pero actívalo solo si entiendes lo que implica.
admin-meeting-webhook-help = Cuando la ubicación de un tipo de evento es «Webhook (proveedor propio)», calrs envía los datos de la reserva por POST a esta URL al confirmarla y espera como respuesta un cuerpo JSON <code>{"{"}"url": "https://..."{"}"}</code>.
admin-auth-hmac-help = Con HMAC, calrs envía <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> calculado sobre el cuerpo original de la petición.
admin-tls-none-warning = Elige <strong>Ninguno</strong> solo para un relay en esta misma máquina que no ofrezca STARTTLS o cuyo certificado sea autofirmado. El correo, y las credenciales que lleve, viajarán sin cifrar.
admin-smtp-env-error-help = Corrige las variables de entorno <code>CALRS_SMTP_*</code>, o quítalas para gestionar el SMTP desde la base de datos aquí.
admin-smtp-env-managed = Gestionado mediante <strong>variables de entorno</strong> (tienen prioridad sobre la base de datos). Cambia las variables <code>CALRS_SMTP_*</code>, o quítalas para gestionar el SMTP desde aquí.
admin-smtp-env-help = También puedes configurarlo con variables de entorno (que tienen prioridad sobre esto): <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> o <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Solo <code>CALRS_SMTP_HOST</code> y <code>CALRS_SMTP_FROM_EMAIL</code> son obligatorias; omite el usuario y la contraseña para retransmitir a través de un MTA local sin autenticación.
admin-sms-env-error-help = Corrige las variables de entorno <code>CALRS_SMS_*</code>, o quítalas para gestionar los SMS desde la base de datos aquí.
admin-sms-env-managed = Gestionado mediante <strong>variables de entorno</strong> (tienen prioridad sobre la base de datos). Cambia las variables <code>CALRS_SMS_*</code>, o quítalas para gestionar los SMS desde aquí.
admin-sms-env-help = También puedes configurarlo con variables de entorno (que tienen prioridad sobre esto): <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>El modo de prueba de Twilio está activado</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Los invitados reciben la plantilla predefinida de Twilio <code>sms_appointment_reminders</code> en lugar del mensaje real, y solo se llega a los números verificados en tu consola de Twilio. Es una ayuda para probar con cuentas de prueba. Quita la variable antes de aceptar reservas.

admin-show-more =
    { $count ->
        [one] Mostrar { $count } más
       *[other] Mostrar { $count } más
    }

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Elige el protocolo que habla tu servidor. EWS está pensado para Exchange 2019/2016/2013 instalado en tus servidores.

admin-sms-going-live = <strong>Antes de ponerlo en producción:</strong> restringe los países de destino en tu pasarela (en Twilio se llama Geo Permissions), mantén la cuenta de prepago sin recarga automática, y deja el captcha activado. Entre esas tres medidas queda acotado lo que puede costarte un intento de SMS pumping.

troubleshoot-heading = Diagnóstico de disponibilidad

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = El nombre y el identificador son obligatorios.
form-error-team-name-length = El nombre no puede superar los 255 caracteres.
form-error-team-description-length = La descripción no puede superar los 5000 caracteres.
form-error-slug-charset = El identificador solo puede contener minúsculas, números y guiones.
form-error-slug-reserved = Este identificador está reservado. Por favor, elige otro.
form-error-team-slug-taken = Ya existe un equipo con este identificador.
form-error-title-required = Hace falta un título para generar el identificador.
form-error-event-type-slug-taken = Ya existe un tipo de evento con este identificador.
form-error-event-type-slug-taken-team = Ya existe un tipo de evento con este identificador en este equipo.
form-error-location-required = Los datos de la ubicación son obligatorios (por ejemplo, un enlace de videollamada, un teléfono o una dirección).
form-error-not-team-admin = No eres administrador de este equipo.
form-error-no-account = No se ha encontrado ningún perfil de agenda. Por favor, habla con administración.
form-error-all-fields-required = Todos los campos son obligatorios.
form-error-encryption = Error de cifrado.
form-error-connection-failed = La conexión ha fallado: { $error }. Revisa la URL y las credenciales, o marca «Omitir la prueba de conexión» para guardar de todos modos.

# Settings page flash (src/web/mod.rs)

settings-saved = Ajustes guardados.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = El nombre debe tener entre 1 y 255 caracteres.
settings-error-username-length = El nombre de usuario debe tener al menos 2 caracteres.
settings-error-username-taken = Ese nombre de usuario ya está en uso.
settings-error-booking-email = Por favor, introduce una dirección de correo de reservas válida.
settings-error-save-failed = No se han podido guardar los ajustes.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = No se ha encontrado el equipo, o no eres administrador de él.
error-team-not-found = No se ha encontrado el equipo.
error-event-type-not-found = No se ha encontrado el tipo de evento.
error-decrypt-failed = No se han podido descifrar las credenciales guardadas.
error-source-not-found = No se ha encontrado la fuente.
error-source-no-password = Esta fuente no tiene contraseña guardada.
error-oauth-invalid-state = Parámetro de estado no válido. Por favor, inténtalo de nuevo.
error-oauth-no-code = No se ha recibido ningún código de autorización.
error-oauth-not-configured = Google OAuth2 no está configurado.
error-no-scheduling-account = No se ha encontrado ningún perfil de agenda.
error-private-event-type-not-found = No se ha encontrado el tipo de evento privado.
error-access-denied = Acceso denegado.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = Este hueco ya no está disponible.
error-slot-too-soon = Este hueco ya no está disponible (demasiado pronto).
error-slot-beyond-horizon = Este hueco queda fuera del periodo de reserva.
error-invite-required = Este tipo de evento requiere un enlace de invitación.
error-invite-invalid = Enlace de invitación no válido.
error-invite-expired = Este enlace de invitación ha caducado.
error-invite-used = Este enlace de invitación ya se ha usado.
error-invalid-date = Fecha no válida.
error-invalid-time = Hora no válida.
error-invalid-date-format = Formato de fecha no válido.
error-invalid-time-format = Formato de hora no válido.
error-too-many-bookings = Demasiados intentos de reserva. Por favor, inténtalo de nuevo en unos minutos.
error-too-many-requests = Demasiadas peticiones. Por favor, inténtalo de nuevo más tarde.
error-no-members-available = Ningún miembro del equipo está disponible para este hueco.
error-dynamic-group-public-only = Los enlaces de grupo dinámicos solo están disponibles para tipos de evento públicos.
error-user-not-found = Usuario no encontrado.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Ha fallado la verificación del captcha
bae-title-invalid-booking = Datos de la reserva no válidos
bae-title-unavailable = No disponible ahora mismo
bae-title-cannot-approve = No se puede aprobar esta reserva
bae-title-invalid-link = Enlace no válido
bae-title-invalid-or-expired = Enlace no válido o caducado
bae-title-booking-not-found = Reserva no encontrada
bae-title-already-approved = Ya aprobada
bae-title-already-declined = Ya rechazada
bae-title-already-cancelled = Ya cancelada
bae-title-booking-cancelled = Reserva cancelada
bae-title-booking-declined = Reserva rechazada

# Booking action error page: bodies

bae-body-go-back = Por favor, vuelve atrás e inténtalo de nuevo.
bae-body-unavailable = El anfitrión no acepta más reservas para esta fecha. Por favor, elige otra fecha o vuelve más tarde.
bae-body-resource-gone = Un recurso necesario ya no está disponible a esta hora. Pide al invitado que elija otro hueco.
bae-body-no-claim-token = No se ha proporcionado ningún token.
bae-body-claim-invalid = Este enlace ya no es válido.
bae-body-booking-gone = Esta reserva ya no existe.
bae-body-decline-link-invalid = Este enlace de rechazo no es válido, ha caducado, o la reserva ya se ha procesado.
bae-body-cancel-link-invalid = Este enlace de cancelación no es válido, ha caducado, o la reserva ya se ha cancelado.
bae-body-cancel-link-invalid-short = Este enlace de cancelación no es válido o ha caducado.
bae-body-reschedule-link-invalid = Este enlace de reprogramación no es válido, ha caducado, o la reserva ya se ha procesado.
bae-body-approval-link-invalid = Este enlace de aprobación no es válido o ha caducado.
bae-body-already-approved = Esta reserva ya se ha aprobado.
bae-body-already-declined = Esta reserva ya se ha rechazado.
bae-body-already-cancelled = Esta reserva ya se ha cancelado.
bae-body-was-cancelled = Esta reserva se canceló.
bae-body-declined-by-host = El anfitrión ha rechazado esta reserva.

# Booking form validation (src/web/mod.rs)

validate-name-length = El nombre debe tener entre 1 y 255 caracteres.
validate-email-length = El correo debe tener entre 1 y 255 caracteres.
validate-email-invalid = Por favor, introduce una dirección de correo válida.
validate-notes-length = Las notas no pueden superar los 5000 caracteres.
validate-date-too-far = No se puede reservar con más de un año de antelación.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = Este tipo de evento no admite invitados adicionales.
guests-too-many =
    { $max ->
        [one] Puedes añadir como máximo un invitado adicional.
       *[other] Puedes añadir como máximo { $max } invitados adicionales.
    }
guests-invalid-email = Correo de invitado adicional no válido: { $email }
dynamic-group-min-usernames = Los enlaces de grupo dinámico necesitan al menos dos nombres de usuario.
dynamic-group-user-not-found = Usuario «{ $username }» no encontrado.
dynamic-group-user-opted-out = El usuario «{ $username }» no ha activado los enlaces de grupo dinámico.

error-slot-unavailable-member = Este hueco ya no está disponible ({ $username } tiene un conflicto).
