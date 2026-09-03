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

confirmed-add-to-calendar = Adicionar à agenda

# Slot picker (templates/slots.html)

slots-location-video = Chamada de vídeo
slots-location-phone = Ligação
slots-location-google-meet = Google Meet

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
book-email-invalid = Por favor, preencha um e-mail completo, incluindo o domínio (ex. maria@exemplo.com).
book-notes-label = Notas
book-notes-optional = (opcional)
book-notes-placeholder = Gostaria de conversar sobre algo em particular?
book-additional-guests-label = Convidados adicionais
book-additional-guests-hint = (opcional, até { $max })
book-add-guest-btn = + Adicionar e-mail de convidado
book-guest-email-placeholder = colega@exemplo.com
book-phone-label = Número de telefone
book-phone-placeholder = (11) 91234-5678
book-phone-help = Números locais funcionam; sem o + inicial, assumimos { $country }.
book-phone-optional-consequence = Deixe em branco se preferir não receber mensagens de texto sobre este agendamento.
book-phone-required = Este agendamento exige um número de telefone.
book-phone-invalid-title = Número de telefone inválido
book-phone-invalid = Por favor, informe um número para o qual possamos enviar SMS, ou deixe o campo em branco.
book-phone-country-search = Buscar
book-phone-country-label = Selecione o país
book-phone-country-none = Nenhum país selecionado
book-phone-country-no-results = Nenhum país corresponde a essa busca
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

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Agendamento confirmado: { $event }, { $date } às { $time } ({ $tz }).
sms-cancelled = Agendamento cancelado: { $event }, { $date } às { $time } ({ $tz }).
sms-rescheduled = Agendamento remarcado: { $event } agora é { $date } às { $time } ({ $tz }).
sms-reminder = Lembrete: { $event } começa { $date } às { $time } ({ $tz }).

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
profile-no-event-type = Ainda não há tipos de evento disponíveis.

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
#   ES: abril 2026  /  martes, 12 de marzo de 2026
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


# Google Meet (English placeholders until translated)
event-type-form-location-google-meet = Google Meet (auto-generated link)
event-type-form-location-google-meet-hint = A unique Google Meet link is created on confirmation, owned by the assigned host. Every host (you, or every eligible team member) must have Google Calendar connected with a write-back calendar selected.
google-meet-prereq-no-host = Google Meet requires a host with Google Calendar connected.
google-meet-prereq-no-eligible = Google Meet requires at least one eligible team member with Google Calendar connected.
google-meet-prereq-missing = Google Meet requires every host to have Google Calendar connected with a write-back calendar selected. Still missing: { $names }. Connect them at Dashboard → Calendar sources.
google-meet-unavailable-title = Google Meet is not available
google-meet-dynamic-group-unavailable = The host needs Google Calendar connected with a write-back calendar selected.

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
event-type-form-booking-horizon-label = Horizonte de agendamento
event-type-form-booking-horizon-help = Com quantos dias de antecedência os convidados podem agendar. Em branco para não limitar, 0 para apenas hoje.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = O cancelamento exige pelo menos { $minutes } minutos de antecedência em relação à reunião.
confirmed-reschedule-notice-info = O reagendamento exige pelo menos { $minutes } minutos de antecedência em relação à reunião.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Este agendamento não pode mais ser cancelado online
booking-blocked-title-reschedule = Este agendamento não pode mais ser reagendado online
booking-blocked-body = O anfitrião exige pelo menos { $minutes } minutos de antecedência. Se não puder comparecer, envie um e-mail diretamente para <a href="mailto:{ $host_email }">{ $host_email }</a>.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Copiar
dashboard-event-types-copied = Copiado!
dashboard-event-types-copy-title = Copiar link de agendamento
dashboard-event-types-copy-failed = Falha ao copiar

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Agendamento
nav-overview = Visão geral
nav-event-types = Tipos de evento
nav-bookings = Agendamentos
nav-teams = Equipes
nav-section-shared-links = Links compartilhados
nav-invite-links = Links de convite
nav-section-calendars = Agendas
nav-sources = Fontes
nav-section-personal = Pessoal
nav-settings = Perfil e configurações
nav-troubleshoot = Diagnóstico
nav-section-admin = Administração
nav-admin-panel = Painel de administração
nav-sign-out = Sair
nav-release-notes = Ver as notas da versão

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = O fuso horário do seu navegador é { $detected }, mas o seu fuso de agendamento está definido como { $current }.
tz-banner-update = Atualizar
tz-banner-dismiss = Dispensar

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = Informe a URL:
editor-link-default-label = texto do link
editor-placeholder-text = texto
editor-nothing-to-preview = Nada para pré-visualizar

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Painel
overview-welcome = Olá, { $name }
overview-public-page = Página pública:
overview-avail-banner-title = Disponibilidade padrão
overview-avail-banner-body = Seu horário de trabalho padrão foi definido de segunda a sexta, das 9h às 17h. Ele é usado quando outras pessoas incluem você em reuniões de grupo dinâmicas.
overview-avail-banner-cta = Revise sua disponibilidade
overview-dismiss = Dispensar
overview-getting-started = Primeiros passos
overview-getting-started-help = Siga estes passos para começar a aceitar agendamentos.
overview-step-connect-calendar = Conectar uma agenda
overview-step-first-event-type = Criar seu primeiro tipo de evento
overview-step-share-link = Compartilhar seu link de agendamento
overview-pending-approval = Aguardando aprovação
overview-booking-with = { $title } com { $guest }
overview-badge-pending = pendente
overview-guest-booked = Agendado pelo convidado:
overview-confirm = Confirmar
overview-decline = Recusar
overview-stat-event-types = Tipos de evento
overview-stat-upcoming = Próximos agendamentos
overview-stat-pending = Aguardando aprovação
overview-stat-sources = Fontes de agenda
overview-quick-actions = Criar um tipo de evento
overview-action-public-title = Página de agendamento pública
overview-action-public-desc = Compartilhe um link: qualquer pessoa pode escolher um horário e agendar com você.
overview-action-team-title = Agendamento de equipe
overview-action-team-desc = Distribua os agendamentos entre os membros da equipe ou encontre um horário em que todos estejam livres.
overview-action-team-desc-empty = Crie primeiro uma equipe e depois configure tipos de evento compartilhados.
overview-action-private-title = Privado, apenas por convite
overview-action-private-desc = Gere links de uso único para contatos específicos. Mais ninguém consegue agendar.
overview-action-shared-title = Links de convite compartilhados
overview-action-shared-desc = Qualquer colega da equipe pode gerar links de agendamento para compartilhar externamente.
overview-action-reason-calendar = Conecte uma agenda primeiro
overview-action-reason-ask-admin = Peça a um administrador que crie uma equipe
overview-action-reason-team-admin = Requer uma equipe: crie uma primeiro
overview-action-reason-team-member = Requer uma equipe: peça a um administrador

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Agendamentos
bookings-pending-approval = Aguardando aprovação
bookings-available-to-claim = Disponíveis para assumir
bookings-upcoming = Próximos agendamentos
bookings-with = { $title } com { $guest }
bookings-guest-booked = Agendado pelo convidado:
bookings-resource = Recurso:
bookings-confirm = Confirmar
bookings-reschedule = Remarcar
bookings-decline = Recusar
bookings-claim = Assumir
bookings-badge-awaiting-reschedule = remarcação pendente
bookings-cancel = Cancelar
bookings-reason-placeholder = Motivo (opcional)
bookings-confirm-cancel = Confirmar o cancelamento
bookings-back = Voltar
bookings-empty = Ainda não há agendamentos futuros.<br>Compartilhe seus { $link } para que outras pessoas possam agendar com você.
bookings-empty-link-label = links de tipos de evento

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Equipes
teams-heading = Equipes
teams-new = Nova
teams-badge-public = pública
teams-badge-private = privada
teams-settings = Configurações
teams-view = Ver
teams-empty = Ainda não há equipes.
teams-empty-admin = { $link } para colaborar com sua equipe.
teams-empty-admin-link-label = Crie uma
teams-empty-member = As equipes são criadas pelos administradores. Peça a eles que criem uma e adicionem você como membro.

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Links de convite
invite-links-heading = Links de convite
invite-links-new = Novo evento interno
invite-links-help = Gere links de agendamento de uso único para tipos de evento internos. Qualquer colega autenticado pode criar e compartilhar links aqui.
invite-links-duration = { $minutes } min
invite-links-hosted-by = Organizado por { $host }
invite-links-get-link = Obter link
invite-links-invites = Convites
invite-links-empty = Ainda não há tipos de evento internos.<br>{ $link } com visibilidade «Interno» para que qualquer colega possa gerar links de agendamento.
invite-links-empty-link-label = Crie um tipo de evento
invite-links-js-generating = Gerando...
invite-links-js-copied = Copiado!
invite-links-js-error = Erro

teams-member-count =
    { $count ->
        [one] { $count } membro
       *[other] { $count } membros
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Fontes de agenda
sources-heading = Fontes de agenda
sources-add = Adicionar
sources-last-sync = Última sincronização:
sources-sync = Sincronizar
sources-full-resync = Ressincronização completa
sources-full-resync-title = Limpar o cache e baixar novamente todos os eventos do servidor
sources-test = Testar
sources-reconnect = Reconectar
sources-reconnect-title = Refazer o fluxo de consentimento do Google
sources-edit = Editar
sources-remove = Remover
sources-remove-confirm = Remover a fonte «{ $name }»? Isso apagará todos os eventos sincronizados a partir dela.
sources-no-write-calendar = Nenhuma agenda de gravação selecionada. Os agendamentos confirmados ficam no calrs e não são enviados para esta agenda. Escolha uma abaixo para ativar a gravação.
sources-write-bookings-to = Gravar os agendamentos em:
sources-write-none = Nenhuma (não gravar)
sources-empty = Nenhuma fonte de agenda conectada. { $link } para verificar a disponibilidade.
sources-empty-link-label = Adicione uma

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Tipos de evento
event-types-heading = Tipos de evento
event-types-new = Novo
event-types-badge-disabled = desativado
event-types-badge-internal = interno
event-types-badge-private = privado
event-types-badge-resources = recursos
event-types-send-invites = Enviar convites
event-types-duration = { $minutes } min
event-types-mode-collective = coletivo
event-types-mode-round-robin = rodízio
event-types-edit = Editar
event-types-disable = Desativar
event-types-enable = Ativar
event-types-embed = Incorporar
event-types-overrides = Exceções
event-types-team-settings = Configurações da equipe
event-types-invites = Convites
event-types-view-public = Ver a página pública
event-types-view-page = Ver a página
event-types-delete = Excluir
event-types-delete-confirm = Excluir o tipo de evento «{ $title }»? Esta ação não pode ser desfeita.
event-types-empty = Ainda não há tipos de evento. { $link } para começar a aceitar agendamentos.
event-types-empty-link-label = Crie um

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Negrito (Ctrl+B)
editor-italic = Itálico (Ctrl+I)
editor-strikethrough = Tachado
editor-code = Código embutido
editor-link = Inserir link (Ctrl+K)
editor-toggle-preview = Mostrar ou ocultar a pré-visualização
editor-preview = Pré-visualização

# Profile and settings (templates/settings.html)

settings-page-title = Configurações
settings-heading = Perfil e configurações
settings-public-page-label = Sua página de agendamento pública
settings-copy = Copiar
settings-copied = Copiado!
settings-open = Abrir
settings-avatar = Avatar
settings-upload = Enviar
settings-remove = Remover
settings-display-name = Nome de exibição
settings-display-name-placeholder = Seu nome
settings-username = Nome de usuário
settings-username-hint = (usado na sua URL de agendamento)
settings-username-pattern-title = Apenas letras minúsculas, números e hifens
settings-username-help = Sua página de agendamento pública:
settings-title = Cargo
settings-title-placeholder = ex.: Engenheira de software, Gerente de produto
settings-title-help = Aparece no seu perfil público e na barra lateral.
settings-bio = Biografia
settings-bio-placeholder = Conte um pouco sobre você...
settings-bio-help = Aparece na sua página de agendamento pública. Aceita **negrito**, *itálico*, ~~tachado~~, `código` e [links](url).
settings-booking-email = E-mail de agendamento
settings-booking-email-help = Este endereço aparecerá nas suas páginas de agendamento públicas e nas notificações por e-mail. Deixe em branco para usar o e-mail de acesso.
settings-booking-email-warning = Verifique se este endereço existe no seu provedor de e-mail. Caso contrário, as notificações não serão entregues.
settings-timezone = Fuso horário
settings-timezone-help = Suas regras de disponibilidade e horários de agendamento são calculados neste fuso horário.
settings-language = Idioma
settings-language-auto = Automático (idioma do navegador)
settings-language-help = Escolha um idioma para a interface, ou deixe em Automático para seguir a configuração do navegador.
settings-dynamic-group = Permitir que outras pessoas me incluam em links de grupo dinâmicos
settings-dynamic-group-help = Quando ativado, outros usuários podem criar URLs de reunião coletiva improvisadas que incluem você (ex.: { $example }).
settings-lend-resource = Emprestar meu acesso à agenda para reservas de recursos
settings-lend-resource-help = Quando um agendamento precisar reservar um recurso compartilhado (laboratório de demonstração, sala de reunião) no qual sua conta de agenda pode gravar, permita que o calrs use suas credenciais salvas para essa gravação.
settings-default-availability = Disponibilidade padrão
settings-default-availability-help = Seu horário de trabalho padrão. Usado nos links de grupo dinâmicos quando outras pessoas incluem você em uma reunião.
settings-copy-to-all = Copiar para todos os dias
settings-copy-to-all-title = Copiar as faixas do primeiro dia ativado para todos os outros dias ativados
settings-add-window = Adicionar faixa de horário
settings-remove-window = Remover a faixa
settings-save = Salvar as configurações
settings-appearance = Aparência
settings-theme-system = Sistema
settings-theme-light = Claro
settings-theme-dark = Escuro

# Sign in (templates/auth/login.html)

login-page-title = Entrar
login-heading = Entrar
login-subtitle = Entre na sua conta do calrs
login-sso = Entrar com SSO
login-or = ou
login-email = E-mail
login-password = Senha
login-submit = Entrar com e-mail
login-no-account = Ainda não tem uma conta? { $link }
login-register-link = Cadastre-se

# Registration (templates/auth/register.html)

register-page-title = Cadastro
register-heading = Criar uma conta
register-subtitle = Cadastre uma nova conta do calrs
register-domains-limited = O cadastro está limitado a: { $domains }
register-name = Nome
register-name-placeholder = Seu nome
register-email = E-mail
register-password = Senha
register-password-hint = (mín. 12 caracteres)
register-submit = Criar uma conta
register-have-account = Já tem uma conta? { $link }
register-signin-link = Entre

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Muitas tentativas de login. Por favor, tente novamente mais tarde.
auth-error-invalid-credentials = E-mail ou senha inválidos
auth-error-internal = Erro interno
auth-error-registration-disabled = O cadastro está desativado.
auth-error-name-length = O nome deve ter entre 1 e 255 caracteres
auth-error-email-length = O e-mail deve ter entre 1 e 255 caracteres
auth-error-email-invalid = Por favor, informe um endereço de e-mail válido
auth-error-email-domain = Domínio de e-mail não permitido
auth-error-password-length = A senha deve ter pelo menos 12 caracteres
auth-error-email-taken = Este e-mail já está cadastrado
auth-error-create-failed = Não foi possível criar a conta

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Fonte de agenda
source-test-sync-heading = Sincronização: { $name }
source-test-heading = Teste de conexão
source-write-page-title = Configurar a gravação na agenda
source-write-back = Voltar ao painel
source-write-heading = Onde os agendamentos devem ser gravados?
source-write-help = Quando alguém agendar uma reunião com você, o calrs pode criar o evento automaticamente na sua agenda. Escolha em qual agenda gravar os agendamentos de { $name }.
source-write-save = Salvar
source-write-skip = Pular por enquanto
source-write-sync-results = Resultados da sincronização

source-write-event-count =
    { $count ->
        [one] { $count } evento
       *[other] { $count } eventos
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Exceções por data
overrides-heading = Exceções por data
overrides-back-teams = Voltar às equipes
overrides-back-event-types = Voltar aos tipos de evento
overrides-intro = Adicione exceções para datas específicas em { $title }
overrides-add-heading = Adicionar uma exceção
overrides-date = Data
overrides-type = Tipo de exceção
overrides-type-blocked = Bloquear o dia inteiro
overrides-type-custom = Horário personalizado
overrides-start-time = Horário de início
overrides-end-time = Horário de término
overrides-add-submit = Adicionar a exceção
overrides-existing = Exceções existentes
overrides-badge-blocked = bloqueada
overrides-badge-custom = horário personalizado
overrides-delete = Excluir
overrides-delete-confirm = Excluir esta exceção?
overrides-empty = Ainda não há exceções por data.<br>Use o formulário acima para bloquear datas específicas (feriados, folgas) ou definir um horário personalizado.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Escolha um tipo de evento para agendar um horário.
team-profile-empty = Ainda não há tipos de evento disponíveis.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Diagnóstico
troubleshoot-empty = Nenhum tipo de evento encontrado. { $link } para começar a diagnosticar a disponibilidade.
troubleshoot-empty-link-label = Crie um
troubleshoot-subtitle = Veja por que os horários de { $title } estão disponíveis ou bloqueados
troubleshoot-duration = { $minutes } min
troubleshoot-buffer-before = { $minutes } min de intervalo antes
troubleshoot-buffer-after = { $minutes } min de intervalo depois
troubleshoot-min-notice = { $minutes } min de antecedência
troubleshoot-blocked-override = Bloqueado por uma exceção de data (folga)
troubleshoot-custom-hours-active = Exceção de horário personalizado ativa (substitui as regras semanais)
troubleshoot-legend-available = Disponível
troubleshoot-legend-calendar-event = Evento da agenda
troubleshoot-legend-booking = Agendamento
troubleshoot-legend-resource = Recurso ocupado
troubleshoot-legend-outside = Fora do horário
troubleshoot-legend-buffer = Intervalo / antecedência mínima
troubleshoot-blocked-slots = Horários bloqueados
troubleshoot-none-date-blocked = Esta data está bloqueada por uma exceção de disponibilidade (folga). Nenhum horário disponível.
troubleshoot-none-custom-hours = Há uma exceção de horário personalizado ativa, mas nenhuma faixa corresponde. Verifique as configurações da exceção.
troubleshoot-none-no-rules = Não há regras de disponibilidade para este dia da semana. Este tipo de evento não pode ser agendado em { $date }.
troubleshoot-none-all-bookable = Nenhum horário bloqueado dentro do período de disponibilidade. Todos os horários podem ser agendados.
troubleshoot-label-outside = Fora da disponibilidade
troubleshoot-label-available = Disponível
troubleshoot-label-min-notice = Antecedência mínima ({ $minutes } min)
troubleshoot-label-beyond-horizon = Além do horizonte de agendamento ({ $days } dias)
troubleshoot-label-buffer = Intervalo ({ $minutes } min)
troubleshoot-label-resource-busy = Recurso ocupado: { $names }
troubleshoot-detail-around = Em torno de: { $label }
troubleshoot-detail-around-booking = Em torno do agendamento de { $guest }
troubleshoot-reason-calendar-event = Evento da agenda: { $label }
troubleshoot-reason-booking = Agendamento: { $label }

# Invite management (templates/invite_form.html)

invites-heading = Convites
invites-back-teams = Voltar às equipes
invites-back-event-types = Voltar aos tipos de evento
invites-intro = Envie links de convite para { $title }
invites-capped = <strong>A entrada foi limitada a { $max } destinatários por envio.</strong> Envie o restante em outra leva.
invites-failed-hint = — consulte os logs do servidor para mais detalhes.
invites-quick-link = Link rápido
invites-quick-link-help = Gere um link de uso único e copie-o para a área de transferência.
invites-get-link = Obter link
invites-or-email = Ou enviar por e-mail
invites-recipients = Destinatários
invites-recipients-hint = (um e-mail por linha, no máximo { $max })
invites-message = Mensagem pessoal
invites-message-hint = (opcional, enviada a todos os destinatários)
invites-message-placeholder = Estou ansioso para te mostrar uma demonstração...
invites-expires-in = Expira em
invites-expires-days = { $days } dias
invites-expires-never = Nunca
invites-allow-multiple = Permitir vários agendamentos por destinatário
invites-send = Enviar os convites
invites-sent-heading = Convites enviados
invites-badge-expired = expirado
invites-badge-used = usado
invites-badge-active = ativo
invites-sent-by = Enviado por { $name }
invites-uses = { $used }/{ $max } usos
invites-expires-at = Expira em { $date }
invites-copy-link = Copiar o link
invites-delete = Excluir
invites-delete-confirm = Excluir este convite?
invites-empty = Ainda não há convites enviados. Use o formulário acima para mandar a alguém um link de agendamento.
invites-js-generating = Gerando...
invites-js-copied = Copiado!
invites-js-error = Erro

invites-sent-count =
    { $count ->
        [one] { $count } convite enviado.
       *[other] { $count } convites enviados.
    }

invites-skipped-invalid =
    { $count ->
        [one] { $count } linha inválida ignorada:
       *[other] { $count } linhas inválidas ignoradas:
    }

invites-skipped-duplicate =
    { $count ->
        [one] { $count } linha duplicada ignorada:
       *[other] { $count } linhas duplicadas ignoradas:
    }

invites-failed =
    { $count ->
        [one] { $count } convite falhou (BD ou SMTP):
       *[other] { $count } convites falharam (BD ou SMTP):
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Editar a fonte de agenda
source-form-title-add = Adicionar uma agenda
source-form-heading-edit = Editar a fonte de agenda
source-form-heading-add = Conectar uma agenda
source-form-subtitle-edit = Atualize a conexão. Deixe a senha em branco para manter a atual. Depois de alterar a URL ou o usuário, execute uma sincronização para atualizar a lista de agendas detectadas.
source-form-subtitle-add = Conecte um servidor CalDAV ou o Microsoft Exchange (EWS) para que o calrs possa verificar a disponibilidade quando alguém agendar.
source-form-backend = Backend
source-form-preset = Predefinição
source-form-connect-google = Conectar com o Google
source-form-google-unavailable = O Google Agenda não está disponível. Fale com a administração.
source-form-name = Nome de exibição
source-form-name-placeholder = Minha agenda
source-form-url-caldav = URL do CalDAV
source-form-url-ews = URL do endpoint EWS
source-form-username = Usuário
source-form-password = Senha
source-form-password-keep = Deixe em branco para manter a atual
source-form-password-placeholder = Senha de aplicativo ou senha da conta
source-form-skip-test = Pular o teste de conexão
source-form-skip-test-help = Use isto se o teste travar (acontece em algumas instalações do BlueMind ou do Zimbra). Você pode testar a conexão depois.
source-form-save = Salvar as alterações
source-form-add = Adicionar a fonte de agenda
source-form-help-google-configured = Clique no botão abaixo para autorizar o calrs a acessar seu Google Agenda.
source-form-help-google-unconfigured = A integração com o Google Agenda ainda não foi configurada. Peça à administração que configure as credenciais OAuth2 do Google no painel de administração.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — Use o endpoint DAV do seu servidor BlueMind.<br> Normalmente: <code>https://mail.yourcompany.com/dav/</code><br> O usuário é o seu <strong>endereço de e-mail</strong> (ex.: <code>alice@yourcompany.com</code>), não apenas o nome de login.<br> Se o teste de conexão travar, marque «Pular o teste de conexão» e sincronize direto.
source-form-help-nextcloud = <strong>Nextcloud</strong> — Use a raiz WebDAV, não a URL de uma agenda específica.<br> Normalmente: <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — Use seu endereço completo no caminho da URL.<br> Exemplo: <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Use uma senha de aplicativo (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — Use <code>https://caldav.icloud.com/</code><br> Você precisa de uma senha de aplicativo de <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Segurança &rarr; Senhas de aplicativo).
source-form-help-zimbra = <strong>Zimbra</strong> — Use o endpoint DAV do seu servidor Zimbra.<br> Normalmente: <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — Use o endpoint DAV do SOGo.<br> Normalmente: <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — Use a URL raiz do servidor.<br> Normalmente: <code>https://cal.example.com/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong>. Use o endpoint SOAP:<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> O usuário é o endereço da caixa postal; a senha precisa aceitar HTTP Basic sobre TLS (ative numa caixa de serviço se o seu tenant desativou o Basic).<br> Escolha também <strong>Microsoft Exchange (EWS)</strong> no menu Backend acima.
source-form-help-google = <strong>Google Agenda</strong>: conexão via OAuth2. Não é preciso senha.<br>
source-form-help-other = Informe a <strong>URL raiz DAV</strong> do seu servidor CalDAV, não a de uma agenda específica nem um link público.<br> O calrs descobrirá suas agendas automaticamente via PROPFIND (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Negrito
editor-italic-short = Itálico
editor-link-short = Inserir link

# Team creation (templates/team_form.html)

team-form-heading = Nova equipe
team-form-name = Nome da equipe
team-form-name-placeholder = Engenharia
team-form-slug = Identificador
team-form-slug-hint = (identificador compatível com URL)
team-form-slug-pattern-title = Apenas letras minúsculas, números e hifens
team-form-description = Descrição
team-form-optional = (opcional)
team-form-description-placeholder = Do que esta equipe cuida...
team-form-description-help = Aparece na página da equipe. Aceita **negrito**, *itálico* e [links](url).
team-form-visibility = Visibilidade
team-form-public = Pública
team-form-private = Privada
team-form-visibility-help = Equipes privadas recebem um token de convite para compartilhar. As públicas aparecem na página de perfil da equipe.
team-form-members = Membros
team-form-members-help = Você será adicionado automaticamente como administrador da equipe. Adicione usuários específicos ou vincule grupos OIDC.
team-form-search-placeholder = Buscar usuários ou grupos...
team-form-search-users = Usuários
team-form-search-groups = Grupos OIDC
team-form-you = (você)
team-form-submit = Criar a equipe

# Team settings (templates/team_settings.html)

team-settings-page-title = Configurações
team-settings-subtitle = Configurações da equipe: os administradores da equipe podem editá-las.
team-settings-public-url = URL pública
team-settings-public-url-help = Qualquer pessoa pode agendar por este link.
team-settings-invite-link = Link de convite
team-settings-invite-link-help = Compartilhe este link para dar acesso à página de agendamento desta equipe privada.
team-settings-avatar = Avatar da equipe
team-settings-profile = Perfil
team-settings-description-placeholder = Fale sobre esta equipe...
team-settings-description-help = Aparece na página de agendamento pública da equipe. Aceita **negrito**, *itálico* e [links](url).
team-settings-visibility-help = Equipes públicas aparecem na página de perfil da equipe. As privadas exigem um link de convite.
team-settings-members-help = Gerencie quem faz parte desta equipe. Adicione usuários específicos ou vincule grupos OIDC para sincronização automática.
team-settings-role-member = Membro
team-settings-role-admin = Administrador
team-settings-oidc-group = Grupo OIDC
team-settings-remove = Remover
team-settings-save = Salvar as alterações
team-settings-danger-zone = Zona de perigo
team-settings-danger-help = Excluir esta equipe permanentemente. Os tipos de evento serão desvinculados, não excluídos. Esta ação não pode ser desfeita.
team-settings-delete = Excluir esta equipe
team-settings-delete-confirm = Excluir a equipe «{ $name }»? Esta ação não pode ser desfeita.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Editar o tipo de evento
etf-heading-new = Novo tipo de evento
etf-team = Equipe
etf-team-hint = (opcional: deixe em branco para um tipo de evento pessoal)
etf-team-personal = Pessoal
etf-scheduling-mode = Modo de atribuição
etf-mode-round-robin = Rodízio: atribuir a um membro disponível
etf-mode-collective = Coletivo: todos os membros precisam estar disponíveis
etf-scheduling-mode-help = «Rodízio» atribui o agendamento a um membro disponível (primeiro o menos ocupado). «Coletivo» exige que todos os membros estejam livres ao mesmo tempo.
etf-title = Título
etf-title-placeholder = Conversa inicial de 30 min
etf-slug = Identificador
etf-slug-placeholder = gerado a partir do título
etf-description-placeholder = Uma conversa rápida de apresentação para falar sobre...
etf-description-help = Aparece na página de agendamento. Aceita **negrito**, *itálico* e [links](url).
etf-location = Local
etf-location-link = Videochamada (URL fixa)
etf-location-jitsi = Jitsi (sala gerada automaticamente)
etf-location-webhook = Webhook (provedor próprio)
etf-location-phone = Telefone
etf-location-in-person = Presencial
etf-location-custom = Personalizado
etf-location-details = Detalhes
etf-location-details-placeholder = https://meet.example.com/minha-sala
etf-pattern-placeholder = Deixe em branco para usar o padrão da organização
etf-duration = Duração (minutos)
etf-slot-interval = Intervalo entre horários (minutos)
etf-slot-interval-placeholder = Igual à duração
etf-slot-interval-help = De quanto em quanto tempo os horários começam. Deixe em branco para acompanhar a duração.
etf-required-members = Membros necessários
etf-required-members-help = Todos os membros marcados precisam estar livres para que um horário seja oferecido. Desmarque quem quiser excluir (a disponibilidade dessa pessoa será ignorada).
etf-member-priority = Prioridade dos membros
etf-member-priority-help = Membros com prioridade mais alta recebem os agendamentos primeiro, quando disponíveis. Com a mesma prioridade, o equilíbrio segue os agendamentos recentes.
etf-member-timezone-title = Fuso horário do membro. O horário de trabalho pessoal dele é interpretado neste fuso.
etf-priority-high = Alta
etf-priority-medium = Média
etf-priority-low = Baixa
etf-section-availability = Disponibilidade
etf-timezone-help = Os horários abaixo são interpretados neste fuso. Em tipos de evento de equipe, escolha o fuso de trabalho da equipe (não necessariamente o de quem criou).
etf-reset-default = Restaurar meus padrões
etf-reset-default-title = Substituir estes horários pela disponibilidade padrão do seu perfil
etf-availability-prefilled = Preenchido a partir da sua { $link }. Você pode alterá-lo aqui para este tipo de evento.
etf-availability-prefilled-link = disponibilidade padrão
etf-section-buffers = Intervalos e antecedência
etf-buffer-before = Intervalo antes (min)
etf-buffer-after = Intervalo depois (min)
etf-min-notice = Antecedência mínima
etf-min-notice-help = Com quanta antecedência é preciso agendar.
etf-section-limits = Limites de agendamento
etf-first-slot-only = Um horário por dia
etf-first-slot-only-help = Mostrar apenas o primeiro horário disponível de cada dia.
etf-freq-limit = Limitar a frequência de agendamentos
etf-freq-limit-help = Limitar quantas vezes este evento pode ser agendado por período.
etf-add-limit = Adicionar um limite
etf-section-options = Opções de agendamento
etf-requires-confirmation = Exige confirmação
etf-requires-confirmation-help = Os agendamentos ficarão pendentes até você aprová-los pelo painel.
etf-sms = Notificações por SMS
etf-sms-off = Desativadas, sem pedir telefone
etf-sms-optional = Opcional, os convidados podem deixar um número
etf-sms-required = Obrigatório, os convidados precisam deixar um número
etf-sms-help = Envia um SMS ao convidado, além do e-mail, quando o agendamento é confirmado, remarcado, cancelado ou está prestes a começar. Quem deixar o campo em branco simplesmente não recebe SMS. Requer um gateway de SMS no { $link }.
etf-admin-panel-link = painel de administração
etf-additional-guests = Convidados adicionais
etf-guests-none = Os convidados não podem adicionar outras pessoas
etf-additional-guests-help = Permitir que quem agenda convide outros participantes, que receberão o convite da agenda.
etf-default-view = Visualização padrão da agenda
etf-view-month = Mês: grade de calendário com lista de horários
etf-view-week = Semana: colunas de 7 dias com os horários
etf-view-column = Coluna: dias em lista com os horários
etf-view-week-short = semanal
etf-view-column-short = de coluna
etf-default-view-help = A visualização que os convidados veem primeiro. Eles podem trocar quando quiserem.
etf-conflict-calendars = Agendas para conflitos
etf-conflict-calendars-help = Escolha quais agendas verificar em busca de conflitos. Se nenhuma for escolhida, todas são usadas.
etf-no-resources = Ainda não há recursos compartilhados configurados. Adicione um (laboratório de demonstração, sala de reunião) no { $link } para exigi-lo aqui.
etf-section-access = Acesso e notificações
etf-visibility-public = Público: visível no seu perfil
etf-visibility-internal = Interno: qualquer colega pode gerar links de convite
etf-visibility-private = Privado: apenas por link de convite
etf-visibility-help = Define quem pode ver e agendar este tipo de evento.
etf-vis-internal = Interno
etf-reminder = Lembrete do agendamento
etf-reminder-none = Sem lembrete
etf-reminder-help = Enviar um e-mail de lembrete para você e para o convidado antes da reunião.
etf-dynamic-group = Link de grupo dinâmico
etf-dynamic-group-help = Crie um link de reunião improvisado que verifica a disponibilidade sua e de outros usuários.
etf-dynamic-group-search = Busque um usuário para adicionar...
etf-dynamic-group-note = Só aparecem os usuários que permitem links de grupo dinâmicos.
etf-dynamic-group-url = URL do link de grupo
etf-watcher-teams = Equipes observadoras
etf-watcher-teams-help = As equipes selecionadas serão avisadas a cada agendamento. Seus membros podem assumir um agendamento para participar dele.
etf-save = Salvar as alterações
etf-create = Criar o tipo de evento
etf-js-loading = Carregando...
etf-js-no-default = Nenhum padrão definido
etf-js-reset-done = Restaurado!
etf-js-error = Erro
etf-js-remove-limit = Remover o limite
etf-period-day = Por dia
etf-period-week = Por semana
etf-period-month = Por mês
etf-period-year = Por ano

# Event type form: runtime summary hints (templates/event_type_form.html)


# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = Nenhum dia definido
etf-hint-every-day = Todos os dias
etf-fmt-day-one = %1 dia
etf-fmt-day-other = %1 dias
etf-fmt-hours = %1 h
etf-fmt-minutes = %1 min
etf-hint-buffer-both = %1 min antes, %2 min depois
etf-hint-buffer-before = %1 min de intervalo antes
etf-hint-buffer-after = %1 min de intervalo depois
etf-hint-notice = %1 de antecedência
etf-hint-no-buffers = Sem intervalos, pode agendar a qualquer hora
etf-hint-max = Máx. %1
etf-hint-period-day = /dia
etf-hint-period-week = /semana
etf-hint-period-month = /mês
etf-hint-period-year = /ano
etf-hint-no-limits = Sem limites
etf-hint-confirmation-required = Exige confirmação
etf-hint-auto-confirmed = Confirmação automática
etf-hint-extra-guests-one = até %1 convidado a mais
etf-hint-extra-guests-other = até %1 convidados a mais
etf-hint-view = visualização %1
etf-hint-reminder = lembrete %1 antes
etf-hint-no-reminder = sem lembrete

etf-guests-up-to =
    { $count ->
        [one] Até { $count } convidado adicional
       *[other] Até { $count } convidados adicionais
    }

etf-reminder-hours =
    { $count ->
        [one] { $count } hora antes
       *[other] { $count } horas antes
    }

etf-reminder-days =
    { $count ->
        [one] { $count } dia antes
       *[other] { $count } dias antes
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Você está criando um tipo de evento <strong>público</strong> &mdash; qualquer pessoa com o link pode agendar.
etf-preset-private = Você está criando um tipo de evento <strong>privado</strong> &mdash; só quem você convidar pode agendar.
etf-preset-internal = Você está criando um tipo de evento <strong>interno</strong> &mdash; qualquer colega pode compartilhar o link de agendamento.
etf-preset-team = Você está criando um tipo de evento <strong>de equipe</strong> &mdash; os agendamentos são distribuídos entre os membros da equipe.
etf-pattern-hint = Padrão próprio opcional. Marcadores: <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Deixe em branco para usar o padrão da organização definido pela administração.
etf-pattern-random-warning = Este padrão não tem o marcador <code>{"{"}random{"}"}</code>. Dois agendamentos deste tipo de evento no mesmo dia vão compartilhar a mesma sala, e o segundo convidado pode entrar na reunião do primeiro. Use salas fixas só se for exatamente isso que você quer.
etf-webhook-hint = A URL de reunião de cada agendamento vem do webhook que a administração configurou em Administração &rarr; Webhook de reunião. Aqui não é preciso nenhuma URL.

# Admin panel (templates/admin.html)

admin-page-title = Administração
admin-heading = Painel de administração
admin-action-refused = Ação recusada:
admin-logo = Logotipo da empresa
admin-logo-help = Aparece nas páginas de agendamento públicas. Recomendado: PNG ou SVG, no máximo 2 MB.
admin-company-link = Link da empresa
admin-company-link-help = Nas páginas de agendamento públicas, o logotipo aponta para esta URL. Deixe em branco para não criar link.
admin-theme = Tema
admin-theme-help = Escolha um tema de cores para todas as páginas. A alternância entre claro e escuro é independente: os temas se adaptam aos dois modos.
admin-theme-default = Padrão
admin-theme-default-desc = Azul limpo
admin-theme-nord-desc = Geada ártica
admin-theme-dracula-desc = Roxo escuro
admin-theme-gruvbox-desc = Retrô quente
admin-theme-solarized-desc = O clássico do Ethan
admin-theme-tokyo-desc = Cidade em neon
admin-theme-custom = Personalizado
admin-theme-custom-desc = Suas cores
admin-custom-colors = Cores personalizadas
admin-color-accent = Cor de destaque
admin-color-accent-hover = Destaque ao passar o cursor
admin-color-bg = Fundo
admin-color-surface = Superfície
admin-color-text = Texto
admin-save-theme = Salvar o tema
admin-users = Usuários ({ $count })
admin-user-filter = Filtrar por nome ou e-mail…
admin-badge-admin = administrador
admin-badge-disabled = desativado
admin-impersonate = Personificar
admin-demote = Rebaixar
admin-promote = Promover
admin-disable = Desativar
admin-enable = Ativar
admin-delete = Excluir
admin-no-users-match = Nenhum usuário corresponde ao filtro.
admin-no-users = Ainda não há usuários.
admin-groups = Grupos ({ $count })
admin-group-filter = Filtrar por nome do grupo…
admin-group-name = Nome do grupo
admin-weight = peso:
admin-no-groups-match = Nenhum grupo corresponde ao filtro.
admin-no-groups = Ainda não há grupos sincronizados. Os grupos são sincronizados automaticamente a partir do seu provedor OIDC.
admin-auth-settings = Configurações de acesso
admin-registration-enabled = Cadastro ativado
admin-allowed-domains = Domínios de e-mail permitidos
admin-allowed-domains-hint = (separados por vírgulas, em branco para permitir todos)
admin-save-auth = Salvar as configurações de acesso
admin-system-settings = Configurações do sistema
admin-base-url = URL base
admin-base-url-help = URL pública desta instância. Usada nos redirecionamentos OIDC e nos links dos e-mails (aprovar/recusar, cancelar, lembretes).
admin-private-hosts = Lista de hosts privados permitidos
admin-private-hosts-help = Nomes de host, separados por vírgulas, que podem resolver para IPs privados ou reservados nas fontes CalDAV/EWS (exceção à proteção contra SSRF). Adicione apenas hosts que você controla (por exemplo, um servidor de agenda na mesma rede Docker). Deixe em branco para manter a proteção ativa em todos os hosts.
admin-unset-env = Remova a variável de ambiente para poder editar isto aqui.
admin-save-system = Salvar as configurações do sistema
admin-status = Status:
admin-status-enabled = ativado
admin-status-disabled = desativado
admin-status-disabled-paren = (desativado)
admin-status-configured = configurado
admin-status-not-configured = não configurado
admin-via-environment = (pelo ambiente)
admin-issuer = Emissor:
admin-client-id = ID do cliente:
admin-instance = Instância:
admin-oidc-settings = Configurações de OIDC
admin-oidc-enabled = OIDC ativado
admin-issuer-url = URL do emissor
admin-client-id-label = ID do cliente
admin-client-secret = Segredo do cliente
admin-keep-current-hint = (deixe em branco para manter o atual)
admin-keep-current-set-hint = (deixe em branco para manter o atual: já há um definido)
admin-keep-unchanged = Deixe em branco para não alterar
admin-oidc-auto-register = Cadastrar automaticamente os novos usuários vindos do OIDC
admin-save-oidc = Salvar as configurações de OIDC
admin-google = Google Agenda (OAuth2)
admin-save-google = Salvar as configurações OAuth2 do Google
admin-captcha = Captcha
admin-instance-url = URL da instância
admin-site-key = Chave do site
admin-secret = Segredo
admin-widget-url = URL do script do widget
admin-widget-url-help = Altere se o CDN estiver bloqueado. As mudanças valem logo após salvar.
admin-captcha-disable-help = Deixe em branco a URL da instância, a chave do site e o segredo para desativar o captcha nas páginas de agendamento.
admin-save-captcha = Salvar as configurações do captcha
admin-resources = Recursos
admin-resources-help = Recursos compartilhados agendáveis (laboratório de demonstração, salas de reunião) baseados num feed de agenda. Vinculados a tipos de evento, um recurso ocupado bloqueia os agendamentos.
admin-resource-stats = Eventos em cache: { $events } &middot; Vinculado a { $attached } tipo(s) de evento
admin-never = nunca
admin-resource-sync-failed = (a última tentativa falhou: { $error })
admin-writeback-enabled = Gravação: ativada ({ $via })
admin-writeback-readonly = Gravação: somente leitura
admin-teams-allowed = Equipes permitidas:
admin-teams-allowed-none = nenhuma (apenas administradores globais)
admin-sync-now = Sincronizar agora
admin-test-write = Testar a gravação
admin-delete-resource-confirm = Excluir este recurso? Os tipos de evento que o usam deixarão de verificá-lo.
admin-name = Nome
admin-name-help = Deixe em branco para pegar o nome do feed.
admin-feed-url = URL do feed ICS (endereço de publicação)
admin-feed-url-help = BlueMind: o endereço de agenda público ou privado da agenda do recurso.
admin-caldav-url = URL da coleção CalDAV (para gravação)
admin-caldav-url-help = Opcional. No BlueMind é deduzida automaticamente da URL do feed.
admin-caldav-username = Usuário do CalDAV
admin-caldav-password = Senha do CalDAV
admin-resource-teams = Equipes autorizadas a usar este recurso
admin-resource-teams-help = Os administradores dessas equipes podem vincular o recurso aos tipos de evento da equipe. Em branco: apenas administradores globais.
admin-no-teams = Ainda não há equipes.
admin-save-resource = Salvar o recurso
admin-add-resource = Adicionar um recurso
admin-jitsi = Jitsi (links de reunião gerados automaticamente)
admin-jitsi-help = Quando o local de um tipo de evento é «Jitsi (sala gerada automaticamente)», o calrs monta uma URL de sala nova para cada agendamento acrescentando o padrão abaixo à sua URL base do Jitsi. Nenhuma chamada a API externa é necessária.
admin-display-name = Nome de exibição
admin-jitsi-display-name-placeholder = ex.: Meet DYB
admin-jitsi-display-name-help = Aparece para os convidados no seletor de horários e no formulário de agendamento. Se ficar em branco, usa-se «Videochamada».
admin-room-pattern = Padrão do nome da sala
admin-jitsi-disable-help = Deixe a URL base em branco para desativar a geração automática do Jitsi.
admin-save-jitsi = Salvar as configurações do Jitsi
admin-meeting-webhook = Webhook de reunião (provedor próprio)
admin-webhook-url = URL do webhook
admin-webhook-display-name-placeholder = ex.: Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Aparece para os convidados no lugar do rótulo genérico «Videochamada».
admin-authentication = Autenticação
admin-auth-none = Nenhuma
admin-auth-hmac = HMAC-SHA256 (cabeçalho X-Calrs-Signature)
admin-shared-secret = Segredo compartilhado
admin-webhook-disable-help = Deixe a URL em branco para desativar o webhook de reunião.
admin-save-webhook = Salvar as configurações do webhook
admin-smtp = Configurações de SMTP
admin-smtp-test-sent = E-mail de teste enviado.
admin-smtp-test-failed = Não foi possível enviar o e-mail de teste. Verifique os logs do servidor e suas configurações de SMTP.
admin-smtp-env-error = Erro na configuração de SMTP vinda do ambiente:
admin-smtp-host = Host:
admin-smtp-from = Remetente:
admin-smtp-enabled = SMTP ativado
admin-host = Host
admin-port = Porta
admin-tls-mode = Modo TLS
admin-tls-starttls = STARTTLS (porta 587)
admin-tls-implicit = TLS implícito (porta 465)
admin-tls-none = Nenhum, sem criptografia (apenas MTA local)
admin-smtp-username-hint = (deixe em branco para um relay sem autenticação)
admin-from-email = E-mail do remetente
admin-from-name = Nome do remetente
admin-save-smtp = Salvar as configurações de SMTP
admin-send-test-email = Enviar um e-mail de teste para
admin-send-test-email-hint = (por padrão, o e-mail da sua conta)
admin-send-test-email-btn = Enviar o e-mail de teste
admin-smtp-clear-confirm = Excluir a configuração de SMTP salva no banco de dados?
admin-clear-db-config = Limpar a configuração do banco de dados
admin-sms = Configurações de SMS
admin-sms-help = Opcional. Os SMS só são enviados em agendamentos de tipos de evento com as «Notificações por SMS» ativadas, e apenas quando o convidado deixou um número de telefone.
admin-sms-test-sent = Mensagem de teste enviada.
admin-sms-test-checked = Credenciais aceitas.
admin-sms-test-error = O gateway de SMS recusou a requisição.
admin-sms-captcha-warning = O formulário de agendamento é público e o número do destinatário vem do convidado, então SMS sem captcha é um relay aberto que outra pessoa pode cobrar de você. Configure o captcha acima e restrinja os países de destino nas configurações do seu gateway.
admin-sms-sent-today = Enviados hoje:
admin-sms-of-cap = de { $cap }
admin-sms-config-error = Erro na configuração de SMS:
admin-sms-gateway = Gateway:
admin-sms-account = Conta:
admin-sms-sender = Remetente:
admin-sms-enabled = SMS ativados
admin-sms-gateway-label = Gateway
admin-required-on-switch = Obrigatório ao trocar de gateway
admin-sms-docs = Documentação da API do { $provider }
admin-sms-country = Código de país padrão
admin-sms-country-hint = (usado quando os convidados informam um número local)
admin-sms-daily-cap = Limite diário
admin-sms-daily-cap-hint = (mensagens por dia para toda a instância, 0 para não limitar)
admin-sms-daily-cap-help = Passado o limite, o calrs para de enviar SMS e continua enviando e-mails, para que nenhum agendamento falhe porque o orçamento de SMS acabou.
admin-save-sms = Salvar as configurações de SMS
admin-send-test-sms = Enviar uma mensagem de teste para
admin-send-test-sms-hint-check = (deixe em branco para apenas verificar as credenciais)
admin-send-test-sms-hint-e164 = (formato E.164)
admin-test-gateway = Testar o gateway
admin-sms-clear-confirm = Excluir a configuração de SMS salva no banco de dados?
admin-sms-allow-all = Permitir que qualquer usuário ative os SMS nos seus tipos de evento
admin-sms-allow-all-help = Desativado por padrão: os SMS gastam crédito da conta configurada aqui, então apenas administradores podem colocar um tipo de evento em modo SMS.
admin-save-policy = Salvar a política
admin-page-of = Página %1 de %2
admin-show-more-js = Mostrar mais %1
admin-show-fewer = Mostrar menos

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Excluir permanentemente o usuário { $email }?{"\u000A"}{"\u000A"}Isso remove a conta dele, o perfil de agendamento, as fontes de agenda, os tipos de evento e todos os dados que pertencem exclusivamente a ele. Agendamentos passados serão excluídos junto com os tipos de evento.{"\u000A"}{"\u000A"}Para usuários de OIDC/SSO: se o cadastro automático estiver ativado, esta pessoa será recriada no próximo login.{"\u000A"}{"\u000A"}Esta ação não pode ser desfeita.
admin-system-settings-help = URL pública e configurações de segurança de rede. Também podem ser definidas pelas variáveis de ambiente <code>CALRS_BASE_URL</code> e <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Quando uma variável de ambiente está definida, ela <strong>tem prioridade</strong> sobre o valor abaixo.
admin-set-by-env = — definido pelo ambiente ({ $var }), tem prioridade sobre o valor salvo
admin-google-help = Para ativar a integração com o Google Agenda, crie credenciais OAuth2 no <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Ative a <strong>Google Calendar API</strong> e depois adicione { $redirect_uri } como URI de redirecionamento autorizado.
admin-room-pattern-help = Marcadores disponíveis: <code>{"{"}username{"}"}</code> (organizador), <code>{"{"}event{"}"}</code> (identificador do tipo de evento), <code>{"{"}date{"}"}</code> (AAAAMMDD), <code>{"{"}random{"}"}</code> (8 caracteres). Padrão: { $default }.
admin-room-pattern-warning = Sem <code>{"{"}random{"}"}</code> o nome da sala é previsível: dois convidados que agendem o mesmo tipo de evento no mesmo dia acabam na mesma sala e podem ver a reunião um do outro. Salas fixas são permitidas (por exemplo, uma sala pessoal por organizador), mas ative isso só se entender a contrapartida.
admin-meeting-webhook-help = Quando o local de um tipo de evento é «Webhook (provedor próprio)», o calrs envia os dados do agendamento por POST para esta URL na confirmação e espera de volta um corpo JSON <code>{"{"}"url": "https://..."{"}"}</code>.
admin-auth-hmac-help = Com HMAC, o calrs envia <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> calculado sobre o corpo bruto da requisição.
admin-tls-none-warning = Escolha <strong>Nenhum</strong> apenas para um relay nesta máquina que não ofereça STARTTLS, ou cujo certificado seja autoassinado. As mensagens, e quaisquer credenciais, trafegam sem criptografia.
admin-smtp-env-error-help = Corrija as variáveis de ambiente <code>CALRS_SMTP_*</code>, ou remova-as para gerenciar o SMTP pelo banco de dados aqui.
admin-smtp-env-managed = Gerenciado por <strong>variáveis de ambiente</strong> (têm prioridade sobre o banco de dados). Altere as variáveis <code>CALRS_SMTP_*</code>, ou remova-as para gerenciar o SMTP daqui.
admin-smtp-env-help = Como alternativa, configure por variáveis de ambiente (que têm prioridade sobre isto): <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> ou <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Só <code>CALRS_SMTP_HOST</code> e <code>CALRS_SMTP_FROM_EMAIL</code> são obrigatórias; omita usuário e senha para retransmitir por um MTA local sem autenticação.
admin-sms-env-error-help = Corrija as variáveis de ambiente <code>CALRS_SMS_*</code>, ou remova-as para gerenciar os SMS pelo banco de dados aqui.
admin-sms-env-managed = Gerenciado por <strong>variáveis de ambiente</strong> (têm prioridade sobre o banco de dados). Altere as variáveis <code>CALRS_SMS_*</code>, ou remova-as para gerenciar os SMS daqui.
admin-sms-env-help = Como alternativa, configure por variáveis de ambiente (que têm prioridade sobre isto): <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>O modo de teste da Twilio está ativado</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Os convidados recebem o modelo predefinido da Twilio <code>sms_appointment_reminders</code> em vez da mensagem real, e apenas números verificados no seu console da Twilio são alcançados. É um recurso de teste para contas de avaliação. Remova a variável antes de aceitar agendamentos.

admin-show-more =
    { $count ->
        [one] Mostrar mais { $count }
       *[other] Mostrar mais { $count }
    }

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Escolha o protocolo que o seu servidor fala. O EWS é voltado a instalações locais do Exchange 2019/2016/2013.

admin-sms-going-live = <strong>Antes de entrar em produção:</strong> restrinja os países de destino no seu gateway (na Twilio isso se chama Geo Permissions), mantenha a conta pré-paga sem recarga automática, e deixe o captcha ativado. Juntas, essas três medidas limitam quanto uma tentativa de SMS pumping pode custar a você.

troubleshoot-heading = Diagnóstico de disponibilidade

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = O nome e o identificador são obrigatórios.
form-error-team-name-length = O nome não pode passar de 255 caracteres.
form-error-team-description-length = A descrição não pode passar de 5000 caracteres.
form-error-slug-charset = O identificador só pode conter letras minúsculas, números e hifens.
form-error-slug-reserved = Este identificador é reservado. Por favor, escolha outro.
form-error-team-slug-taken = Já existe uma equipe com este identificador.
form-error-title-required = É preciso um título para gerar o identificador.
form-error-event-type-slug-taken = Já existe um tipo de evento com este identificador.
form-error-event-type-slug-taken-team = Já existe um tipo de evento com este identificador nesta equipe.
form-error-location-required = Os detalhes do local são obrigatórios (por exemplo, um link de videochamada, um telefone ou um endereço).
form-error-not-team-admin = Você não é administrador desta equipe.
form-error-no-account = Nenhum perfil de agendamento encontrado. Por favor, fale com a administração.
form-error-all-fields-required = Todos os campos são obrigatórios.
form-error-encryption = Erro de criptografia.
form-error-connection-failed = A conexão falhou: { $error }. Verifique a URL e as credenciais, ou marque «Pular o teste de conexão» para salvar mesmo assim.

# Settings page flash (src/web/mod.rs)

settings-saved = Configurações salvas.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = O nome deve ter entre 1 e 255 caracteres.
settings-error-username-length = O nome de usuário deve ter pelo menos 2 caracteres.
settings-error-username-taken = Este nome de usuário já está em uso.
settings-error-booking-email = Por favor, informe um endereço de e-mail de agendamento válido.
settings-error-save-failed = Não foi possível salvar as configurações.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = Equipe não encontrada, ou você não é administrador dela.
error-team-not-found = Equipe não encontrada.
error-event-type-not-found = Tipo de evento não encontrado.
error-decrypt-failed = Não foi possível descriptografar as credenciais salvas.
error-source-not-found = Fonte não encontrada.
error-source-no-password = Esta fonte não tem senha salva.
error-oauth-invalid-state = Parâmetro de estado inválido. Por favor, tente novamente.
error-oauth-no-code = Nenhum código de autorização recebido.
error-oauth-not-configured = O Google OAuth2 não está configurado.
error-no-scheduling-account = Nenhum perfil de agendamento encontrado.
error-private-event-type-not-found = Tipo de evento privado não encontrado.
error-access-denied = Acesso negado.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = Este horário não está mais disponível.
error-slot-too-soon = Este horário não está mais disponível (muito em cima da hora).
error-slot-beyond-horizon = Este horário está além da janela de agendamento.
error-invite-required = Este tipo de evento exige um link de convite.
error-invite-invalid = Link de convite inválido.
error-invite-expired = Este link de convite expirou.
error-invite-used = Este link de convite já foi usado.
error-invalid-date = Data inválida.
error-invalid-time = Horário inválido.
error-invalid-date-format = Formato de data inválido.
error-invalid-time-format = Formato de horário inválido.
error-too-many-bookings = Muitas tentativas de agendamento. Por favor, tente novamente em alguns minutos.
error-too-many-requests = Muitas requisições. Por favor, tente novamente mais tarde.
error-no-members-available = Nenhum membro da equipe está disponível para este horário.
error-dynamic-group-public-only = Links de grupo dinâmicos só estão disponíveis para tipos de evento públicos.
error-user-not-found = Usuário não encontrado.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Falha na verificação do captcha
bae-title-invalid-booking = Dados do agendamento inválidos
bae-title-unavailable = Indisponível no momento
bae-title-cannot-approve = Não é possível aprovar este agendamento
bae-title-invalid-link = Link inválido
bae-title-invalid-or-expired = Link inválido ou expirado
bae-title-booking-not-found = Agendamento não encontrado
bae-title-already-approved = Já aprovado
bae-title-already-declined = Já recusado
bae-title-already-cancelled = Já cancelado
bae-title-booking-cancelled = Agendamento cancelado
bae-title-booking-declined = Agendamento recusado

# Booking action error page: bodies

bae-body-go-back = Por favor, volte e tente novamente.
bae-body-unavailable = O organizador não está aceitando mais agendamentos para esta data. Por favor, escolha outra data ou volte mais tarde.
bae-body-resource-gone = Um recurso necessário não está mais disponível neste horário. Peça ao convidado para escolher outro horário.
bae-body-no-claim-token = Nenhum token informado.
bae-body-claim-invalid = Este link não é mais válido.
bae-body-booking-gone = Este agendamento não existe mais.
bae-body-decline-link-invalid = Este link de recusa é inválido, expirou, ou o agendamento já foi processado.
bae-body-cancel-link-invalid = Este link de cancelamento é inválido, expirou, ou o agendamento já foi cancelado.
bae-body-cancel-link-invalid-short = Este link de cancelamento é inválido ou expirou.
bae-body-reschedule-link-invalid = Este link de remarcação é inválido, expirou, ou o agendamento já foi processado.
bae-body-approval-link-invalid = Este link de aprovação é inválido ou expirou.
bae-body-already-approved = Este agendamento já foi aprovado.
bae-body-already-declined = Este agendamento já foi recusado.
bae-body-already-cancelled = Este agendamento já foi cancelado.
bae-body-was-cancelled = Este agendamento foi cancelado.
bae-body-declined-by-host = Este agendamento foi recusado pelo organizador.

# Booking form validation (src/web/mod.rs)

validate-name-length = O nome deve ter entre 1 e 255 caracteres.
validate-email-length = O e-mail deve ter entre 1 e 255 caracteres.
validate-email-invalid = Por favor, informe um endereço de e-mail válido.
validate-notes-length = As observações não podem passar de 5000 caracteres.
validate-date-too-far = Não é possível agendar com mais de um ano de antecedência.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = Este tipo de evento não aceita convidados adicionais.
guests-too-many =
    { $max ->
        [one] Você pode adicionar no máximo um convidado adicional.
       *[other] Você pode adicionar no máximo { $max } convidados adicionais.
    }
guests-invalid-email = E-mail de convidado adicional inválido: { $email }
dynamic-group-min-usernames = Os links de grupo dinâmico exigem pelo menos dois nomes de usuário.
dynamic-group-user-not-found = Usuário “{ $username }” não encontrado.
dynamic-group-user-opted-out = O usuário “{ $username }” não ativou os links de grupo dinâmico.

error-slot-unavailable-member = Este horário não está mais disponível ({ $username } tem um conflito).
