# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Agendamento pendente
confirmed-page-title-booked = Agendamento confirmado

confirmed-heading-reschedule-requested = Reagendamento solicitado
confirmed-heading-rescheduled = Reagendado!
confirmed-heading-pending = Aguardando confirmação
confirmed-heading-booked = Está agendado!

confirmed-subtitle-reschedule-requested = Seu pedido de reagendamento foi enviado para { $host }. Você receberá um e-mail em { $email } assim que for aprovado.
confirmed-subtitle-rescheduled = Seu agendamento foi remarcado. Um e-mail de confirmação foi enviado para { $email }.
confirmed-subtitle-pending = Sua solicitação de agendamento foi enviada para { $host }. Você receberá um e-mail em { $email } assim que for confirmada.
confirmed-subtitle-booked = Um e-mail de confirmação foi enviado para { $email }.

confirmed-detail-event = Evento:
confirmed-detail-date = Data:
confirmed-detail-time = Hora:
confirmed-detail-with = Com:
confirmed-detail-location = Local:
confirmed-detail-notes = Notas:
confirmed-detail-additional-guests = Convidados adicionais:

confirmed-book-another = Agendar outro horário

# Slot picker (templates/slots.html)

slots-location-video = Chamada de vídeo
slots-location-phone = Ligação

slots-tz-label = Seu fuso horário
slots-time-format-label = Formato de horário

slots-view-month = Visualização mensal
slots-view-week = Visualização semanal
slots-view-column = Visualização em colunas

slots-weekday-mon = Seg
slots-weekday-tue = Ter
slots-weekday-wed = Qua
slots-weekday-thu = Qui
slots-weekday-fri = Sex
slots-weekday-sat = Sáb
slots-weekday-sun = Dom

slots-weekday-mon-short = S
slots-weekday-tue-short = T
slots-weekday-wed-short = Q
slots-weekday-thu-short = Q
slots-weekday-fri-short = S
slots-weekday-sat-short = S
slots-weekday-sun-short = D

slots-select-date = Selecione uma data
slots-loading-availability = Carregando disponibilidade...
slots-click-highlighted = Selecione uma data destacada para ver horários disponíveis
slots-no-times-month = Sem horários disponíveis nesse mês
slots-no-times-day = Sem horários disponíveis nesse dia
slots-no-availability-participants = Nenhuma disponibilidade encontrada para todos os participantes neste mês
slots-week-more = mais

# Booking form (templates/book.html)

book-page-title = Agendar { $title }
book-back-to-times = Voltar aos horários
book-name-label = Seu nome
book-name-placeholder = Maria Silva
book-email-label = E-mail
book-email-placeholder = maria@exemplo.com
book-email-invalid = Por favor preencha um e-mail completo, incluindo o domínio (ex. maria@exemplo.com).
book-notes-label = Notas
book-notes-optional = (opcional)
book-notes-placeholder = Gostaria de conversar sobre algo em particular?
book-additional-guests-label = Convidados adicionais
book-additional-guests-hint = (opcional, até { $max })
book-add-guest-btn = + Adicionar e-mail de convidado
book-guest-email-placeholder = colega@exemplo.com
captcha-label = Verificação de segurança
captcha-initial-state = Confirme que você é humano
captcha-verifying = Verificando...
captcha-solved = Você é humano
captcha-error = Erro
captcha-troubleshooting = Solução de problemas
captcha-wasm-disabled = Ative o WASM para resolver muito mais rapidamente
captcha-verify-aria = Clique para confirmar que você é humano
captcha-verifying-aria = Verificando, por favor espere
captcha-verified-aria = Verificado
captcha-required = Por favor, confirme que você é humano
captcha-error-aria = Ocorreu um erro, por favor tente novamente
book-confirm-button = Confirmar agendamento

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Convidado:
common-detail-reason = Motivo:
common-reason-optional = (opcional)
common-close-page = Você pode fechar a página.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Cancelar agendamento
cancel-heading = Cancelar agendamento
cancel-subtitle = Você está prestes a cancelar seu agendamento.
cancel-reason-label = Motivo
cancel-reason-placeholder-host = Explique ao anfitrião o motivo...
cancel-button = Cancelar agendamento
cancelled-heading = Agendamento cancelado
cancelled-subtitle = Seu agendamento foi cancelado e o anfitrião foi notificado.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Recusar agendamento
decline-heading = Recusar agendamento
decline-subtitle = Você está prestes a recusar esta solicitação de agendamento.
decline-reason-placeholder-guest = Explique ao convidado o motivo...
decline-button = Recusar agendamento
declined-heading = Agendamento recusado
declined-subtitle = O agendamento foi recusado e o convidado foi notificado.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Aprovar agendamento
approve-heading = Aprovar agendamento
approve-subtitle = Você está prestes a aprovar esta solicitação de agendamento.
approve-button = Aprovar agendamento
approved-heading = Agendamento aprovado
approved-subtitle = O agendamento foi confirmado e um e-mail de confirmação foi enviado para { $email }.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Assumir agendamento
claim-heading = Assumir agendamento
claim-subtitle = Você está prestes a assumir este agendamento. Você será adicionado como participante.
claim-assigned-to = Atribuído a:
claim-button = Assumir este agendamento
claimed-page-title = Agendamento assumido
claimed-heading = Agendamento assumido
claimed-subtitle = Você assumiu este agendamento. Um convite de calendário foi enviado para o seu e-mail.
already-claimed-page-title = Já foi assumido
already-claimed-heading = Já foi assumido
already-claimed-subtitle = Este agendamento já foi assumido por { $name }.

# Generic error page (booking_action_error.html)

action-error-page-title = Erro na ação do agendamento

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Reagendar agendamento — calrs
host-resched-heading = Reagendar agendamento
host-resched-subtitle = Isso enviará um e-mail para { $guest } pedindo que escolha um novo horário.
host-resched-currently = Atualmente:
host-resched-button = Enviar pedido de reagendamento
host-resched-cancel-link = Cancelar

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Confirmar reagendamento
resched-confirm-heading = Confirmar reagendamento
resched-confirm-subtitle = Você está prestes a mover seu agendamento para um novo horário.
resched-was = Antes:
resched-new = Novo:
resched-button = Confirmar reagendamento
resched-back-to-picker = Voltar à seleção de horários

# Base layout chrome (templates/base.html)

base-loader-checking = Verificando disponibilidade
base-loader-please-wait = Aguarde, carregando os dados mais recentes do calendário...
base-stop-impersonating = Encerrar personificação
base-theme-toggle = Alternar tema
base-powered-by = Desenvolvido com

# Profile (templates/profile.html)

profile-pick-event-type-invite = Escolha um tipo de evento para agendar um horário.
profile-no-event-type = Nenhum tipo de evento disponível ainda.

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

common-month-1 = janeiro
common-month-2 = fevereiro
common-month-3 = março
common-month-4 = abril
common-month-5 = maio
common-month-6 = junho
common-month-7 = julho
common-month-8 = agosto
common-month-9 = setembro
common-month-10 = outubro
common-month-11 = novembro
common-month-12 = dezembro

common-weekday-long-mon = segunda-feira
common-weekday-long-tue = terça-feira
common-weekday-long-wed = quarta-feira
common-weekday-long-thu = quinta-feira
common-weekday-long-fri = sexta-feira
common-weekday-long-sat = sábado
common-weekday-long-sun = domingo

# Format patterns are parametric per locale to handle word order. Translators
# pick where each placeholder lands. Example outputs:
#   EN: April 2026  /  Tuesday, March 12, 2026
#   FR: avril 2026  /  mardi 12 mars 2026
#   PT: abril de 2026  /  quinta-feira, 12 de março de 2026
common-format-month-year = { $month } de { $year }
common-format-long-date = { $weekday }, { $day } de { $month } de { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Reagendar
email-action-cancel-booking = Cancelar agendamento

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
email-confirm-greeting = Olá { $name },
email-confirm-headline = Seu agendamento foi confirmado!
email-confirm-ics-attached-plain = Um convite de calendário está anexado.
email-confirm-ics-attached-html = Um convite de calendário está anexado a este e-mail.
email-confirm-need-to-cancel = Precisa cancelar? { $url }

# Email: guest reminder

email-reminder-subject = Lembrete: { $event } às { $time }
email-reminder-headline = Sua reunião está próxima.

# Email: guest cancellation

email-cancel-subject = Cancelado: { $event } — { $date }
email-cancel-headline-by-host = Seu agendamento foi cancelado por { $host }.
email-cancel-headline-by-guest = Seu agendamento foi cancelado.
email-cancel-ics-attached-plain = Um cancelamento de calendário está anexado.
email-cancel-ics-attached-html = Um cancelamento de calendário está anexado a este e-mail.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Observação: o cancelamento exige pelo menos { $minutes } minutos de antecedência.
email-confirm-reschedule-notice = Observação: o reagendamento exige pelo menos { $minutes } minutos de antecedência.

# Event type form: cancel/reschedule minimum notice (templates/event_type_form.html)

event-type-form-cancel-notice-label = Antecedência mínima para cancelar
event-type-form-reschedule-notice-label = Antecedência mínima para reagendar
event-type-form-notice-help = Deixe em branco para não restringir.
event-type-form-resources-label = Recursos necessários
event-type-form-resources-hint = Os horários só são oferecidos quando os recursos selecionados estão disponíveis, conforme o modo abaixo.
event-type-form-resources-mode-all = Todos os recursos selecionados devem estar livres
event-type-form-resources-mode-round-robin = Basta um recurso livre (ele é atribuído ao agendamento)
event-type-form-notice-unit-minutes = minutos
event-type-form-notice-unit-hours = horas
event-type-form-notice-unit-days = dias

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = O cancelamento exige pelo menos { $minutes } minutos de antecedência em relação à reunião.
confirmed-reschedule-notice-info = O reagendamento exige pelo menos { $minutes } minutos de antecedência em relação à reunião.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Este agendamento não pode mais ser cancelado online
booking-blocked-title-reschedule = Este agendamento não pode mais ser reagendado online
booking-blocked-body = O anfitrião exige pelo menos { $minutes } minutos de antecedência. Se não puder comparecer, envie um e-mail diretamente para <a href="mailto:{ $host_email }">{ $host_email }</a>.
