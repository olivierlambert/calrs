# OAuth connection consent
oauth-title = Подключение приложения
oauth-intro = Разрешить этому приложению доступ к calrs от имени { $email }? Название приложения указывает его разработчик. Прежде чем разрешить доступ, проверьте адрес перенаправления.
oauth-events = Просматривать типы встреч, к которым у вас есть доступ
oauth-blocks = Просматривать занятое время в ваших календарях
oauth-invites = Создавать ссылки-приглашения на запись с вашими правами доступа
oauth-allow = Разрешить доступ
oauth-deny = Отклонить

# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Запись ожидает подтверждения
confirmed-page-title-booked = Запись подтверждена

confirmed-heading-reschedule-requested = Запрошен перенос
confirmed-heading-rescheduled = Запись перенесена!
confirmed-heading-pending = Ожидает подтверждения
confirmed-heading-booked = Вы записаны!

confirmed-subtitle-reschedule-requested = Запрос на перенос отправлен организатору ({ $host }). Когда перенос подтвердят, на { $email } придёт письмо.
confirmed-subtitle-rescheduled = Ваша запись перенесена. Письмо с подтверждением отправлено на { $email }.
confirmed-subtitle-pending = Запрос на запись отправлен организатору ({ $host }). Когда запись подтвердят, на { $email } придёт письмо.
confirmed-subtitle-booked = Письмо с подтверждением отправлено на { $email }.

confirmed-detail-event = Встреча:
confirmed-detail-date = Дата:
confirmed-detail-time = Время:
confirmed-detail-with = Организатор:
confirmed-detail-location = Место:
confirmed-detail-notes = Комментарий:
confirmed-detail-additional-guests = Дополнительные гости:

confirmed-book-another = Записаться ещё раз

confirmed-add-to-calendar = Добавить в календарь

# Slot picker (templates/slots.html)

slots-location-video = Видеозвонок
slots-location-phone = Телефонный звонок
slots-location-google-meet = Google Meet

slots-tz-label = Ваш часовой пояс
slots-tz-fixed = Время указано в часовом поясе { $tz }
slots-time-format-label = Формат времени
tz-group-common = Популярные часовые пояса
tz-region-africa = Африка
tz-region-america = Америка
tz-region-antarctica = Антарктида
tz-region-arctic = Арктика
tz-region-asia = Азия
tz-region-atlantic = Атлантический океан
tz-region-australia = Австралия
tz-region-europe = Европа
tz-region-indian = Индийский океан
tz-region-pacific = Тихий океан
slots-offset-note-day = %1: время указано по %2 (перевод часов — %3).
slots-offset-note-range = Перевод часов — %1: с этого дня время указано по %2.

slots-view-month = Месяц
slots-view-week = Неделя
slots-view-column = Список

slots-weekday-mon = Пн
slots-weekday-tue = Вт
slots-weekday-wed = Ср
slots-weekday-thu = Чт
slots-weekday-fri = Пт
slots-weekday-sat = Сб
slots-weekday-sun = Вс

slots-weekday-mon-short = Пн
slots-weekday-tue-short = Вт
slots-weekday-wed-short = Ср
slots-weekday-thu-short = Чт
slots-weekday-fri-short = Пт
slots-weekday-sat-short = Сб
slots-weekday-sun-short = Вс

slots-select-date = Выберите дату
slots-loading-availability = Загружаем свободное время…
slots-click-highlighted = Выберите отмеченную дату, чтобы увидеть свободное время
slots-bookable-range-both = Запись открыта с { $from } по { $until }
slots-bookable-range-from = Запись открыта с { $from }
slots-bookable-range-until = Запись открыта до { $until }
slots-no-longer-bookable = Запись на эту встречу закрыта.
slots-no-times-month = В этом месяце нет свободного времени
slots-no-times-day = В этот день нет свободного времени
slots-no-availability-participants = В этом месяце нет времени, когда свободны все участники
slots-week-more = ещё

# Booking form (templates/book.html)

book-page-title = Запись: { $title }
book-back-to-times = Назад к выбору времени
book-name-label = Ваше имя
book-name-placeholder = Анна Смирнова
book-email-label = Эл. почта
book-email-placeholder = anna@example.com
book-email-invalid = Введите адрес эл. почты полностью, с доменом (например, anna@example.com).
book-notes-label = Комментарий
book-notes-optional = (необязательно)
book-notes-placeholder = Что вы хотели бы обсудить?
book-additional-guests-label = Дополнительные гости
book-additional-guests-hint = (необязательно, не более { $max })
book-add-guest-btn = + Добавить адрес гостя
book-guest-email-placeholder = colleague@example.com
book-phone-label = Номер телефона
book-phone-placeholder = 8 912 345-67-89
book-phone-help = Можно ввести местный номер: если он начинается не с +, используется код страны { $country }.
book-phone-optional-consequence = Оставьте поле пустым, если не хотите получать SMS об этой записи.
book-phone-required = Для этой записи нужен номер телефона.
book-phone-invalid-title = Неверный номер телефона
book-phone-invalid = Введите номер, на который можно отправить SMS, или оставьте поле пустым.
book-phone-country-search = Поиск
book-phone-country-label = Выберите страну
book-phone-country-none = Страна не выбрана
book-phone-country-no-results = Ничего не найдено
captcha-label = Проверка безопасности
captcha-initial-state = Подтвердите, что вы не робот
captcha-verifying = Проверяем…
captcha-solved = Проверка пройдена
captcha-error = Ошибка
captcha-troubleshooting = Помощь
captcha-wasm-disabled = Включите WASM, чтобы проверка проходила намного быстрее
captcha-verify-aria = Нажмите, чтобы подтвердить, что вы не робот
captcha-verifying-aria = Идёт проверка, подождите
captcha-verified-aria = Проверка пройдена
captcha-required = Подтвердите, что вы не робот
captcha-error-aria = Что-то пошло не так, попробуйте ещё раз
book-confirm-button = Записаться

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Запись подтверждена: { $event }, { $date } в { $time } ({ $tz }).
sms-cancelled = Запись отменена: { $event }, { $date } в { $time } ({ $tz }).
sms-rescheduled = Запись перенесена: { $event }, теперь { $date } в { $time } ({ $tz }).
sms-reminder = Напоминание: { $event }, { $date } в { $time } ({ $tz }).

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Гость:
common-detail-reason = Причина:
common-reason-optional = (необязательно)
common-close-page = Эту страницу можно закрыть.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Отмена записи
cancel-heading = Отмена записи
cancel-subtitle = Вы собираетесь отменить запись.
cancel-reason-label = Причина
cancel-reason-placeholder-host = Сообщите организатору причину…
cancel-button = Отменить запись
cancelled-heading = Запись отменена
cancelled-subtitle = Ваша запись отменена, организатор получил уведомление.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Отклонение записи
decline-heading = Отклонение записи
decline-subtitle = Вы собираетесь отклонить этот запрос на запись.
decline-reason-placeholder-guest = Сообщите гостю причину…
decline-button = Отклонить запись
declined-heading = Запись отклонена
declined-subtitle = Запись отклонена, гость получил уведомление.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Подтверждение записи
approve-heading = Подтверждение записи
approve-subtitle = Вы собираетесь подтвердить этот запрос на запись.
approve-button = Подтвердить запись
approved-heading = Запись подтверждена
approved-subtitle = Запись подтверждена, письмо с подтверждением отправлено на { $email }.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Взять запись
claim-heading = Взять запись
claim-subtitle = Вы собираетесь взять эту запись. Вас добавят в участники встречи.
claim-assigned-to = Ответственный:
claim-button = Взять запись
claimed-page-title = Вы взяли запись
claimed-heading = Вы взяли запись
claimed-subtitle = Вы взяли эту запись. Приглашение в календарь отправлено вам на почту.
already-claimed-page-title = Запись уже взята
already-claimed-heading = Запись уже взята
already-claimed-subtitle = Эту запись уже взял другой участник: { $name }.

# Generic error page (booking_action_error.html)

action-error-page-title = Не удалось выполнить действие с записью

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Перенос записи — calrs
host-resched-heading = Перенос записи
host-resched-subtitle = { $guest } получит письмо с просьбой выбрать новое время.
host-resched-currently = Запланировано:
host-resched-button = Отправить запрос на перенос
host-resched-cancel-link = Отмена

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Подтверждение переноса
resched-confirm-heading = Подтверждение переноса
resched-confirm-subtitle = Вы собираетесь перенести запись на новое время.
resched-was = Было:
resched-new = Стало:
resched-button = Подтвердить перенос
resched-back-to-picker = Назад к выбору времени

# Base layout chrome (templates/base.html)

base-loader-checking = Проверяем свободное время
base-loader-please-wait = Подождите, загружаем актуальные данные календаря…
base-stop-impersonating = Вернуться в свой аккаунт
base-theme-toggle = Сменить тему
base-powered-by = Работает на

# Profile (templates/profile.html)

profile-pick-event-type-invite = Выберите тип встречи и запишитесь на удобное время.
profile-no-event-type = Пока нет доступных типов встреч.

# Lengths of time (i18n::format_duration and format_duration_long, the
# templates' duration_label() and duration_words()): the short form labels a
# meeting ("45 min", "1 h 30"), the long one is read in sentences and by
# screen readers, with the language's plurals.
duration-short-m = { $minutes } мин
duration-short-h = { $hours } ч
duration-short-hm = { $hours } ч { $minutes } мин
duration-long-m =
    { $minutes ->
        [one] { $minutes } минута
        [few] { $minutes } минуты
        [many] { $minutes } минут
       *[other] { $minutes } минуты
    }
duration-long-h =
    { $hours ->
        [one] { $hours } час
        [few] { $hours } часа
        [many] { $hours } часов
       *[other] { $hours } часа
    }
duration-long-hm =
    { $hours ->
        [one] { $hours } час
        [few] { $hours } часа
        [many] { $hours } часов
       *[other] { $hours } часа
    } { $minutes ->
        [one] { $minutes } минута
        [few] { $minutes } минуты
        [many] { $minutes } минут
       *[other] { $minutes } минуты
    }
duration-long-d =
    { $days ->
        [one] { $days } день
        [few] { $days } дня
        [many] { $days } дней
       *[other] { $days } дня
    }
# Public landing pages (/u/{username}, /team/{slug}).
landing-with-one-of = с одним из участников
faces-and-more = и ещё { $count }
landing-with-everyone = со всеми участниками
# The usual hours beside the host's name: landing-hours-heading over a
# timetable, one row per schedule: the days with the short weekday names
# (landing-days-span for a run, landing-days-every-short for the whole week,
# else a list joined with list-and; the row starts capitalised), the hours
# with landing-time-range (%1 and %2 the times). The "My time" button
# (landing-hours-toggle) rewrites the rows in the visitor's timezone, with
# landing-time-range-next when they end the next day, and the heading as
# landing-hours-heading-yours. With too many schedules, landing-hours-vary
# alone; its $days is landing-days-range, landing-days-every or a list.
# landing-host-confirms labels a meeting whose requests the host confirms by
# hand (landing-on-request-hint explains it).
landing-days-range = { $first }–{ $last }
landing-days-every = каждый день
landing-on-request-hint = Каждый запрос организатор подтверждает вручную.
landing-how-it-works = Выберите тему и удобное время — подтверждение придёт на эл. почту.
landing-based-in = Местоположение: { $place }
landing-speaks = Языки: { $languages }
landing-meetings = Встречи
landing-hours-vary = Часы приёма: { $days }, в разные дни по-разному (часовой пояс: { $zone })
landing-time-range = %1–%2
landing-time-range-next = %1–%2 следующего дня
landing-other-meetings = Другие встречи
landing-hours-heading = Часы приёма (часовой пояс: { $zone })
landing-hours-heading-yours = Часы приёма (ваше время)
landing-hours-toggle = Моё время
landing-hours-toggle-hint = Показать эти часы в вашем часовом поясе
landing-days-every-short = Каждый день
landing-days-span = { $first }–{ $last }
landing-host-confirms = Подтверждает организатор
list-and = { $first } и { $last }

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

common-month-1 = январь
common-month-2 = февраль
common-month-3 = март
common-month-4 = апрель
common-month-5 = май
common-month-6 = июнь
common-month-7 = июль
common-month-8 = август
common-month-9 = сентябрь
common-month-10 = октябрь
common-month-11 = ноябрь
common-month-12 = декабрь
common-month-short-1 = янв.
common-month-short-2 = февр.
common-month-short-3 = мар.
common-month-short-4 = апр.
common-month-short-5 = мая
common-month-short-6 = июн.
common-month-short-7 = июл.
common-month-short-8 = авг.
common-month-short-9 = сент.
common-month-short-10 = окт.
common-month-short-11 = нояб.
common-month-short-12 = дек.

common-weekday-long-mon = понедельник
common-weekday-long-tue = вторник
common-weekday-long-wed = среда
common-weekday-long-thu = четверг
common-weekday-long-fri = пятница
common-weekday-long-sat = суббота
common-weekday-long-sun = воскресенье

# Format patterns are parametric per locale to handle word order. Translators
# pick where each placeholder lands. Example outputs:
#   EN: April 2026  /  Tuesday, March 12, 2026
#   FR: avril 2026  /  mardi 12 mars 2026
#   ES: abril 2026  /  martes, 12 de marzo de 2026
common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday }, { $day } { $monthnum ->
    [1] января
    [2] февраля
    [3] марта
    [4] апреля
    [5] мая
    [6] июня
    [7] июля
    [8] августа
    [9] сентября
    [10] октября
    [11] ноября
   *[12] декабря
} { $year }
common-format-date =
    { $day } { $monthnum ->
    [1] января
    [2] февраля
    [3] марта
    [4] апреля
    [5] мая
    [6] июня
    [7] июля
    [8] августа
    [9] сентября
    [10] октября
    [11] ноября
    *[12] декабря
    } { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Перенести
email-action-cancel-booking = Отменить запись

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
email-confirm-greeting = Здравствуйте, { $name }!
email-confirm-headline = Ваша запись подтверждена.
email-confirm-ics-attached-plain = Приглашение в календарь во вложении.
email-confirm-ics-attached-html = К письму приложено приглашение в календарь.
email-confirm-need-to-cancel = Нужно отменить? { $url }

# Email: guest reminder

email-reminder-subject = Напоминание: { $event } в { $time }
email-reminder-headline = Встреча скоро начнётся.

# Email: guest cancellation

email-cancel-subject = Отменено: { $event } — { $date }
email-cancel-headline-by-host = Ваша запись отменена организатором ({ $host }).
email-cancel-headline-by-guest = Ваша запись отменена.
email-cancel-ics-attached-plain = Уведомление об отмене для календаря во вложении.
email-cancel-ics-attached-html = К письму приложено уведомление об отмене для календаря.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Важно: минимальный срок для отмены — { $duration } до начала встречи.
email-confirm-reschedule-notice = Важно: минимальный срок для переноса — { $duration } до начала встречи.

# Event type form: Google Meet location + cancel/reschedule minimum notice
# (templates/event_type_form.html, src/google_meet.rs)

event-type-form-location-google-meet = Google Meet (ссылка создаётся автоматически)
event-type-form-location-google-meet-hint = Уникальная ссылка Google Meet создаётся при подтверждении записи, её владелец — назначенный организатор. У каждого организатора (у вас или у каждого подходящего участника команды) должен быть подключён Google Calendar и выбран календарь для новых записей.

google-meet-prereq-no-host = Для Google Meet нужен организатор с подключённым Google Calendar.
google-meet-prereq-no-eligible = Для Google Meet нужен хотя бы один подходящий участник команды с подключённым Google Calendar.
google-meet-prereq-missing = Для Google Meet у каждого организатора должен быть подключён Google Calendar и выбран календарь для новых записей. Ещё не настроили: { $names }. Подключить календарь можно в разделе «Источники календарей».
google-meet-unavailable-title = Google Meet недоступен
google-meet-dynamic-group-unavailable = Организатору нужно подключить Google Calendar и выбрать календарь для новых записей.

event-type-form-cancel-notice-label = Минимальный срок для отмены
event-type-form-reschedule-notice-label = Минимальный срок для переноса
event-type-form-notice-help = Оставьте пустым, чтобы не ограничивать.
event-type-form-resources-label = Необходимые ресурсы
event-type-form-resources-hint = Время доступно для записи, только если выбранные ресурсы свободны (с учётом режима ниже).
event-type-form-resources-mode-all = Все выбранные ресурсы должны быть свободны
event-type-form-resources-mode-round-robin = Достаточно одного свободного ресурса (он закрепляется за записью)
event-type-form-notice-unit-minutes = минуты
event-type-form-notice-unit-hours = часы
event-type-form-notice-unit-days = дни
event-type-form-booking-horizon-label = Глубина записи
event-type-form-booking-horizon-help = На сколько дней вперёд можно записаться. Оставьте пустым, чтобы не ограничивать; 0 — только на сегодня.
event-type-form-bookable-range-label = Период записи
event-type-form-bookable-from = с
event-type-form-bookable-until = по
event-type-form-bookable-range-help = Записаться можно только на эти дни. Оставьте пустым, чтобы не ограничивать. Чтобы задать другие часы для отдельных дней, используйте исключения для дат.
event-type-form-bookable-range-past-warning = Этот период уже закончился. Тип встречи больше не показывается публично.
event-type-form-bookable-range-too-far-warning = Этот период начинается больше чем через год. Записаться можно максимум на год вперёд, поэтому запись откроется с { $date }.
event-type-form-bookable-range-horizon-warning = Глубина записи заканчивается раньше, чем начинается этот период. Гости смогут записаться с { $date }.
event-type-form-bookable-range-bookings-outside =
    { $count ->
        [one] { $count } уже созданная запись не попадает в период записи.
        [few] { $count } уже созданные записи не попадают в период записи.
        [many] { $count } уже созданных записей не попадают в период записи.
       *[other] { $count } уже созданные записи не попадают в период записи.
    }
event-type-form-bookable-range-bookings-link = Показать записи
event-type-form-saved = Сохранено.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = Минимальный срок для отмены — { $duration } до начала встречи.
confirmed-reschedule-notice-info = Минимальный срок для переноса — { $duration } до начала встречи.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Эту запись уже нельзя отменить онлайн
booking-blocked-title-reschedule = Эту запись уже нельзя перенести онлайн
booking-blocked-body = Организатор установил минимальный срок для изменений — { $duration } до начала встречи. Если вы не сможете прийти, напишите организатору напрямую: <a href="mailto:{ $host_email }">{ $host_email }</a>.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Скопировать
dashboard-event-types-copied = Скопировано
dashboard-event-types-copy-title = Скопировать ссылку для записи
dashboard-event-types-copy-failed = Не удалось скопировать

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Планирование
nav-overview = Обзор
nav-event-types = Типы встреч
nav-bookings = Записи
nav-teams = Команды
nav-section-shared-links = Общие ссылки
nav-invite-links = Ссылки-приглашения
nav-section-calendars = Календари
nav-sources = Источники
nav-section-personal = Личное
nav-settings = Профиль и настройки
nav-troubleshoot = Диагностика
nav-section-admin = Администрирование
nav-admin-panel = Панель администратора
nav-sign-out = Выйти
nav-release-notes = Список изменений
dash-skip-to-content = Перейти к содержимому
dash-open-menu = Открыть меню
dash-shortcuts-title = Сочетания клавиш
dash-shortcut-search = Поиск по странице
dash-shortcut-new = Создать
dash-shortcut-save = Сохранить форму
dash-shortcut-close = Закрыть меню и диалоги
dash-shortcut-help = Показать эту справку
dash-shortcut-go = Перейти: { $page }
dash-shortcut-then = затем
dash-close = Закрыть
dash-copied = Ссылка скопирована
dash-copy-manual = Скопируйте ссылку:
dash-confirm-again = Нажмите ещё раз, чтобы подтвердить
dash-unsaved = Несохранённые изменения
dash-no-matches = Ничего не найдено.
dash-more-actions = Другие действия

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = Часовой пояс браузера — { $detected }, а для записей у вас указан { $current }.
tz-banner-update = Обновить
tz-banner-dismiss = Скрыть

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = Введите адрес ссылки:
editor-link-default-label = текст ссылки
editor-placeholder-text = текст
editor-nothing-to-preview = Нечего показать

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Обзор
overview-avail-banner-body = Для вас установлены рабочие часы по умолчанию: пн–пт, 9:00–17:00. Они учитываются, когда другие добавляют вас во встречи по групповой ссылке.
overview-avail-banner-cta = Проверить рабочие часы
overview-dismiss = Скрыть
overview-step-connect-calendar = Подключите календарь
overview-step-first-event-type = Создайте первый тип встречи
overview-pending-approval = Ждут вашего решения
overview-confirm = Подтвердить
overview-decline = Отклонить
overview-copy-link = Скопировать ссылку для записи
overview-chart-title = Встречи по неделям
overview-chart-this-week = Эта неделя
overview-chart-week-of = Неделя с { $date }
overview-series-confirmed = Подтверждённые
overview-series-pending = Ожидающие подтверждения
overview-series-cancelled = Отменённые или отклонённые
overview-view-data = Показать в виде таблицы
overview-table-week = Неделя
overview-types-title = По типам встреч
overview-types-other = Другие типы
overview-agenda-title = Ближайшие встречи
overview-agenda-today = Сегодня
overview-agenda-tomorrow = Завтра
overview-agenda-join = Подключиться
overview-agenda-empty = В ближайшие 14 дней встреч нет. Поделитесь ссылкой для записи, чтобы к вам могли записаться.
overview-agenda-all = Все записи
nav-main-label = Основное меню
dash-shortcuts-short = Клавиши
dash-hint-to-decide = нужно решить: { $count }
dash-hint-failing = с ошибками: { $count }
common-format-day-date = { $weekday }, { $day } { $monthnum ->
        [1] января
        [2] февраля
        [3] марта
        [4] апреля
        [5] мая
        [6] июня
        [7] июля
        [8] августа
        [9] сентября
        [10] октября
        [11] ноября
       *[12] декабря
    }
common-format-short-date = { $day } { $month }
bookings-summary = Предстоящие: { $upcoming }. Ждут вашего решения: { $pending }. Часовой пояс: { $tz }.
sources-state-failing = Ошибка синхронизации
troubleshoot-previous-day = Предыдущий день
troubleshoot-next-day = Следующий день
overview-new-meeting-type = Новый тип встречи
overview-step-connect = Подключить
overview-step-create = Создать
overview-links-title = Ваши ссылки для записи
overview-open-calendar = Открыть календарь
overview-week-title = Эта неделя
overview-week-closed = выходной
overview-week-total = Занято: { $booked }
overview-week-short = Нед. { $week }
overview-time-range = { $from }–{ $to }
overview-until = до { $time }
overview-expires = Истекает: { $when }
overview-guest-and-more = { $name } и ещё { $count }
overview-greeting-night = Здравствуйте, { $name }
overview-greeting-morning = Доброе утро, { $name }
overview-greeting-afternoon = Добрый день, { $name }
overview-greeting-evening = Добрый вечер, { $name }
overview-mast-date = { $date }, неделя { $week }
overview-sum-now = Сейчас идёт встреча: { $guest }, до { $time }.
overview-sum-today = Сегодня в { $time } встреча: { $guest }.
overview-sum-tomorrow = Завтра в { $time } встреча: { $guest }.
overview-sum-later = Следующая встреча: { $guest }, { $day }, в { $time }.
overview-sum-more-today = { $count ->
    [one] Сегодня ещё { $count } встреча.
    [few] Сегодня ещё { $count } встречи.
    [many] Сегодня ещё { $count } встреч.
   *[other] Сегодня ещё { $count } встречи.
}
overview-sum-none = В ближайшие 14 дней записей нет.
overview-sum-pending = { $count ->
    [one] { $count } запрос ждёт вашего ответа.
    [few] { $count } запроса ждут вашего ответа.
    [many] { $count } запросов ждут вашего ответа.
   *[other] { $count } запроса ждут вашего ответа.
}
overview-sum-guests = { $count ->
    [one] { $name } и ещё { $count }
    [few] { $name } и ещё { $count }
    [many] { $name } и ещё { $count }
   *[other] { $name } и ещё { $count }
}
overview-hero-title = Следующая встреча
overview-when-now = Сейчас, до { $time }
overview-when-minutes = { $count ->
    [one] через { $count } минуту
    [few] через { $count } минуты
    [many] через { $count } минут
   *[other] через { $count } минуты
}
overview-when-hours = { $count ->
    [one] через { $count } час
    [few] через { $count } часа
    [many] через { $count } часов
   *[other] через { $count } часа
}
overview-when-today-night = сегодня ночью
overview-when-today-morning = сегодня утром
overview-when-today-afternoon = сегодня днём
overview-when-today-evening = сегодня вечером
overview-when-tomorrow-night = завтра рано утром
overview-when-tomorrow-morning = завтра утром
overview-when-tomorrow-afternoon = завтра днём
overview-when-tomorrow-evening = завтра вечером
overview-when-weekday-night = { $day }, рано утром
overview-when-weekday-morning = { $day }, утром
overview-when-weekday-afternoon = { $day }, днём
overview-when-weekday-evening = { $day }, вечером
overview-when-days = { $count ->
    [one] через { $count } день
    [few] через { $count } дня
    [many] через { $count } дней
   *[other] через { $count } дня
}
overview-pulse-held = { $count ->
    [one] За последние 30 дней — { $count } встреча, { $time } с гостями.
    [few] За последние 30 дней — { $count } встречи, { $time } с гостями.
    [many] За последние 30 дней — { $count } встреч, { $time } с гостями.
   *[other] За последние 30 дней — { $count } встречи, { $time } с гостями.
}
overview-pulse-held-vs = { $count ->
    [one] За последние 30 дней — { $count } встреча, { $time } с гостями (в предыдущие 30 дней — { $prev }).
    [few] За последние 30 дней — { $count } встречи, { $time } с гостями (в предыдущие 30 дней — { $prev }).
    [many] За последние 30 дней — { $count } встреч, { $time } с гостями (в предыдущие 30 дней — { $prev }).
   *[other] За последние 30 дней — { $count } встречи, { $time } с гостями (в предыдущие 30 дней — { $prev }).
}
overview-pulse-booked = { $count ->
    [one] За тот же период — { $count } новая запись.
    [few] За тот же период — { $count } новые записи.
    [many] За тот же период — { $count } новых записей.
   *[other] За тот же период — { $count } новые записи.
}
overview-pulse-booked-only = { $count ->
    [one] За последние 30 дней — { $count } новая запись.
    [few] За последние 30 дней — { $count } новые записи.
    [many] За последние 30 дней — { $count } новых записей.
   *[other] За последние 30 дней — { $count } новые записи.
}
overview-links-page = Ваша страница
overview-agenda-nothing-else = В ближайшие 14 дней больше ничего нет.
overview-setup-count = { $count ->
    [one] Осталось настроить { $count } пункт
    [few] Осталось настроить { $count } пункта
    [many] Осталось настроить { $count } пунктов
   *[other] Осталось настроить { $count } пункта
}
overview-ruler-booked = Занято
overview-ruler-pending = Ждёт решения
overview-ruler-unavailable = Недоступно (ваш календарь)
overview-key-open = Открыто для записи
overview-ruler-unreadable = Повторяющееся событие, которое calrs не может прочитать
overview-ruler-unreadable-tip = На всякий случай считается занятым. В диагностике видно, какое это событие.
overview-figures-title = Последние 30 дней

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Записи
bookings-pending-approval = Ждут вашего решения
bookings-available-to-claim = Можно взять
bookings-upcoming = Предстоящие записи
bookings-guest-booked = Время гостя:
bookings-resource = Ресурс:
bookings-confirm = Подтвердить
bookings-reschedule = Перенести
bookings-decline = Отклонить
bookings-claim = Взять
bookings-badge-awaiting-reschedule = ждёт переноса
bookings-badge-writeback-failed = нет в вашем календаре
bookings-writeback-failed-detail = Не удалось добавить в календарь: { $error }
bookings-cancel = Отменить
bookings-reason-placeholder = Причина (необязательно)
bookings-confirm-cancel = Подтвердить отмену
bookings-back = Назад
bookings-empty = Предстоящих записей пока нет.<br>Поделитесь { $link }, чтобы к вам могли записаться.
bookings-empty-link-label = ссылками на типы встреч
bookings-search-placeholder = Поиск по гостям, эл. почте и встречам
bookings-recent = Последние 30 дней
bookings-cancelled = Отменённые и отклонённые

# Host notes on a booking (templates/booking_notes.html, bookings page, overview, host emails)

host-notes-title = Заметки организатора
host-notes-hint = Их видят организаторы этой записи, участник команды, который её взял, и администратор, вошедший от имени одного из организаторов. Гость их не видит, в приглашение и событие календаря они не попадают.
host-notes-placeholder = Например: о чём договорились во время звонка, что подготовить.
host-notes-save = Сохранить заметки
host-notes-saved = Заметки сохранены.
host-notes-too-long = Заметки слишком длинные (максимум символов: { $max }).
host-notes-conflict = Пока страница была открыта, заметки изменились. Сохранённая версия показана ниже; если сохранить ещё раз, её заменит текст из поля.
host-notes-current = Сохранённая версия:
host-notes-last-edit = Изменено: { $name }, { $when }
host-notes-last-edit-anon = Изменено: { $when }
host-notes-edit = Изменить заметку
host-notes-marker = Заметка организатора
host-notes-cleared = (заметки удалены)
host-notes-last-edit-as = Изменено: { $name } (администратор, от имени { $host }), { $when }
dash-not-found-title = Не найдено
dash-not-found-body = Такой страницы нет, или у этого аккаунта нет к ней доступа.
host-notes-from-guest = Комментарий гостя:
bookings-host-notes = Заметки организатора:
email-detail-host-notes = Заметки организатора:

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Команды
teams-heading = Команды
teams-new = Создать
teams-badge-public = публичная
teams-badge-private = закрытая
teams-settings = Настройки
teams-view = Открыть
teams-empty = Команд пока нет.
teams-empty-admin = { $link }, чтобы работать вместе с коллегами.
teams-empty-admin-link-label = Создайте команду
teams-empty-member = Команды создают администраторы. Попросите администратора создать команду и добавить вас в неё.
teams-subtitle = Люди, которые ведут общие встречи — по очереди или все вместе.
teams-search-placeholder = Фильтр по названию или ссылке

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Ссылки-приглашения
invite-links-heading = Ссылки-приглашения
invite-links-new = Создать внутренний тип встречи
invite-links-help = Создавайте одноразовые ссылки для записи на внутренние типы встреч. Любой коллега с аккаунтом может создавать здесь ссылки и делиться ими.
invite-links-hosted-by = Организатор: { $host }
invite-links-get-link = Получить ссылку
invite-links-invites = Приглашения
invite-links-empty = Внутренних типов встреч пока нет.<br>{ $link } с видимостью «Внутренний», чтобы любой коллега мог создавать ссылки для записи.
invite-links-empty-link-label = Создайте тип встречи
invite-links-js-generating = Создаём…
invite-links-js-copied = Скопировано
invite-links-js-error = Ошибка

teams-member-count =
    { $count ->
        [one] { $count } участник
        [few] { $count } участника
        [many] { $count } участников
       *[other] { $count } участника
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Источники календарей
sources-heading = Источники календарей
sources-add = Добавить
sources-last-sync = Последняя синхронизация:
sources-sync-failing = Ошибки синхронизации начиная с { $since } UTC: { $error }
sources-sync = Синхронизировать
sources-full-resync = Полная синхронизация
sources-full-resync-title = Очистить кэш и заново загрузить все события с сервера
sources-test = Проверить
sources-reconnect = Переподключить
sources-reconnect-title = Заново пройти авторизацию в Google
sources-edit = Изменить
sources-remove = Удалить
sources-remove-confirm = Удалить источник «{ $name }»? Все события, синхронизированные из него, будут удалены.
sources-no-write-calendar = Календарь для новых записей не выбран. Подтверждённые записи остаются в calrs и не попадают в этот календарь. Выберите календарь ниже, чтобы записи добавлялись в него.
sources-write-calendar-read-only = Календарь для новых записей стал доступен только для чтения, поэтому записи в него не попадают. Выберите ниже календарь, в который можно добавлять события.
sources-write-calendar-refused = В этот календарь нельзя добавлять записи: он не относится к этому источнику или у аккаунта нет прав на его изменение. Выберите другой.
sources-microsoft-not-ready = calrs ещё не закончил загрузку этого календаря. Пока загрузка не завершится, на ваших страницах записи не будет свободного времени. Обычно это занимает до минуты после подключения; если дольше — причину покажет ошибка синхронизации выше.
sources-write-bookings-to = Добавлять записи в:
sources-write-none = Никуда (не добавлять)
sources-empty = Источники календарей не подключены. { $link }, чтобы calrs учитывал вашу занятость.
sources-empty-link-label = Добавьте источник
sources-subtitle = Календари, по которым calrs узнаёт, когда вы заняты, и куда добавляет новые записи.
sources-never-synced = Ещё не синхронизирован
sources-calendars = Календари
sources-calendars-synced = Синхронизируется календарей: { $synced } из { $total }
source-calendars-page-title = Календари для синхронизации
source-calendars-heading = Календари для синхронизации
source-calendars-back = Вернуться к источникам календарей
source-calendars-help = Выберите календари источника «{ $name }», которые будет синхронизировать calrs. Неотмеченные календари не загружаются и не влияют на вашу занятость.
source-calendars-select-all = Выбрать все
source-calendars-select-none = Снять все
source-calendars-write-badge = Сюда добавляются записи
source-calendars-write-hint = Подтверждённые записи добавляются в этот календарь, даже если он не синхронизируется.
source-calendars-save = Сохранить
source-calendars-saved = Сохранено. Только что отмеченные календари загрузятся при следующей синхронизации; чтобы загрузить их сейчас, нажмите «Синхронизировать» на странице источников.
source-calendars-empty = Для этого источника календари пока не найдены. Запустите синхронизацию, чтобы их найти.

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Типы встреч
event-types-heading = Типы встреч
event-types-new = Создать
event-types-badge-disabled = выключен
event-types-badge-internal = внутренний
event-types-badge-private = закрытый
event-types-badge-resources = ресурсы
event-types-badge-active = активен
event-types-badge-expired = истёк
event-types-badge-range-from = с { $date }
event-types-badge-range-until = до { $date }
event-types-send-invites = Отправить приглашения
event-types-mode-collective = совместная
event-types-mode-round-robin = по очереди
event-types-edit = Изменить
event-types-disable = Выключить
event-types-enable = Включить
event-types-embed = Встроить
event-types-overrides = Исключения для дат
event-types-team-settings = Настройки команды
event-types-invites = Приглашения
event-types-view-public = Открыть публичную страницу
event-types-view-page = Открыть страницу
event-types-delete = Удалить
event-types-delete-confirm = Удалить тип встречи «{ $title }»? Это действие нельзя отменить.
event-types-duplicate = Создать копию
event-types-duplicate-title = { $title } (копия)
event-types-empty = Типов встреч пока нет. { $link }, чтобы начать принимать записи.
event-types-empty-link-label = Создайте тип встречи
event-types-subtitle = Встречи, на которые к вам можно записаться. Скопируйте ссылку или встройте её на свой сайт.
event-types-search-placeholder = Фильтр по названию или ссылке
event-types-switch-label = Приём записей

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Жирный (Ctrl+B)
editor-italic = Курсив (Ctrl+I)
editor-strikethrough = Зачёркнутый
editor-code = Код
editor-link = Вставить ссылку (Ctrl+K)
editor-toggle-preview = Показать или скрыть предпросмотр
editor-preview = Предпросмотр

# Profile and settings (templates/settings.html)

settings-page-title = Настройки
settings-heading = Профиль и настройки
settings-subtitle = Ваш публичный профиль, язык, часовой пояс и рабочие часы по умолчанию.
settings-public-page-label = Ваша публичная страница записи
settings-copy = Скопировать
settings-copied = Скопировано
settings-open = Открыть
settings-avatar = Фото профиля
settings-upload = Загрузить
settings-remove = Удалить
settings-display-name = Отображаемое имя
settings-display-name-placeholder = Ваше имя
settings-username = Имя пользователя
settings-username-hint = (используется в ссылке на вашу страницу записи)
settings-username-pattern-title = Только строчные латинские буквы, цифры и дефисы
settings-username-help = Ваша публичная страница записи:
settings-title = Должность
settings-title-placeholder = например, разработчик или менеджер продукта
settings-title-help = Показывается в вашем публичном профиле и на боковой панели.
settings-bio = О себе
settings-bio-placeholder = Расскажите немного о себе…
settings-bio-help = Показывается на вашей публичной странице записи. Можно использовать **жирный**, *курсив*, ~~зачёркнутый~~, `код` и [ссылки](url).
settings-booking-email = Эл. почта для записей
settings-booking-email-help = Этот адрес будет указан на ваших публичных страницах записи и в уведомлениях по эл. почте. Оставьте поле пустым, чтобы использовать адрес, с которым вы входите.
settings-booking-email-warning = Убедитесь, что такой адрес есть у вашего почтового сервиса, иначе уведомления не дойдут.
settings-timezone = Часовой пояс
settings-timezone-help = Ваши правила доступности и время записей рассчитываются в этом часовом поясе.
settings-language = Язык
settings-language-auto = Авто (как в браузере)
settings-language-help = Выберите язык интерфейса или оставьте «Авто», чтобы использовать настройку браузера.
settings-public-page-language = Язык публичной страницы записи
settings-public-page-language-visitor = Язык посетителя
settings-public-page-language-host = Всегда мой язык
settings-public-page-language-help = Язык вашего публичного профиля, страниц записи и писем, которые получают ваши гости. «Всегда мой язык» — язык, выбранный выше.
settings-error-public-page-language = Выберите допустимый язык публичной страницы записи.
settings-dynamic-group = Разрешить другим добавлять меня в групповые ссылки
settings-dynamic-group-help = Если включено, другие пользователи могут создавать ссылки на совместные встречи с вашим участием (например, { $example }).
settings-lend-resource = Разрешить использовать доступ к моему календарю для бронирования ресурсов
settings-lend-resource-help = Если для записи нужно забронировать общий ресурс (демолабораторию, переговорную), а ваш аккаунт календаря может добавлять в него события, calrs сможет использовать для этого ваши сохранённые учётные данные календаря.
settings-default-availability = Доступность по умолчанию
settings-default-availability-help = Ваши рабочие часы по умолчанию. Используются в групповых ссылках, когда другие добавляют вас во встречу.
settings-copy-to-all = Скопировать на все дни
settings-copy-to-all-title = Скопировать интервалы первого включённого дня на все остальные включённые дни
settings-add-window = Добавить интервал
settings-remove-window = Убрать интервал
settings-save = Сохранить настройки
settings-appearance = Внешний вид
settings-card-profile = Профиль
settings-card-languages = Языки
settings-theme-system = Как в системе
settings-theme-light = Светлая
settings-theme-dark = Тёмная

# Sign in (templates/auth/login.html)

login-page-title = Вход
login-heading = Вход
login-subtitle = Войдите в свой аккаунт calrs
login-sso = Войти через SSO
login-or = или
login-email = Эл. почта
login-password = Пароль
login-submit = Войти по эл. почте
login-no-account = Нет аккаунта? { $link }
login-register-link = Зарегистрироваться

# Registration (templates/auth/register.html)

register-page-title = Регистрация
register-heading = Создание аккаунта
register-subtitle = Создайте новый аккаунт calrs
register-domains-limited = Регистрация доступна только для доменов: { $domains }
register-name = Имя
register-name-placeholder = Ваше имя
register-email = Эл. почта
register-password = Пароль
register-password-hint = (не менее 12 символов)
register-submit = Создать аккаунт
register-have-account = Уже есть аккаунт? { $link }
register-signin-link = Войти

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Слишком много попыток входа. Попробуйте позже.
auth-error-invalid-credentials = Неверный адрес эл. почты или пароль
auth-error-internal = Внутренняя ошибка
auth-error-registration-disabled = Регистрация отключена.
auth-error-name-length = Имя должно содержать от 1 до 255 символов
auth-error-email-length = Адрес эл. почты должен содержать от 1 до 255 символов
auth-error-email-invalid = Введите корректный адрес эл. почты
auth-error-email-domain = Этот почтовый домен не разрешён
auth-error-password-length = Пароль должен быть не короче 12 символов
auth-error-email-taken = Этот адрес эл. почты уже зарегистрирован
auth-sso-linked-elsewhere = Учётная запись с этим адресом эл. почты уже привязана к другой учётной записи SSO. Обратитесь к администратору.
auth-sso-unverified-link = Учётная запись с этим адресом эл. почты уже существует, но ваш поставщик удостоверений не подтвердил, что адрес принадлежит вам, поэтому связать их автоматически нельзя. Обратитесь к администратору.
auth-sso-local-mfa = Учётная запись с этим адресом эл. почты уже существует и использует двухфакторную аутентификацию, поэтому её нельзя автоматически привязать к SSO. Обратитесь к администратору.
auth-sso-no-account = Учётной записи с этим адресом эл. почты нет, а новые учётные записи автоматически не создаются. Обратитесь к администратору.
auth-sso-unverified-register = Ваш поставщик удостоверений не подтвердил ваш адрес эл. почты. Попросите администратора подтвердить его у поставщика и попробуйте снова.
auth-sso-no-email = Ваш поставщик удостоверений не передал адрес эл. почты. Попросите администратора открыть его для calrs и попробуйте снова.
auth-error-create-failed = Не удалось создать аккаунт

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Источник календарей
source-test-sync-heading = Синхронизация: { $name }
source-test-heading = Проверка подключения
source-write-page-title = Настройка добавления в календарь
source-write-back = Вернуться в кабинет
source-write-heading = Куда добавлять записи?
source-write-help = Когда кто-то записывается к вам на встречу, calrs может сам создать событие в вашем календаре. Выберите календарь источника «{ $name }», в который будут добавляться записи.
source-write-save = Сохранить
source-write-skip = Не сейчас
source-write-sync-results = Результаты синхронизации

source-write-event-count =
    { $count ->
        [one] { $count } событие
        [few] { $count } события
        [many] { $count } событий
       *[other] { $count } события
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Исключения для дат
overrides-heading = Исключения для дат
overrides-back-teams = Вернуться к командам
overrides-back-event-types = Вернуться к типам встреч
overrides-intro = Добавьте исключения для отдельных дат: { $title }
overrides-add-heading = Новое исключение
overrides-date = Дата
overrides-type = Тип исключения
overrides-type-blocked = Закрыть весь день
overrides-type-custom = Особые часы
overrides-start-time = Начало
overrides-end-time = Конец
overrides-add-submit = Добавить исключение
overrides-existing = Текущие исключения
overrides-badge-blocked = закрыто
overrides-badge-custom = особые часы
overrides-delete = Удалить
overrides-delete-confirm = Удалить это исключение?
overrides-empty = Исключений для дат пока нет.<br>В форме выше можно закрыть отдельные даты (праздники, выходные) или задать особые часы.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Выберите тип встречи, чтобы записаться.
team-profile-empty = Доступных типов встреч пока нет.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Диагностика
troubleshoot-empty = Типы встреч не найдены. { $link }, чтобы начать диагностику доступности.
troubleshoot-empty-link-label = Создайте тип встречи
troubleshoot-subtitle = Почему время свободно или недоступно: { $title }
troubleshoot-buffer-before = перерыв { $duration } до встречи
troubleshoot-buffer-after = перерыв { $duration } после встречи
troubleshoot-min-notice = мин. срок: { $duration }
troubleshoot-blocked-override = Закрыто исключением для даты (выходной)
troubleshoot-custom-hours-active = Действует исключение с особыми часами (вместо недельного расписания)
troubleshoot-microsoft-not-ready = Первая синхронизация вашего календаря Microsoft ещё не завершена: до её окончания всё время показывается как занятое. См. «Источники календарей».
troubleshoot-legend-available = Свободно
troubleshoot-legend-calendar-event = Событие календаря
troubleshoot-legend-booking = Запись
troubleshoot-legend-resource = Ресурс занят
troubleshoot-legend-outside = Вне часов доступности
troubleshoot-legend-buffer = Перерыв / мин. срок
troubleshoot-blocked-slots = Недоступное время
troubleshoot-none-date-blocked = Эта дата закрыта исключением (выходной). Свободного времени нет.
troubleshoot-none-custom-hours = Действует исключение с особыми часами, но подходящих интервалов нет. Проверьте настройки исключения.
troubleshoot-none-no-rules = Для этого дня недели нет правил доступности. В этот день ({ $date }) записаться на этот тип встречи нельзя.
troubleshoot-none-all-bookable = В часы доступности ничего не заблокировано: записаться можно на любое время.
troubleshoot-label-outside = Вне часов доступности
troubleshoot-label-available = Свободно
troubleshoot-label-min-notice = Мин. срок ({ $duration })
troubleshoot-label-beyond-horizon = За пределами глубины записи (дней: { $days })
troubleshoot-label-outside-date-range = Вне периода записи
troubleshoot-label-buffer = Перерыв ({ $duration })
troubleshoot-label-resource-busy = Ресурс занят: { $names }
troubleshoot-detail-around = Рядом: { $label }
troubleshoot-detail-around-booking = Рядом с записью: { $guest }
troubleshoot-reason-calendar-event = Событие календаря: { $label }
troubleshoot-reason-booking = Запись: { $label }

# Invite management (templates/invite_form.html)

invites-heading = Приглашения
invites-back-teams = Вернуться к командам
invites-back-event-types = Вернуться к типам встреч
invites-intro = Отправьте ссылки-приглашения на встречу «{ $title }»
invites-capped = <strong>Обработаны не все адреса: за один раз можно отправить не более { $max }.</strong> Остальные отправьте отдельно.
invites-failed-hint = Подробности — в журнале сервера.
invites-quick-link = Быстрая ссылка
invites-quick-link-help = Создайте одноразовую ссылку — она сразу скопируется в буфер обмена.
invites-get-link = Получить ссылку
invites-or-email = Или отправьте по эл. почте
invites-recipients = Получатели
invites-recipients-hint = (по одному адресу в строке, не более { $max })
invites-message = Личное сообщение
invites-message-hint = (необязательно, его получат все адресаты)
invites-message-placeholder = С удовольствием покажу вам, как всё работает…
invites-expires-in = Срок действия
invites-expires-days = { $days } дн.
invites-expires-never = Бессрочно
invites-allow-multiple = Разрешить получателю записываться несколько раз
invites-send = Отправить приглашения
invites-sent-heading = Отправленные приглашения
invites-badge-expired = истекло
invites-badge-used = использовано
invites-badge-active = активно
invites-sent-by = Отправитель: { $name }
invites-uses = Использовано: { $used } из { $max }
invites-expires-at = Действует до { $date }
invites-copy-link = Скопировать ссылку
invites-delete = Удалить
invites-delete-confirm = Удалить это приглашение?
invites-empty = Приглашений пока нет. Отправьте ссылку для записи с помощью формы выше.
invites-js-generating = Создаём…
invites-js-copied = Скопировано
invites-js-error = Ошибка

invites-sent-count =
    { $count ->
        [one] Отправлено { $count } приглашение.
        [few] Отправлено { $count } приглашения.
        [many] Отправлено { $count } приглашений.
       *[other] Отправлено { $count } приглашения.
    }

invites-skipped-invalid =
    { $count ->
        [one] Пропущена { $count } строка с ошибкой:
        [few] Пропущены { $count } строки с ошибками:
        [many] Пропущено { $count } строк с ошибками:
       *[other] Пропущены { $count } строки с ошибками:
    }

invites-skipped-duplicate =
    { $count ->
        [one] Пропущена { $count } повторяющаяся строка:
        [few] Пропущены { $count } повторяющиеся строки:
        [many] Пропущено { $count } повторяющихся строк:
       *[other] Пропущены { $count } повторяющиеся строки:
    }

invites-failed =
    { $count ->
        [one] Не удалось отправить { $count } приглашение (БД или SMTP):
        [few] Не удалось отправить { $count } приглашения (БД или SMTP):
        [many] Не удалось отправить { $count } приглашений (БД или SMTP):
       *[other] Не удалось отправить { $count } приглашения (БД или SMTP):
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Изменение источника календарей
source-form-title-add = Добавление календаря
source-form-heading-edit = Изменение источника календарей
source-form-heading-add = Подключение календаря
source-form-subtitle-edit = Измените параметры подключения. Чтобы оставить текущий пароль, не заполняйте поле пароля. Если вы изменили адрес или имя пользователя, запустите синхронизацию, чтобы обновить список календарей.
source-form-subtitle-add = Подключите сервер CalDAV или Microsoft Exchange (EWS), чтобы calrs учитывал вашу занятость, когда гости записываются на встречи.
source-form-server-title = Сервер CalDAV или Exchange
source-form-backend = Протокол
source-form-preset = Шаблон
source-form-connect-google = Подключить через Google
source-form-google-unavailable = Google Calendar недоступен. Обратитесь к администратору.
source-form-name = Отображаемое название
source-form-name-placeholder = Мой календарь
source-form-url-caldav = Адрес CalDAV
source-form-url-ews = Адрес EWS
source-form-username = Имя пользователя
source-form-password = Пароль
source-form-password-keep = Оставьте пустым, чтобы не менять
source-form-password-placeholder = Пароль приложения или аккаунта
source-form-skip-test = Пропустить проверку подключения
source-form-skip-test-help = Отметьте, если проверка зависает (так бывает с некоторыми серверами BlueMind и Zimbra). Проверить подключение можно позже.
source-form-save = Сохранить изменения
source-form-add = Добавить источник
source-form-help-google-configured = Нажмите кнопку ниже, чтобы разрешить calrs доступ к вашему Google Calendar.
source-form-help-google-unconfigured = Интеграция с Google Calendar ещё не настроена. Попросите администратора указать учётные данные Google OAuth2 в панели администратора.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — укажите DAV-адрес вашего сервера BlueMind.<br> Обычно это <code>https://mail.yourcompany.com/dav/</code><br> Имя пользователя — ваш <strong>адрес эл. почты</strong> (например, <code>alice@yourcompany.com</code>), а не только имя для входа.<br> Если проверка подключения зависает, отметьте «Пропустить проверку подключения» и сразу запустите синхронизацию.
source-form-help-nextcloud = <strong>Nextcloud</strong> — укажите корневой адрес WebDAV, а не ссылку на отдельный календарь.<br> Обычно это <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — укажите в пути URL свой полный адрес эл. почты.<br> Пример: <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Используйте пароль приложения (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — укажите <code>https://caldav.icloud.com/</code><br> Нужен пароль приложения, его можно создать на <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Безопасность &rarr; Пароли приложений).
source-form-help-zimbra = <strong>Zimbra</strong> — укажите DAV-адрес вашего сервера Zimbra.<br> Обычно это <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — укажите DAV-адрес SOGo.<br> Обычно это <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — укажите корневой адрес сервера.<br> Обычно это <code>https://cal.example.com/</code>
source-form-help-baikal = <strong>Baikal</strong> — укажите DAV-адрес вашего сервера Baikal.<br> Обычно это <code>https://cal.example.com/dav.php/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong> — укажите адрес SOAP:<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> Имя пользователя — адрес почтового ящика; пароль должен работать с HTTP Basic через TLS (если в вашей организации Basic отключён, включите его для служебного почтового ящика).<br> Не забудьте выбрать <strong>Microsoft Exchange (EWS)</strong> в списке «Протокол» выше.
source-form-help-google = <strong>Google Calendar</strong> — подключение через OAuth2, пароль не нужен.<br>
source-form-help-other = Укажите <strong>корневой DAV-адрес</strong> вашего сервера CalDAV, а не адрес отдельного календаря или публичную ссылку.<br> calrs сам найдёт ваши календари через PROPFIND (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Жирный
editor-italic-short = Курсив
editor-link-short = Вставить ссылку

# Team creation (templates/team_form.html)

team-form-heading = Новая команда
team-form-name = Название команды
team-form-name-placeholder = Разработка
team-form-slug = Короткий адрес
team-form-slug-hint = (часть ссылки: латиница, цифры, дефис)
team-form-slug-pattern-title = Только строчные латинские буквы, цифры и дефисы
team-form-description = Описание
team-form-optional = (необязательно)
team-form-description-placeholder = Чем занимается эта команда…
team-form-description-help = Показывается на странице команды. Можно использовать **жирный**, *курсив* и [ссылки](url).
team-form-visibility = Видимость
team-form-public = Публичная
team-form-private = Закрытая
team-form-visibility-help = Для закрытой команды создаётся ссылка-приглашение, которой можно поделиться. Публичные команды видны на странице профиля команды.
team-form-members = Участники
team-form-members-help = Вы автоматически станете администратором команды. Добавьте пользователей или привяжите группы OIDC.
team-form-search-placeholder = Найти пользователей или группы…
team-form-search-users = Пользователи
team-form-search-groups = Группы OIDC
team-form-you = (вы)
team-form-submit = Создать команду

# Team settings (templates/team_settings.html)

team-settings-page-title = Настройки
team-settings-subtitle = Настройки команды: их могут менять администраторы команды.
team-settings-public-url = Публичный адрес
team-settings-public-url-help = Записаться по этой ссылке может любой.
team-settings-invite-link = Ссылка-приглашение
team-settings-invite-link-help = Поделитесь этой ссылкой, чтобы открыть доступ к странице записи этой закрытой команды.
team-settings-avatar = Изображение команды
team-settings-profile = Профиль
team-settings-description-placeholder = Расскажите об этой команде…
team-settings-description-help = Показывается на публичной странице записи команды. Можно использовать **жирный**, *курсив* и [ссылки](url).
team-settings-visibility-help = Публичные команды видны на странице профиля команды. На страницу закрытой команды можно попасть только по ссылке-приглашению.
team-settings-members-help = Управляйте составом команды. Добавьте пользователей или привяжите группы OIDC для автоматической синхронизации.
team-settings-time-off-help = Запланированное отсутствие участников. Время указано в вашем часовом поясе ({ $timezone }). Заметки участников видны только им.
team-settings-role-member = Участник
team-settings-role-admin = Администратор
team-settings-oidc-group = Группа OIDC
team-settings-remove = Убрать
team-settings-save = Сохранить изменения
team-settings-danger-zone = Опасная зона
team-settings-danger-help = Команда будет удалена навсегда. Типы встреч отвяжутся от неё, но не удалятся. Это действие нельзя отменить.
team-settings-delete = Удалить эту команду
team-settings-delete-confirm = Удалить команду «{ $name }»? Это действие нельзя отменить.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Изменение типа встречи
etf-heading-new = Новый тип встречи
etf-disabled-banner = Этот тип встречи выключен, поэтому записаться на него пока нельзя.
etf-team = Команда
etf-team-hint = (необязательно — оставьте пустым, если тип встречи личный)
etf-team-personal = Личный
etf-scheduling-mode = Режим распределения
etf-mode-round-robin = По очереди — встреча назначается одному свободному участнику
etf-mode-collective = Совместная — нужны все участники
etf-scheduling-mode-help = В режиме «По очереди» встреча назначается одному свободному участнику (сначала наименее занятому). В режиме «Совместная» все участники должны быть свободны в одно и то же время.
etf-title = Название
etf-title-placeholder = Знакомство, 30 минут
etf-slug = Короткий адрес
etf-slug-placeholder = создаётся автоматически из названия
etf-description-placeholder = Короткий звонок, чтобы познакомиться и обсудить…
etf-description-help = Показывается на странице записи. Можно использовать **жирный**, *курсив* и [ссылки](url).
etf-location = Место
etf-location-link = Видеозвонок (постоянная ссылка)
etf-location-jitsi = Jitsi (комната создаётся автоматически)
etf-location-webhook = Вебхук (свой сервис)
etf-location-phone = Телефон
etf-location-in-person = Лично
etf-location-custom = Другое
etf-location-details = Подробности
etf-location-details-placeholder = https://meet.example.com/my-room
etf-pattern-placeholder = Оставьте пустым, чтобы использовать шаблон организации по умолчанию
etf-duration = Длительность (мин)
etf-slot-interval = Шаг времени (мин)
etf-slot-interval-placeholder = Равен длительности
etf-slot-interval-help = С каким шагом предлагать время. Если не заполнено, шаг равен длительности.
etf-required-members = Обязательные участники
etf-required-members-help = Время предлагается, только если все отмеченные участники свободны. Снимите отметку с тех, кого нужно исключить (их доступность не будет учитываться).
etf-member-priority = Приоритет участников
etf-member-priority-help = Новые записи в первую очередь получают свободные участники с более высоким приоритетом. При равном приоритете запись получает тот, у кого меньше недавних записей.
etf-member-timezone-title = Часовой пояс участника: в нём задаются его личные рабочие часы.
etf-priority-high = Высокий
etf-priority-medium = Средний
etf-priority-low = Низкий
etf-section-availability = Доступность
etf-timezone-help = Часы ниже указаны в этом часовом поясе. Для командных типов встреч выберите рабочий часовой пояс команды (он может не совпадать с вашим).
etf-reset-default = Вернуть мою доступность по умолчанию
etf-reset-default-title = Заменить эти часы доступностью по умолчанию из вашего профиля
etf-availability-prefilled = Часы взяты из вашей { $link }. Здесь их можно изменить для этого типа встречи.
etf-availability-prefilled-link = доступности по умолчанию
etf-section-buffers = Перерывы и сроки
etf-buffer-before = Перерыв до встречи (мин)
etf-buffer-after = Перерыв после встречи (мин)
etf-min-notice = Минимальный срок записи
etf-min-notice-help = Минимальное время между записью и началом встречи.
etf-section-limits = Ограничения записи
etf-first-slot-only = Одно время в день
etf-first-slot-only-help = Показывать только самое раннее свободное время каждого дня.
etf-freq-limit = Ограничить частоту записи
etf-freq-limit-help = Сколько раз можно записаться на этот тип встречи за период.
etf-add-limit = Добавить ограничение
etf-section-options = Параметры записи
etf-requires-confirmation = Нужно подтверждение
etf-requires-confirmation-help = Новые записи будут ждать, пока вы не подтвердите их в кабинете.
etf-sms = SMS-уведомления
etf-sms-off = Выключены, номер не запрашивается
etf-sms-optional = Номер необязателен, гость может его оставить
etf-sms-required = Номер обязателен, гость должен его оставить
etf-sms-help = Помимо писем, гость получает SMS, когда запись подтверждена, перенесена или отменена, а также перед началом встречи. Если гость не укажет номер, SMS отправляться не будут. Нужен SMS-шлюз, настроенный в { $link }.
etf-admin-panel-link = панели администратора
etf-additional-guests = Дополнительные гости
etf-guests-none = Гости не могут приглашать других
etf-additional-guests-help = Гость сможет пригласить дополнительных участников встречи — они тоже получат приглашение в календарь.
etf-default-view = Вид календаря по умолчанию
etf-view-month = Месяц — сетка календаря со списком свободного времени
etf-view-week = Неделя — 7 колонок по дням со свободным временем
etf-view-column = Список — дни друг под другом со свободным временем
etf-view-week-short = неделя
etf-view-column-short = список
etf-default-view-help = Так гости увидят календарь при открытии страницы. Сменить вид можно в любой момент.
etf-conflict-calendars = Календари для проверки занятости
etf-conflict-calendars-help = Выберите календари, по которым проверяется занятость. Если ничего не выбрано, проверяются ваши собственные календари, а календари, которыми с вами поделились коллеги, — нет. Отметьте все календари, которые должны закрывать время, включая общие.
etf-no-resources = Общих ресурсов пока нет. Добавьте ресурс (демолабораторию, переговорную) в { $link }, чтобы привязать его к этому типу встречи.
etf-section-access = Доступ и уведомления
etf-visibility-public = Публичный — виден в вашем профиле
etf-visibility-internal = Внутренний — любой коллега может создавать ссылки-приглашения
etf-visibility-private = Закрытый — только по ссылке-приглашению
etf-visibility-help = Определяет, кто может видеть этот тип встречи и записываться на него.
etf-vis-internal = Внутренний
etf-reminder = Напоминание о встрече
etf-reminder-none = Без напоминания
etf-reminder-help = Перед встречей вам и гостю придёт письмо-напоминание.
etf-guest-footer = Подпись в письмах гостям
etf-guest-footer-placeholder = например: Вопросы? Телефон: +7 495 123-45-67
etf-guest-footer-help = Добавляется в конец каждого письма, которое гость получает об этой записи, независимо от языка. Заменяет подпись по умолчанию, заданную администратором. Доступны только жирный шрифт, курсив и ссылки.
etf-dynamic-group = Групповая ссылка
etf-dynamic-group-help = Создайте ссылку для разовой встречи с проверкой доступности — вашей и других пользователей.
etf-dynamic-group-search = Найти пользователя…
etf-dynamic-group-note = Показаны только пользователи, которые разрешили добавлять себя в групповые ссылки.
etf-dynamic-group-url = Адрес групповой ссылки
etf-watcher-teams = Команды-наблюдатели
etf-watcher-teams-help = Выбранные команды получают уведомления о новых записях. Их участники могут взять запись и присоединиться к встрече.
etf-watcher-team-locked = Это не ваша команда, поэтому её нельзя отвязать
etf-watcher-team-private = Закрытая команда
etf-save = Сохранить изменения
etf-create = Создать тип встречи
etf-js-loading = Загрузка…
etf-js-no-default = Доступность по умолчанию не задана
etf-js-reset-done = Сброшено
etf-js-error = Ошибка
etf-js-remove-limit = Убрать ограничение
etf-period-day = В день
etf-period-week = В неделю
etf-period-month = В месяц
etf-period-year = В год

# Event type form: runtime summary hints (templates/event_type_form.html)

# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = Дни не выбраны
etf-hint-every-day = Каждый день
etf-fmt-day-one = %1 день
etf-fmt-day-other = %1 дн.
etf-fmt-hours = %1 ч
etf-fmt-minutes = %1 мин
etf-hint-buffer-both = перерыв %1 мин до и %2 мин после
etf-hint-buffer-before = перерыв %1 мин до встречи
etf-hint-buffer-after = перерыв %1 мин после встречи
etf-hint-notice = мин. срок: %1
etf-hint-no-buffers = Без перерывов, записаться можно в любой момент
etf-hint-max = Не более %1
etf-hint-period-day = /день
etf-hint-period-week = /нед.
etf-hint-period-month = /мес.
etf-hint-period-year = /год
etf-hint-no-limits = Без ограничений
etf-hint-confirmation-required = Нужно подтверждение
etf-hint-auto-confirmed = Подтверждается автоматически
etf-hint-extra-guests-one = до %1 доп. гостя
etf-hint-extra-guests-other = доп. гостей: до %1
etf-hint-view = вид: %1
etf-hint-reminder = напоминание за %1
etf-hint-no-reminder = без напоминания

etf-guests-up-to =
    { $count ->
        [one] До { $count } дополнительного гостя
        [few] До { $count } дополнительных гостей
        [many] До { $count } дополнительных гостей
       *[other] До { $count } дополнительных гостей
    }

etf-reminder-hours =
    { $count ->
        [one] За { $count } час
        [few] За { $count } часа
        [many] За { $count } часов
       *[other] За { $count } часа
    }

etf-reminder-days =
    { $count ->
        [one] За { $count } день
        [few] За { $count } дня
        [many] За { $count } дней
       *[other] За { $count } дня
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Создаётся <strong>публичный</strong> тип встречи &mdash; записаться может любой, у кого есть ссылка.
etf-preset-private = Создаётся <strong>закрытый</strong> тип встречи &mdash; записаться могут только те, кого вы пригласили.
etf-preset-internal = Создаётся <strong>внутренний</strong> тип встречи &mdash; ссылкой для записи может поделиться любой коллега.
etf-preset-team = Создаётся <strong>командный</strong> тип встречи &mdash; записи распределяются между участниками команды.
etf-pattern-hint = Необязательный шаблон вместо стандартного. Переменные: <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Оставьте пустым, чтобы использовать шаблон организации, заданный администратором.
etf-pattern-random-warning = В этом шаблоне нет переменной <code>{"{"}random{"}"}</code>. Две записи этого типа встречи в один день попадут в одну и ту же комнату, и второй гость сможет зайти на встречу первого. Используйте постоянные комнаты, только если вам нужно именно это.
etf-webhook-hint = Ссылка на встречу для каждой записи запрашивается у вебхука, который администратор настроил в разделе «Панель администратора &rarr; Вебхук для встреч». Указывать адрес здесь не нужно.

# Admin panel (templates/admin.html)

admin-page-title = Панель администратора
admin-action-refused = Действие не выполнено:
admin-error-last-admin = Это последний активный администратор. Сначала сделайте администратором другого пользователя или разблокируйте другого администратора.
admin-logo = Логотип компании
admin-logo-help = Показывается на публичных страницах записи. PNG, JPEG, GIF или WebP, не более 2 МБ.
admin-logo-light = Светлая тема
admin-logo-dark = Тёмная тема
admin-logo-dark-help = Необязательно. Показывается посетителям с тёмной темой. Если тёмного логотипа нет, в обеих темах используется светлый.
admin-logo-refused = Логотип не сохранён. Загрузите изображение PNG, JPEG, GIF или WebP размером не более 2 МБ.
admin-favicon = Значок сайта
admin-favicon-help = Показывается во вкладках браузера и в закладках. Рекомендуется квадратный PNG размером от 32×32 пикселей и не более 512 КБ. Если значка нет, показывается краб calrs.
admin-favicon-refused = Значок сайта не сохранён. Загрузите изображение PNG, JPEG, GIF или WebP размером не более 512 КБ.
admin-upload-save-failed = Не удалось сохранить файл в каталоге данных сервера. Подробности — в журнале сервера.

# User and team avatar uploads (#307)
avatar-upload-refused = Изображение не сохранено. Загрузите файл PNG, JPEG, GIF или WebP размером не более 2 МБ.
avatar-upload-save-failed = Не удалось сохранить изображение на сервере. Попробуйте позже или попросите администратора проверить журнал сервера.
admin-company-link = Сайт компании
admin-company-link-help = Логотип на публичных страницах записи ведёт на этот адрес. Оставьте пустым, чтобы логотип не был ссылкой.
admin-theme = Тема
admin-theme-help = Выберите цветовую тему для всех страниц. Светлый и тёмный режим переключаются отдельно: каждая тема поддерживает оба режима.
admin-theme-default = По умолчанию
admin-theme-default-desc = Чистый синий
admin-theme-nord-desc = Арктический иней
admin-theme-dracula-desc = Тёмный фиолетовый
admin-theme-gruvbox-desc = Тёплое ретро
admin-theme-solarized-desc = Классика от Итана
admin-theme-tokyo-desc = Неоновый город
admin-theme-custom = Своя
admin-theme-custom-desc = Ваши цвета
admin-custom-colors = Свои цвета
admin-color-accent = Акцентный цвет
admin-color-accent-hover = Акцентный цвет при наведении
admin-color-bg = Фон
admin-color-surface = Фон карточек
admin-color-text = Текст
admin-save-theme = Сохранить тему
admin-users = Пользователи
admin-user-filter = Фильтр по имени или эл. почте…
admin-badge-admin = администратор
admin-badge-disabled = заблокирован
admin-impersonate = Войти от имени
admin-demote = Снять права администратора
admin-promote = Сделать администратором
admin-disable = Заблокировать
admin-enable = Разблокировать
admin-delete = Удалить
admin-no-users-match = Пользователи по фильтру не найдены.
admin-no-users = Пользователей пока нет.
admin-groups = Группы
admin-group-filter = Фильтр по названию группы…
admin-group-name = Название группы
admin-weight = вес:
admin-no-groups-match = Группы по фильтру не найдены.
admin-no-groups = Группы пока не синхронизированы. Они загружаются автоматически из вашего поставщика удостоверений (OIDC).
admin-auth-settings = Настройки входа
admin-registration-enabled = Разрешить регистрацию
admin-allowed-domains = Разрешённые домены эл. почты
admin-allowed-domains-hint = (через запятую; оставьте пустым, чтобы разрешить любые)
admin-save-auth = Сохранить настройки входа
admin-system-settings = Системные настройки
admin-base-url = Публичный адрес
admin-base-url-help = Публичный адрес этого сервера. Используется для перенаправлений OIDC и ссылок в письмах (подтверждение и отклонение, отмена, напоминания).
admin-private-hosts = Разрешённые внутренние хосты
admin-private-hosts-help = Хосты (через запятую), которым разрешено указывать на частные или зарезервированные IP-адреса в источниках CalDAV/EWS (исключение из защиты от SSRF). Добавляйте только хосты, которыми управляете сами (например, сервер календарей в той же сети Docker). Оставьте пустым, чтобы защита действовала для всех хостов.
admin-trusted-proxies = Доверенные прокси
admin-trusted-proxies-help = Адреса или диапазоны CIDR обратных прокси, чьему заголовку X-Forwarded-For можно доверять. По нему различаются клиенты при ограничении частоты запросов. Оставьте пустым для значения по умолчанию (loopback, частные, уникальные локальные и link-local диапазоны: прокси на том же хосте или в той же сети Docker). Введите none, если к calrs обращаются напрямую, — тогда клиент не сможет подменить свой адрес.
admin-trusted-proxies-everyone = Доверенные прокси: значение «{ $entry }» означает доверие любому адресу — так любой клиент сможет выдать себя за кого угодно. Укажите адрес или диапазон вашего прокси.
admin-trusted-proxies-invalid = Доверенные прокси: «{ $entry }» — не IP-адрес и не диапазон CIDR.
admin-german-formal = Обращение на немецком
admin-german-formal-du = Неформальное (du)
admin-german-formal-sie = Формальное (Sie)
admin-german-formal-help = Как страницы и письма обращаются к посетителям, чей браузер запрашивает немецкий язык. У пользователей, выбравших язык в разделе «Профиль и настройки», остаётся их выбор.
admin-unset-env = Чтобы изменить это значение здесь, удалите переменную окружения.
admin-save-system = Сохранить системные настройки
admin-status = Статус:
admin-status-enabled = включено
admin-status-disabled = выключено
admin-status-disabled-paren = (выключено)
admin-status-configured = настроено
admin-status-not-configured = не настроено
admin-via-environment = (задано в переменных окружения)
admin-issuer = Издатель:
admin-client-id = ID клиента:
admin-instance = Сервер:
admin-oidc-settings = Настройки OIDC
admin-oidc-enabled = Включить OIDC
admin-issuer-url = Адрес издателя (Issuer URL)
admin-client-id-label = ID клиента
admin-client-secret = Секретный ключ клиента
admin-keep-current-hint = (оставьте пустым, чтобы не менять)
admin-keep-current-set-hint = (оставьте пустым, чтобы не менять; сейчас задан)
admin-keep-unchanged = Оставьте пустым, чтобы не менять
admin-oidc-auto-register = Автоматически регистрировать новых пользователей при входе через OIDC
admin-save-oidc = Сохранить настройки OIDC
admin-google = Google Calendar (OAuth2)
admin-save-google = Сохранить настройки Google OAuth2
admin-captcha = Капча
admin-instance-url = Адрес сервера
admin-site-key = Ключ сайта
admin-secret = Секретный ключ
admin-widget-url = Адрес скрипта виджета
admin-widget-url-help = Укажите свой адрес, если CDN заблокирован. Изменения вступают в силу сразу после сохранения.
admin-wasm-url = Адрес WASM-файла виджета
admin-wasm-url-help = Адрес, с которого виджет загружает решатель. Укажите WASM-файл, который отдаёт ваш сервер Cap, чтобы страницы записи не обращались к cdn.jsdelivr.net. Оставьте пустым для значения по умолчанию.
admin-captcha-wasm-url-invalid = Адрес WASM-файла виджета должен быть полным и начинаться с http:// или https://.
admin-captcha-disable-help = Оставьте адрес сервера, ключ сайта и секретный ключ пустыми, чтобы выключить капчу на страницах записи.
admin-save-captcha = Сохранить настройки капчи
admin-resources = Ресурсы
admin-resources-help = Общие ресурсы для бронирования (демолаборатория, переговорные), занятость которых берётся из календаря по ICS-ссылке. Если ресурс привязан к типу встречи и занят, на это время записаться нельзя.
admin-resource-stats = Событий в кэше: { $events } &middot; Привязан к типам встреч: { $attached }
admin-never = никогда
admin-resource-sync-failed = (последняя попытка не удалась: { $error })
admin-writeback-enabled = Добавление в календарь: включено ({ $via })
admin-writeback-readonly = Добавление в календарь: нет, только для чтения
admin-teams-allowed = Доступен командам:
admin-teams-allowed-none = нет (только администраторы сервера)
admin-sync-now = Синхронизировать
admin-test-write = Проверить добавление
admin-delete-resource-confirm = Удалить этот ресурс? Типы встреч, к которым он привязан, перестанут проверять его занятость.
admin-name = Название
admin-name-help = Оставьте пустым, чтобы взять название из ICS-ссылки.
admin-feed-url = ICS-ссылка (адрес публикации)
admin-feed-url-help = BlueMind: публичный или закрытый адрес календаря ресурса.
admin-caldav-url = Адрес коллекции CalDAV (для добавления в календарь)
admin-caldav-url-help = Необязательно. Для BlueMind определяется автоматически по ICS-ссылке.
admin-caldav-username = Имя пользователя CalDAV
admin-caldav-password = Пароль CalDAV
admin-resource-teams = Команды, которым доступен ресурс
admin-resource-teams-help = Администраторы этих команд могут привязывать ресурс к типам встреч своей команды. Пусто — только администраторы сервера.
admin-no-teams = Команд пока нет.
admin-save-resource = Сохранить ресурс
admin-add-resource = Добавить ресурс
admin-jitsi = Jitsi (ссылки на встречи создаются автоматически)
admin-jitsi-help = Если в типе встречи выбрано место «Jitsi (комната создаётся автоматически)», calrs создаёт для каждой записи новую комнату: добавляет шаблон ниже к базовому адресу Jitsi. Обращаться к внешнему API не нужно.
admin-display-name = Отображаемое название
admin-jitsi-display-name-placeholder = например, Meet DYB
admin-jitsi-display-name-help = Показывается гостям при выборе времени и в форме записи. Если не указано, показывается «Видеозвонок».
admin-room-pattern = Шаблон названия комнаты
admin-jitsi-disable-help = Оставьте базовый адрес пустым, чтобы выключить автоматическое создание комнат Jitsi.
admin-save-jitsi = Сохранить настройки Jitsi
admin-meeting-webhook = Вебхук для встреч (свой сервис видеосвязи)
admin-webhook-url = Адрес вебхука
admin-webhook-display-name-placeholder = например, Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Показывается гостям вместо общей метки «Видеозвонок».
admin-authentication = Аутентификация
admin-auth-none = Нет
admin-auth-hmac = HMAC-SHA256 (заголовок X-Calrs-Signature)
admin-shared-secret = Общий секретный ключ
admin-webhook-disable-help = Оставьте адрес пустым, чтобы выключить вебхук для встреч.
admin-save-webhook = Сохранить настройки вебхука
admin-smtp = Настройки SMTP
admin-smtp-test-sent = Тестовое письмо отправлено.
admin-smtp-test-failed = Не удалось отправить тестовое письмо. Проверьте журнал сервера и настройки SMTP.
admin-smtp-env-error = Ошибка в настройках SMTP из переменных окружения:
admin-smtp-host = Сервер:
admin-smtp-from = Отправитель:
admin-smtp-enabled = SMTP включён
admin-host = Сервер
admin-port = Порт
admin-tls-mode = Режим TLS
admin-tls-starttls = STARTTLS (порт 587)
admin-tls-implicit = Неявный TLS (порт 465)
admin-tls-none = Без шифрования (только для локального MTA)
admin-smtp-username-hint = (оставьте пустым, если сервер не требует аутентификации)
admin-from-email = Адрес отправителя
admin-from-name = Имя отправителя
admin-save-smtp = Сохранить настройки SMTP
admin-send-test-email = Отправить тестовое письмо на адрес
admin-send-test-email-hint = (по умолчанию — адрес эл. почты вашего аккаунта)
admin-send-test-email-btn = Отправить тестовое письмо
admin-smtp-clear-confirm = Удалить настройки SMTP из базы данных?
admin-clear-db-config = Удалить настройки из базы данных
admin-sms = Настройки SMS
admin-sms-help = Необязательно. SMS отправляются только по записям на типы встреч, где включены «SMS-уведомления», и только если гость указал номер телефона.
admin-sms-test-sent = Тестовое сообщение отправлено.
admin-sms-test-checked = Учётные данные приняты.
admin-sms-test-error = SMS-шлюз отклонил запрос.
admin-sms-captcha-warning = Форма записи публичная, а номер получателя указывает гость, поэтому без капчи кто угодно сможет рассылать SMS за ваш счёт. Настройте капчу выше и ограничьте страны получателей в настройках самого шлюза.
admin-sms-sent-today = Отправлено сегодня:
admin-sms-segments-today = Оплачиваемых сегментов сегодня:
admin-sms-unknown-today = Без ответа от шлюза (учтены в лимите):
admin-sms-of-cap = из { $cap }
admin-sms-config-error = Ошибка в настройках SMS:
admin-sms-gateway = Шлюз:
admin-sms-account = Аккаунт:
admin-sms-sender = Отправитель:
admin-sms-enabled = SMS включены
admin-sms-gateway-label = Шлюз
admin-required-on-switch = Обязательно при смене шлюза
admin-sms-docs = Документация API { $provider }
admin-sms-country = Код страны по умолчанию
admin-sms-country-hint = (используется, когда гость вводит местный номер телефона)
admin-sms-daily-cap = Дневной лимит
admin-sms-daily-cap-hint = (оплачиваемые сегменты SMS за сутки по UTC для всего сервера, 0 — без ограничений)
admin-sms-daily-cap-help = Сообщение до 160 символов — это один сегмент, а если в нём есть символы вне алфавита GSM (например, кириллица), — до 70. Более длинное сообщение оплачивается по сегменту за каждые 153 символа (или 67). Тестовые сообщения тоже учитываются. После достижения лимита calrs перестаёт отправлять SMS, но продолжает отправлять письма, так что записи не срываются из-за исчерпанного бюджета на SMS.
admin-save-sms = Сохранить настройки SMS
admin-send-test-sms = Отправить тестовое сообщение на номер
admin-send-test-sms-hint-check = (оставьте пустым, чтобы только проверить учётные данные)
admin-send-test-sms-hint-e164 = (в формате E.164)
admin-test-gateway = Проверить шлюз
admin-sms-clear-confirm = Удалить настройки SMS из базы данных?
admin-sms-allow-all = Разрешить всем пользователям включать SMS в своих типах встреч
admin-sms-allow-all-help = По умолчанию выключено: SMS расходуют баланс аккаунта, настроенного здесь, поэтому включать SMS в типе встречи могут только администраторы.
admin-save-policy = Сохранить правило
admin-guest-footer = Подпись в письмах гостям
admin-guest-footer-help = Короткий текст в конце каждого письма гостям и участникам встречи (подтверждения, напоминания, отмены и т. д.). Письма организатору и участнику команды, который взял запись, не меняются.
admin-guest-footer-default = Подпись по умолчанию
admin-guest-footer-default-help = Используется, если у типа встречи нет своей подписи. Текст одинаков для всех языков. Можно использовать только жирный шрифт, курсив и ссылки.
admin-guest-footer-allow-all = Разрешить всем пользователям задавать подпись в своих типах встреч
admin-guest-footer-allow-all-help = По умолчанию выключено: подпись отправляется от имени этого сервера, поэтому задавать её могут только администраторы. Подпись в типе встречи заменяет подпись по умолчанию.
admin-save-guest-footer = Сохранить подпись

# Admin panel: overview, section tabs, users table (templates/admin.html)

admin-copy = Скопировать
admin-copied = Скопировано
admin-sections = Разделы панели администратора
admin-tab-overview = Обзор
admin-tab-users = Пользователи
admin-tab-access = Вход и безопасность
admin-tab-notifications = Почта и SMS
admin-tab-meetings = Ссылки на встречи
admin-tab-resources = Ресурсы
admin-tab-branding = Оформление
admin-tab-system = Система
admin-att-title = Требует внимания
admin-mast-users = { $count ->
    [one] { $count } пользователь
    [few] { $count } пользователя
    [many] { $count } пользователей
   *[other] { $count } пользователя
}
admin-mast-bookings = { $count ->
    [one] { $count } запись
    [few] { $count } записи
    [many] { $count } записей
   *[other] { $count } записи
}
admin-mast-attention = { $count ->
    [one] { $count } пункт требует внимания.
    [few] { $count } пункта требуют внимания.
    [many] { $count } пунктов требуют внимания.
   *[other] { $count } пункта требуют внимания.
}
admin-mast-clear = Всё в порядке.
admin-sev-failing = Ошибка
admin-sev-missing = Не настроено
admin-sev-off = Выключено
admin-sev-exposed = Уязвимость
admin-sev-expiring = Истекает
admin-sev-stale = Устарело
admin-fig-title = Ключевые показатели
admin-chart-week-short = Нед. { $week }
admin-chart-now = сейчас
admin-chart-ended-key = Отменённые или отклонённые (число под каждой неделей)
admin-pct = { $value }%
admin-att-review = Посмотреть
admin-att-smtp-error = Настройки SMTP из переменных окружения неверны: письма не отправляются.
admin-att-smtp-missing = Почта не настроена: гости и организаторы не получают писем с подтверждениями, напоминаниями и отменами.
admin-att-smtp-disabled = Отправка писем выключена.
admin-att-sources-failing = Источники календарей с ошибками синхронизации
admin-att-writeback = Предстоящие записи, которых нет в календаре организатора
admin-att-sms-captcha = SMS настроены без капчи: через публичную форму записи можно рассылать платные сообщения.
admin-att-resources-failing = Ресурсы с ошибками синхронизации
admin-att-pending-expiring = Запросы, срок которых истекает в ближайшие 24 часа
admin-att-sources-stale = Источники календарей, не синхронизированные больше 24 часов
admin-att-base-url = Публичный адрес сервера не задан: ссылки в письмах и перенаправление при входе через SSO могут не работать.
admin-kpi-bookings = Записи за 30 дней
admin-kpi-prev = Предыдущие 30 дней: { $count }
admin-kpi-upcoming = Предстоящие встречи
admin-kpi-upcoming-week = Ближайшие 7 дней: { $count }
admin-kpi-pending = Запросы на запись
admin-kpi-pending-expiring = Истекают в ближайшие 24 часа: { $count }
admin-kpi-pending-none = Нет запросов, ожидающих подтверждения
admin-kpi-pending-ok = В ближайшие 24 часа ничего не истекает
admin-kpi-active = Активные пользователи
admin-kpi-active-sub = Входили за 30 дней (из всех незаблокированных аккаунтов)
admin-chart-weekly = Записи по неделям
admin-chart-weekly-help = Записи, созданные за последние 12 недель, по их текущему статусу.
admin-series-confirmed = Подтверждённые
admin-series-pending = Ожидающие подтверждения
admin-series-ended = Отменённые или отклонённые
admin-chart-bar-label = Неделя с { $week } — подтверждено: { $confirmed }, ожидают: { $pending }, отменено или отклонено: { $ended }
admin-chart-table = Показать в виде таблицы
admin-chart-week = Неделя с
admin-chart-total = Всего
admin-chart-empty = За последние 12 недель записей нет.
admin-status-title = Записи по статусу
admin-status-empty = Записей пока нет.
admin-st-cancelled = Отменено
admin-st-declined = Отклонено
admin-svc-title = Сервисы
admin-svc-email = Эл. почта (SMTP)
admin-svc-sms = SMS
admin-svc-oidc = Единый вход (OIDC)
admin-svc-google = Google Calendar
admin-svc-microsoft = Microsoft 365 / Outlook.com
admin-svc-registration = Самостоятельная регистрация
admin-svc-jitsi = Ссылки на встречи Jitsi
admin-svc-webhook = Вебхук для встреч
admin-svc-base-url = Публичный адрес сервера
admin-state-error = ошибка
admin-top-event-types = Самые популярные типы встреч
admin-top-hosts = Самые загруженные организаторы
admin-top-help = Подтверждённые записи, созданные за последние 90 дней.
admin-top-empty = За последние 90 дней подтверждённых записей нет.
admin-top-team = командный
admin-health-title = Состояние календарей
admin-health-help = Состояние синхронизации всех источников календарей и записи, которые не удалось добавить в календарь организатора.
admin-health-healthy = Работают
admin-health-failing = С ошибками
admin-health-stale = Без синхронизации более 24 ч
admin-health-disabled = Выключены
admin-health-writeback-count = Не добавлено в календарь
admin-health-no-sources = Ни один источник календарей ещё не подключён.
admin-health-failing-title = Источники с ошибками синхронизации
admin-health-col-source = Источник
admin-health-col-owner = Владелец
admin-health-col-error = Последняя ошибка
admin-health-col-since = Начало сбоя
admin-health-attempts = Неудачных попыток: { $count }
admin-health-owner-hint = Доступность по-прежнему рассчитывается по последней успешной синхронизации. Владелец видит ошибку на странице «Источники календарей» и получает письмо, если ошибка сохраняется.
admin-health-writeback-title = Записи, которых нет в календаре организатора
admin-health-col-booking = Запись
admin-health-col-host = Организатор
admin-health-col-retry = Повтор
admin-health-retrying = попытки продолжаются
admin-health-retry-exhausted = попытки исчерпаны
admin-health-resources-title = Ресурсы с ошибками синхронизации
admin-inv-title = Сводка по серверу
admin-inv-users = Аккаунты пользователей
admin-inv-users-sub = Администраторов: { $admins }, заблокированных: { $disabled }
admin-inv-signin-sub = по паролю: { $local }, через SSO: { $sso }
admin-inv-new-users = Новые аккаунты за 30 дней
admin-inv-mfa-sub = Аккаунты с паролем, в которых включена 2FA
admin-inv-teams = Команды
admin-inv-groups-sub = Групп OIDC: { $count }
admin-inv-event-types = Типы встреч
admin-inv-event-types-sub = Включённых: { $enabled }, командных: { $team }
admin-inv-bookings = Записи
admin-inv-bookings-sub = Подтверждённых: { $count }
admin-inv-sources = Источники календарей
admin-inv-sources-sub = Включённых: { $count }
admin-inv-resources-failing = С ошибками синхронизации: { $count }
admin-inv-sms = SMS за 30 дней
admin-inv-sms-today = сегодня: { $count }
admin-users-filter-label = Фильтр пользователей
admin-filter-all = Все
admin-filter-admins = Администраторы
admin-filter-disabled = Заблокированные
admin-filter-never = Ни разу не входили
admin-provider-local = Пароль
admin-provider-oidc = SSO
admin-col-user = Пользователь
admin-col-role = Роль
admin-col-signin = Способ входа
admin-col-last = Последний вход
admin-col-created = Регистрация
admin-col-actions = Действия
admin-user-actions = Действия: { $name }
admin-role-user = пользователь
admin-badge-mfa = 2FA
admin-delete-confirm-btn = Удалить навсегда
admin-users-shown = Показано пользователей: %1 из %2

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Удалить пользователя { $email } навсегда?{"\u000A"}{"\u000A"}Будут удалены его профиль, аккаунт календаря, источники календарей, типы встреч и все данные, которые принадлежат только ему. Прошлые записи удаляются вместе с типами встреч.{"\u000A"}{"\u000A"}Для пользователей OIDC/SSO: если включена автоматическая регистрация, пользователь будет создан заново при следующем входе.{"\u000A"}{"\u000A"}Это действие нельзя отменить.
admin-system-settings-help = Публичный адрес и настройки сетевой безопасности. Их также можно задать переменными окружения <code>CALRS_BASE_URL</code> и <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Если переменная окружения задана, она <strong>имеет приоритет</strong> над значением ниже.
admin-set-by-env = задано переменной окружения ({ $var }) вместо сохранённого значения
admin-google-help = Чтобы подключить Google Calendar, создайте учётные данные OAuth2 в <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Включите <strong>CalDAV API</strong> (проверка занятости и добавление записей в календарь) и <strong>Google Calendar API</strong> (ссылки Google Meet), затем добавьте { $redirect_uri } в разрешённые URI перенаправления.
admin-room-pattern-help = Доступные подстановки: <code>{"{"}username{"}"}</code> (организатор), <code>{"{"}event{"}"}</code> (короткий адрес типа встречи), <code>{"{"}date{"}"}</code> (ГГГГММДД), <code>{"{"}random{"}"}</code> (8 символов). По умолчанию: { $default }.
admin-room-pattern-warning = Без <code>{"{"}random{"}"}</code> название комнаты предсказуемо: два гостя, записавшиеся на один тип встречи в один день, попадут в одну комнату и смогут видеть встречи друг друга. Постоянные комнаты допустимы (например, одна личная комната на организатора), но включайте этот режим, только если понимаете последствия.
admin-meeting-webhook-help = Если для типа встречи выбрано место «Вебхук (свой сервис)», при подтверждении записи calrs отправляет её данные POST-запросом на этот адрес и ожидает в ответ JSON <code>{"{"}"url": "https://..."{"}"}</code>.
admin-auth-hmac-help = С HMAC calrs подписывает исходное тело запроса и передаёт подпись в заголовке <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code>.
admin-tls-none-warning = Выбирайте <strong>Без шифрования</strong>, только если почтовый сервер работает на этой же машине и не поддерживает STARTTLS или использует самоподписанный сертификат. Письма и учётные данные передаются по сети открытым текстом.
admin-smtp-env-error-help = Исправьте переменные окружения <code>CALRS_SMTP_*</code> или удалите их, чтобы управлять SMTP здесь, через базу данных.
admin-smtp-env-managed = Управляется через <strong>переменные окружения</strong> (они имеют приоритет над базой данных). Чтобы изменить настройки, отредактируйте переменные <code>CALRS_SMTP_*</code>. Чтобы управлять SMTP здесь, удалите их.
admin-smtp-env-help = Настройки можно задать и переменными окружения (они имеют приоритет над этой формой): <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> или <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Обязательны только <code>CALRS_SMTP_HOST</code> и <code>CALRS_SMTP_FROM_EMAIL</code>. Чтобы отправлять письма через локальный MTA без аутентификации, не указывайте имя пользователя и пароль.
admin-sms-env-error-help = Исправьте переменные окружения <code>CALRS_SMS_*</code> или удалите их, чтобы управлять SMS здесь, через базу данных.
admin-sms-env-managed = Управляется через <strong>переменные окружения</strong> (они имеют приоритет над базой данных). Чтобы изменить настройки, отредактируйте переменные <code>CALRS_SMS_*</code>. Чтобы управлять SMS здесь, удалите их.
admin-sms-env-help = Настройки можно задать и переменными окружения (они имеют приоритет над этой формой): <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>Включён пробный режим Twilio</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Гости получают стандартный шаблон Twilio <code>sms_appointment_reminders</code>, а не настоящее сообщение, и доставка возможна только на номера, подтверждённые в вашей консоли Twilio. Этот режим нужен только для проверки на пробном аккаунте. Удалите переменную, прежде чем принимать записи.

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Выберите протокол, который поддерживает ваш сервер. EWS — для Exchange 2019/2016/2013 на собственных серверах.

admin-sms-going-live = <strong>Перед запуском:</strong> ограничьте страны назначения в настройках шлюза (в Twilio это Geo Permissions), используйте предоплаченный счёт без автопополнения и не выключайте капчу. Вместе эти три меры ограничивают, во сколько вам может обойтись атака с накруткой SMS-трафика.

troubleshoot-heading = Диагностика доступности

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = Укажите название и короткий адрес.
form-error-team-name-length = Название должно быть не длиннее 255 символов.
form-error-team-description-length = Описание должно быть не длиннее 5000 символов.
form-error-slug-charset = Короткий адрес может содержать только строчные латинские буквы, цифры и дефисы.
form-error-slug-reserved = Этот короткий адрес зарезервирован. Выберите другой.
form-error-team-slug-taken = Команда с таким коротким адресом уже существует.
form-error-title-required = Укажите название, чтобы создать короткий адрес.
form-error-event-type-slug-taken = Тип встречи с таким коротким адресом уже существует.
form-error-event-type-slug-taken-team = В этой команде уже есть тип встречи с таким коротким адресом.
form-error-event-type-title-required = Укажите название.
form-error-event-type-title-length = Название должно быть не длиннее 255 символов.
form-error-event-type-duration = Длительность должна быть целым числом минут от 1 до 1440 (24 часа).
form-error-event-type-number = Перерывы, минимальный срок, напоминание и число дополнительных гостей должны быть целыми числами не меньше 0.
form-error-event-type-horizon = Глубина записи должна быть целым числом дней не меньше 0. Оставьте поле пустым, чтобы не ограничивать её.
form-error-location-required = Укажите место (например, ссылку на видеозвонок, номер телефона или адрес).
form-error-bookable-range-order = Дата окончания не может быть раньше даты начала.
form-error-bookable-range-invalid = Введите даты в формате ГГГГ-ММ-ДД.
form-error-not-team-admin = Вы не администратор этой команды.
form-error-no-account = Аккаунт календаря не найден. Обратитесь к администратору.
form-error-all-fields-required = Заполните все поля.
form-error-encryption = Ошибка шифрования.
form-error-connection-failed = Не удалось подключиться: { $error }. Проверьте адрес и учётные данные или отметьте «Пропустить проверку подключения», чтобы всё равно сохранить.

# Settings page flash (src/web/mod.rs)

settings-saved = Настройки сохранены.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = Имя должно содержать от 1 до 255 символов.
settings-error-username-length = Имя пользователя должно содержать не менее 2 символов.
settings-error-username-taken = Это имя пользователя уже занято.
settings-error-booking-email = Введите корректный адрес для записей.
settings-error-save-failed = Не удалось сохранить настройки.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = Команда не найдена, или вы не администратор этой команды.
error-team-not-found = Команда не найдена.
error-event-type-not-found = Тип встречи не найден.
error-decrypt-failed = Не удалось расшифровать сохранённые учётные данные.
error-source-not-found = Источник не найден.
error-source-no-password = Для источника не сохранён пароль.
error-oauth-invalid-state = Неверный параметр state. Попробуйте ещё раз.
error-oauth-no-code = Код авторизации не получен.
error-oauth-not-configured = Google OAuth2 не настроен.
error-no-scheduling-account = Аккаунт календаря не найден.
error-private-event-type-not-found = Закрытый тип встречи не найден.
error-access-denied = Доступ запрещён.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = Это время уже занято.
error-slot-too-soon = Это время уже недоступно: до начала осталось слишком мало времени.
error-slot-beyond-horizon = Это время за пределами периода записи.
error-slot-not-yet-bookable = Запись на это время ещё не открыта.
error-invite-required = Для этого типа встречи нужна ссылка-приглашение.
error-invite-invalid = Недействительная ссылка-приглашение.
error-invite-expired = Срок действия этой ссылки-приглашения истёк.
error-invite-used = Эта ссылка-приглашение уже использована.
error-invalid-date = Неверная дата.
error-invalid-time = Неверное время.
error-invalid-date-format = Неверный формат даты.
error-invalid-time-format = Неверный формат времени.
error-too-many-bookings = Слишком много попыток записи. Попробуйте ещё раз через несколько минут.
error-too-many-requests = Слишком много запросов. Попробуйте позже.
error-no-members-available = На это время нет свободных участников команды.
error-dynamic-group-public-only = Групповые ссылки доступны только для публичных типов встреч.
error-user-not-found = Пользователь не найден.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Проверка капчи не пройдена
bae-title-invalid-booking = Неверные данные записи
bae-title-unavailable = Запись сейчас недоступна
bae-title-cannot-approve = Не удалось подтвердить запись
bae-title-invalid-link = Недействительная ссылка
bae-title-invalid-or-expired = Ссылка недействительна или устарела
bae-title-booking-not-found = Запись не найдена
bae-title-already-approved = Запись уже подтверждена
bae-title-already-declined = Запись уже отклонена
bae-title-already-cancelled = Запись уже отменена
bae-title-booking-cancelled = Запись отменена
bae-title-booking-declined = Запись отклонена

# Booking action error page: bodies

bae-body-go-back = Вернитесь на предыдущую страницу и попробуйте ещё раз.
bae-body-unavailable = Организатор больше не принимает записи на эту дату. Выберите другую дату или зайдите позже.
bae-body-resource-gone = Нужный ресурс на это время уже недоступен. Попросите гостя выбрать другое время.
bae-body-no-claim-token = Ссылка неполная: в ней нет токена.
bae-body-claim-invalid = Эта ссылка больше не действует: взять по ней запись нельзя.
bae-body-booking-gone = Этой записи больше нет.
bae-body-decline-link-invalid = Ссылка для отклонения недействительна, устарела или запись уже обработана.
bae-body-cancel-link-invalid = Ссылка для отмены недействительна, устарела или запись уже отменена.
bae-body-cancel-link-invalid-short = Ссылка для отмены недействительна или устарела.
bae-body-reschedule-link-invalid = Ссылка для переноса недействительна, устарела или запись уже обработана.
bae-body-approval-link-invalid = Ссылка для подтверждения недействительна или устарела.
bae-body-already-approved = Эта запись уже подтверждена.
bae-body-already-declined = Эта запись уже отклонена.
bae-body-already-cancelled = Эта запись уже отменена.
bae-body-was-cancelled = Эта запись была отменена.
bae-body-declined-by-host = Запись отклонена организатором.

# Booking form validation (src/web/mod.rs)

validate-name-length = Имя должно содержать от 1 до 255 символов.
validate-email-length = Адрес эл. почты должен содержать от 1 до 255 символов.
validate-email-invalid = Введите корректный адрес эл. почты.
validate-notes-length = Комментарий должен быть не длиннее 5000 символов.
validate-date-too-far = Записаться можно не более чем на год вперёд.
validate-guest-footer-too-long = Подпись в письмах гостям должна быть не длиннее 500 символов.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = Для этого типа встречи нельзя добавлять дополнительных гостей.
guests-too-many =
    { $max ->
        [one] Можно добавить не более { $max } дополнительного гостя.
        [few] Можно добавить не более { $max } дополнительных гостей.
        [many] Можно добавить не более { $max } дополнительных гостей.
       *[other] Можно добавить не более { $max } дополнительных гостей.
    }
guests-invalid-email = Неверный адрес дополнительного гостя: { $email }
dynamic-group-min-usernames = Для групповой ссылки нужны как минимум два имени пользователя.
dynamic-group-user-not-found = Пользователь «{ $username }» не найден.
dynamic-group-user-opted-out = У пользователя «{ $username }» выключены групповые ссылки.

error-slot-unavailable-member = Это время больше недоступно (занят участник: { $username }).

# Pending requests: slot hold, expiry and approval re-check (#234)

sms-expired = Срок запроса истёк: { $event }, { $date } в { $time } ({ $tz }). Организатор не ответил — выберите другое время.
email-pending-expired-status = Срок запроса истёк
email-pending-expired-headline = Срок вашего запроса на запись истёк: организатор не ответил вовремя. Выберите другое время.
email-pending-expired-action = Выбрать другое время
bae-body-host-busy = Организатор (или обязательный участник команды) в это время уже занят. Попросите гостя выбрать другое время.
bae-body-host-not-synced = Календарь Microsoft одного из организаторов ещё не прошёл первую синхронизацию, поэтому calrs не может определить, свободно ли это время. Попробуйте через минуту или проверьте ошибку синхронизации источника в разделе «Источники календарей».
bookings-error-generic = Что-то пошло не так.
bookings-error-no-bookable-dates = У этого типа встречи не осталось дат для записи, поэтому гость не сможет выбрать новое время. Сначала продлите период записи.
bookings-error-reschedule-pending = Гостя уже попросили выбрать новое время, ответа пока нет.
bookings-error-reschedule-resend-too-soon = Запрос отправлен гостю меньше часа назад. Попробуйте позже.
bookings-resend-reschedule = Отправить запрос ещё раз
bookings-resend-reschedule-hint = Отправить гостю ту же ссылку ещё раз (не чаще раза в час)
troubleshoot-detail-pending-request = { $title } (запрос ожидает подтверждения)
troubleshoot-detail-claimed-booking = { $title } (запись взята)
troubleshoot-detail-unsupported-recurrence = { $calendar } (calrs не понимает правило повторения; время считается занятым с начала события)
etf-pending-blocks-slot = Запросы, ожидающие ответа, занимают время
etf-pending-blocks-slot-help = Пока запрос ждёт вашего ответа, никто другой не может записаться на это время — ни на этот тип встречи, ни на другие ваши.
etf-pending-expiry = Срок ожидания запроса
etf-pending-expiry-help = Если вы не ответите на запрос до этого срока, он будет отклонён автоматически, а гостю предложат выбрать другое время. Срок истекает раньше, если до встречи остаётся так мало времени, что на неё уже нельзя записаться. В середине срока вы получите одно напоминание.
etf-pending-expiry-never = Без срока

etf-pending-expiry-hours =
    { $hours ->
        [one] Через { $hours } час
        [few] Через { $hours } часа
        [many] Через { $hours } часов
       *[other] Через { $hours } часа
    }

# Host, watcher and remaining guest emails (src/email.rs, #209)

email-footer-sent-by = Отправлено через
# Sender display name of guest emails: the host, then the instance name (#221).
email-sender-via = { $host } через { $instance }
email-date-at-time = { $date } в { $time }
email-detail-phone = Телефон:
email-detail-expires = Действует до:
email-detail-new-date = Новая дата:
email-detail-new-time = Новое время:
email-detail-previous = Прежнее время:
email-detail-requested = Запрошенное время:
email-detail-originally = Исходное время:
email-detail-organizer = Организатор:
email-detail-booked-by = Автор записи:
email-detail-invited-by = Приглашение от:
email-detail-message = Сообщение:
email-action-approve = Подтвердить
email-action-decline = Отклонить
email-action-pick-new-time = Выбрать новое время
email-host-approve-line = Подтвердить: { $url }
email-host-decline-line = Отклонить: { $url }
email-host-approval-dashboard = Войдите в кабинет, чтобы подтвердить или отклонить эту запись.
email-host-new-subject = Новая запись: { $event }, { $guest } ({ $date })
email-host-new-heading = Новая запись
email-host-new-message = К вам записался новый гость: { $guest }.
email-host-confirmed-subject = Подтверждено: { $event }, { $guest } ({ $date })
email-host-confirmed-heading = Запись подтверждена
email-host-confirmed-message = Вы подтвердили запись. Гость: { $guest }.
email-host-confirmed-footer = Событие добавлено в ваш календарь.
email-host-sync-failure-subject = Требуется действие: календарь не обновлён — { $event } ({ $date })
email-host-sync-failure-heading = Ваш календарь не обновлён
email-host-sync-failure-intro = Эта запись перенесена, но calrs не удалось обновить событие в Google Calendar. У гостя уже новое время, а в вашем календаре по-прежнему старое.
email-host-sync-failure-action = Откройте событие в Google Calendar и перенесите его на время, указанное выше. Именно перенесите существующее событие, а не создавайте новое, иначе ссылка Google Meet, которая уже есть у гостя, перестанет работать.
email-writeback-failure-subject = Требуется действие: запись не добавлена в календарь — { $event } ({ $date })
email-writeback-failure-heading = Запись не попала в ваш календарь
email-writeback-failure-intro = calrs не удалось добавить эту запись в ваш календарь даже после нескольких попыток. Запись подтверждена, приглашение у гостя есть.
email-writeback-failure-action = Добавьте событие в календарь вручную, чтобы не пропустить встречу, и проверьте источник календарей в calrs. calrs ещё несколько часов будет повторять попытки; больше писем об этой записи не придёт.
email-detail-error = Ошибка:
email-source-failure-subject = Требуется действие: источник календарей «{ $source }» не синхронизируется
email-source-failure-heading = Источник календарей перестал синхронизироваться
email-source-failure-intro = calrs уже больше часа не удаётся синхронизировать этот источник календарей. Страницы записи по-прежнему предлагают свободное время, но по данным последней успешной синхронизации, поэтому события, добавленные или перенесённые с тех пор, не учитываются.
email-source-failure-action = Проверьте источник на странице «Источники календарей»: обычно причина в истёкшем пароле или отозванном доступе. Следующее письмо об этом источнике придёт, только если он восстановится, а потом снова перестанет синхронизироваться.
email-detail-source = Источник календарей:
email-detail-failing-since = Перестал синхронизироваться:
email-host-reminder-subject = Напоминание: { $event }, { $guest } ({ $date })
email-host-reminder-heading = Предстоящая встреча
email-host-reminder-message = Напоминание: скоро встреча. Гость: { $guest }.
email-host-cancel-subject = Отменено: { $event }, { $guest } ({ $date })
email-host-cancel-message-by-host = Вы отменили эту запись.
email-host-cancel-message-by-guest = Запись отменена гостем: { $guest }.
email-host-approval-subject = Нужно ваше решение: { $event }, { $guest } ({ $date })
email-host-approval-heading = Нужно ваше решение
email-host-approval-message = { $guest } хочет записаться к вам.
email-host-approval-reminder-subject = Напоминание: { $guest } ждёт вашего ответа ({ $event }, { $date })
email-host-approval-reminder-heading = Напоминание: нужно ваше решение
email-host-approval-reminder-message = { $guest } всё ещё ждёт вашего ответа. Если вы не ответите до { $deadline }, запрос будет автоматически отклонён.
email-host-approval-footer = Ответить на запрос можно и в кабинете.
email-host-expired-subject = Срок запроса истёк: { $event }, { $guest } ({ $date })
email-host-expired-heading = Срок запроса истёк
email-host-expired-message = Гость: { $guest }. На запрос никто не ответил вовремя, поэтому он автоматически отклонён, а гостю предложено выбрать другое время.
email-host-expired-footer = Срок ожидания запросов можно изменить в настройках типа встречи.
email-host-reschedule-request-subject = Запрос на перенос: { $event }, { $guest } ({ $email })
email-host-reschedule-request-message = { $guest } хочет перенести запись.
email-watcher-claim-subject = Можно взять: { $event }, { $guest } ({ $date })
email-watcher-claim-message = Появилась новая запись, которую можно взять. Нажмите кнопку ниже, чтобы стать участником встречи.
email-watcher-claim-link = Взять запись: { $url }
email-watcher-claim-footer = Взять запись можно и в кабинете.
email-claim-confirmed-subject = Вы взяли запись: { $event }, { $guest } ({ $date })
email-claim-confirmed-footer = Вас добавят в участники этой встречи.
email-attendee-greeting = Здравствуйте!
email-attendee-message = Вас добавили в участники встречи.
email-attendee-rescheduled-message = Встреча, в которой вы участвуете, перенесена на другое время.
email-attendee-cancelled-message = Встреча, в которой вы участвуете, отменена.
email-attendee-cancelled-subject = Отменено: { $event }
email-original-booking-cancelled = Исходная запись тоже отменена.
email-pick-new-time-subject = Перенос: { $event }, выберите новое время
email-pick-new-time-message = { $host } просит перенести вашу запись. Выберите новое время.
email-pick-new-time-link = Выбрать новое время: { $url }
email-rescheduled-message = Организатор ({ $host }) перенёс вашу запись.
email-reschedule-approved-message = Организатор ({ $host }) подтвердил новое время вашей записи.
email-rescheduled-ics-attached-plain = Обновлённое приглашение в календарь во вложении.
email-rescheduled-ics-attached-html = К письму приложено обновлённое приглашение в календарь.
email-need-to-reschedule = Нужно перенести? { $url }
email-invite-subject = { $host } приглашает вас записаться: { $event }
email-invite-message = { $host } приглашает вас записаться: { $event }
email-invite-link = Чтобы выбрать время, перейдите по ссылке ниже:
email-invite-action = Выбрать время
email-cancel-reason-calendar-deleted = Организатор удалил событие из календаря.

# Embed snippet generator (templates/embed.html)
embed-page-title = Встраивание — { $title }
embed-heading = Встроить на сайт: { $title }
embed-subtitle = Создайте код для вставки, чтобы разместить этот тип встречи на своём сайте.
embed-warn-internal = Обратите внимание: этот тип встречи <strong>внутренний</strong>. Чтобы записаться через встроенную форму, посетителям нужна действующая ссылка-приглашение, а код для вставки её не содержит. Чтобы посетители могли записаться без приглашения, сделайте тип встречи публичным.
embed-warn-private = Обратите внимание: этот тип встречи <strong>закрытый</strong>. Чтобы записаться через встроенную форму, посетителям нужна действующая ссылка-приглашение, а код для вставки её не содержит. Чтобы посетители могли записаться без приглашения, сделайте тип встречи публичным.
embed-warn-private-team = Обратите внимание: эта команда <strong>закрытая</strong>. Без ссылки-приглашения команды посетители не смогут записаться через встроенную форму, а код для вставки её не содержит. Чтобы посетители могли записаться без приглашения, сделайте команду публичной.
embed-need-username = Чтобы встроить этот тип встречи, сначала задайте <strong>имя пользователя</strong>: ссылка для встраивания — { $path }. { $link } и вернитесь сюда.
embed-need-username-link = Задайте имя пользователя в настройках
embed-username-placeholder = имя_пользователя
embed-tab-inline = Блок на странице
embed-tab-floating = Плавающая кнопка
embed-tab-element = По нажатию на элемент
embed-layout = Вид
embed-theme = Тема
embed-theme-auto = Авто (как у посетителя)
embed-brand-color = Фирменный цвет
embed-use-org-color = Использовать цвет организации
embed-fix-time-format = Фиксированный формат времени (без переключателя 12/24 ч)
embed-fixed-tz = Показывать время в часовом поясе встречи (без выбора часового пояса)
embed-fixed-tz-hint = Подходит для очных встреч. Посетители видят местное время встречи с названием часового пояса.
embed-button-text = Текст кнопки
embed-button-default = Записаться на встречу
embed-button-position = Положение кнопки
embed-pos-bottom-right = Справа внизу
embed-pos-bottom-left = Слева внизу
embed-pos-top-right = Справа вверху
embed-pos-top-left = Слева вверху
embed-button-colors = Цвета кнопки
embed-button-fg-placeholder = текст
embed-button-colors-hint = Фон / текст
embed-show-icon = Показывать значок календаря
embed-element-help = Вставьте код на сайт один раз, затем добавьте показанные data-атрибуты к любому элементу (кнопке, ссылке, div), по нажатию на который должно открываться окно записи.
embed-snippet = Код для вставки
embed-loading = Загрузка…
embed-preview = Предпросмотр
embed-preview-back = назад к предпросмотру
embed-preview-floating-hint = Нажмите на кнопку, чтобы открыть окно записи
embed-preview-element-hint = Примеры элементов. На своём сайте добавьте data-атрибуты из кода к любому элементу. Нажмите на любой из них, чтобы открыть окно записи:
embed-preview-text-link = или нажмите на эту текстовую ссылку
embed-snippet-attr-comment = Добавьте эти атрибуты к любому элементу страницы, на который можно нажать:

# Leftovers in partly translated pages (host reschedule, impersonation banner, slots, team settings)
host-resched-when = { $date }, { $start }–{ $end }
base-impersonating = Вы вошли от имени пользователя: { $name }
slots-rescheduling-banner = Перенос: { $title } (сейчас: { $date } в { $time })
slots-invite-required-title = Для записи нужна ссылка-приглашение
team-settings-slug-url = Используется в адресе команды: { $url }
team-event-type-no-teams = Команд пока нет.
team-event-type-not-team-admin = Вы не администратор ни одной команды.

# Google Calendar connection errors (src/web/sources.rs)
google-error-not-configured = Интеграция с Google Calendar не настроена. Попросите администратора указать учётные данные Google OAuth2 в панели администратора.
google-error-no-base-url = Публичный адрес сервера не настроен. Прежде чем подключаться через OAuth2, укажите его в переменной окружения CALRS_BASE_URL или в панели администратора, в разделе «Системные настройки».
google-error-authorization = Не удалось авторизоваться в Google: { $error }
google-error-token-exchange = Не удалось получить токен: { $error }
google-error-account-email = Не удалось получить адрес эл. почты аккаунта Google: { $error }

# Admin panel notices (src/web/admin.rs)
admin-resource-feed-invalid = Неверная ICS-ссылка: { $error }
admin-resource-feed-check-failed = Не удалось проверить ICS-ссылку: { $error }
admin-resource-caldav-invalid = Неверный адрес CalDAV: { $error }
admin-resource-credentials-failed = Не удалось сохранить учётные данные: { $error }
admin-resource-added = Ресурс «{ $name }» добавлен. Событий в кэше: { $count }.
admin-resource-updated = Ресурс обновлён. Событий в кэше: { $count }.
admin-resource-deleted = Ресурс удалён.
admin-resource-not-found = Ресурс не найден.
admin-resource-synced = Синхронизация завершена. Событий в кэше: { $count }.
admin-resource-sync-error = Не удалось синхронизировать: { $error }
admin-resource-no-caldav-url = Адрес CalDAV не указан, и его не удалось определить по ICS-ссылке.
admin-resource-no-write-credentials = Нет учётных данных для добавления событий: служебной учётной записи нет, и ни у одного участника, разрешившего доступ, нет подходящего источника календарей.
admin-resource-write-passed = Проверка добавления событий от имени пользователя { $user } пройдена (создание: { $created }, удаление: { $cleaned }).
admin-resource-write-verified = проверено
admin-resource-write-unverified = не проверено
admin-resource-write-check-calendar = проверьте календарь ресурса
admin-resource-write-failed = Проверка добавления событий не пройдена. Последняя ошибка: { $error }
admin-base-url-invalid = Публичный адрес должен быть абсолютным URL с http(s), например https://cal.example.com
admin-company-link-invalid = Ссылка на компанию должна начинаться с http:// или https://
admin-smtp-host-required = Укажите сервер SMTP.
admin-smtp-from-invalid = Укажите корректный адрес эл. почты отправителя.
admin-smtp-port-invalid = Порт SMTP должен быть числом от 1 до 65535.
admin-smtp-tls-invalid = Режим TLS должен быть «starttls», «tls» или «none».
admin-smtp-password-required = При первой настройке SMTP нужно указать пароль.
admin-sms-switch-needs-secret = Чтобы сменить SMS-шлюз, введите учётные данные этого шлюза.
admin-sms-settings-invalid = Настройки SMS: { $error }.
admin-sms-sender-format = Числовой отправитель SMS указывается в международном формате, например +79001234567.
admin-sms-not-configured = SMS не настроены.
admin-sms-load-failed = Не удалось загрузить настройки SMS.
admin-sms-check-accepted = Шлюз принял учётные данные.
admin-sms-bad-number = Этот номер телефона не подходит.
admin-sms-test-body = Тестовое сообщение calrs. SMS-шлюз работает.
admin-sms-test-accepted = Шлюз принял тестовое сообщение.
admin-sms-test-over-cap = Дневной лимит SMS исчерпан, поэтому тестовое сообщение не отправлено. Тестовые сообщения учитываются в дневном лимите наравне с остальными.
admin-sms-test-ledger-failed = Не удалось учесть расход SMS, поэтому тестовое сообщение не отправлено.

## Local multi-factor authentication

mfa-title = Двухфакторная аутентификация
mfa-invalid = Проверка не пройдена. Проверьте пароль или код. Каждый код действует только один раз; при необходимости дождитесь нового. После слишком большого числа попыток подождите 15 минут. Если время настройки истекло, начните заново.
mfa-restart = Вернуться ко входу
mfa-manage = Настройка двухфакторной аутентификации
mfa-recovery-help = Сохраните эти резервные коды в надёжном месте. Они показываются только один раз. Каждый код можно использовать один раз вместо кода из приложения-аутентификатора. Новые резервные коды заменяют все предыдущие.
mfa-continue = Перейти в кабинет
mfa-continue-consent = Перейти к подтверждению доступа
mfa-enabled = Двухфакторная аутентификация включена.
mfa-disabled = Двухфакторная аутентификация выключена.
mfa-required-help = Администратор сделал двухфакторную аутентификацию обязательной для локальных аккаунтов. Выключить её нельзя.
mfa-code = Код из приложения-аутентификатора или резервный код
mfa-regenerate = Создать новые резервные коды
mfa-disable = Выключить двухфакторную аутентификацию
mfa-enable = Включить двухфакторную аутентификацию
mfa-changes-help = Для изменений нужен пароль. Чтобы выключить двухфакторную аутентификацию или заменить резервные коды, нужен ещё и код; при этом завершатся все остальные ваши сеансы.
mfa-back = Вернуться в настройки
mfa-setup-help = Отсканируйте этот QR-код приложением-аутентификатором или введите ключ ниже вручную. Чтобы завершить настройку, введите шестизначный код. На настройку даётся 10 минут.
mfa-qr-alt = QR-код для настройки приложения-аутентификатора
mfa-login-help = Введите код из приложения-аутентификатора или один из сохранённых резервных кодов. На вход даётся 10 минут.
mfa-totp-code = Шестизначный код из приложения-аутентификатора
mfa-verify = Подтвердить
mfa-policy-help = Эта политика действует для локальных аккаунтов. При входе через SSO используется двухфакторная аутентификация поставщика удостоверений. Прежде чем менять политику, локальные администраторы должны настроить двухфакторную аутентификацию у себя. Если сделать её обязательной, локальные сеансы без второго фактора будут завершены, а при следующем входе её нужно будет настроить.
mfa-policy = Двухфакторная аутентификация для локальных аккаунтов
mfa-optional = Необязательна
mfa-required = Обязательна
mfa-save-policy = Сохранить политику

# First day of the week (#330)
week-start-label = Первый день недели
week-start-inherit-site = По умолчанию для сервера
week-start-inherit-owner = Как у владельца

## Booking directory and personal favorites

directory-title = Каталог встреч
directory-help = Найдите ссылку для записи на встречу коллеги или команды. Отмечайте звёздочкой встречи, которыми часто делитесь, чтобы они были под рукой в кабинете.
directory-search = Поиск по имени, команде или названию встречи
directory-find = Найти
directory-favorites-only = Только избранное
directory-favorites = Избранные встречи
directory-browse = Открыть каталог
directory-generate = Создать ссылку-приглашение
directory-invite-help = Одноразовое приглашение, действует 7 дней.
directory-add = Добавить «{ $title }» в избранное
directory-remove = Убрать «{ $title }» из избранного
directory-added = Добавлено в избранное.
directory-removed = Убрано из избранного.
directory-error = Не удалось выполнить действие. Обновите страницу и попробуйте ещё раз.
directory-manual-copy = Скопируйте ссылку вручную:
directory-empty = Встречи не найдены. Попробуйте изменить запрос.
directory-favorites-empty = Избранных встреч нет. Отметьте встречи звёздочкой в каталоге, и они появятся здесь.
directory-with-hosts = Организаторы: { $names }

## Microsoft Graph calendar connection (independent of SSO)
microsoft-calendar-title = Календарь Microsoft 365 / Outlook.com
microsoft-calendar-help = Подключите календарь Microsoft, разрешив доступ от вашего имени. Аккаунт Microsoft может отличаться от того, с которым вы входите в calrs. Для локального Exchange используйте протокол EWS ниже.
microsoft-calendar-connect = Подключить календарь Microsoft
microsoft-calendar-unconfigured = Чтобы подключать календари, администратор должен настроить приложение Microsoft.
microsoft-admin-help = Зарегистрируйте веб-приложение именно с этим URI перенаправления. Запросите делегированные разрешения Calendars.ReadWrite, User.Read и offline_access (не разрешения приложения). Каждый пользователь подключает свой аккаунт. Неявное предоставление (implicit grant) и потоки общедоступных клиентов включать не нужно.
microsoft-tenant-label = Арендатор Microsoft (tenant)
microsoft-tenant-help = Значение common намеренно допускает и рабочие или учебные, и личные аккаунты, если регистрация приложения поддерживает оба типа. Укажите organizations, чтобы разрешить только рабочие или учебные аккаунты, consumers — только личные, или UUID арендатора / подтверждённый домен, чтобы ограничить доступ.
microsoft-admin-save = Сохранить настройки Microsoft
microsoft-error-impersonation = Нельзя подключить календарь Microsoft, пока вы вошли от имени другого пользователя.
microsoft-error-start = Не удалось начать авторизацию в Microsoft. Проверьте настройки приложения и публичный адрес сервера с HTTPS, а также убедитесь, что этот источник принадлежит вашему аккаунту.
microsoft-error-state = Запрос авторизации Microsoft недействителен, устарел или уже использован, либо изменился ваш сеанс. Начните подключение заново.
microsoft-error-consent = Авторизация в Microsoft не завершена. Начните заново и разрешите чтение и изменение календаря, доступ к профилю и автономный доступ.
microsoft-error-connect = Не удалось подключить календарь Microsoft. Проверьте настройки и разрешения приложения. Чтобы переподключиться, выберите тот же аккаунт Microsoft; чтобы использовать другой аккаунт, добавьте новый источник.
microsoft-error-sync = Аккаунт Microsoft подключён, но синхронизировать календарь не удалось. Проверьте, что у аккаунта есть календарь Exchange Online или Outlook.com. Вернитесь к источникам календарей, чтобы повторить синхронизацию или переподключиться.
microsoft-error-selection = Аккаунт Microsoft подключён, но не удалось загрузить список календарей для выбора. Вернитесь к источникам календарей.
microsoft-error-settings = Настройки Microsoft не сохранены. Проверьте UUID приложения (клиента), арендатора и доступ к базе данных. Если оставить секретный ключ пустым, сохранится текущий.
microsoft-error-heading = Календарь Microsoft не подключён
microsoft-error-settings-heading = Настройки Microsoft не сохранены
microsoft-error-back-admin = Вернуться в панель администратора
source-test-microsoft-ok = Подключение к календарю Microsoft работает.
source-test-microsoft-failed = Не удалось подключиться к календарю Microsoft. Проверьте доступ к календарю или переподключитесь.
source-test-microsoft-credentials = Не удалось загрузить учётные данные календаря Microsoft. Переподключите аккаунт.
microsoft-callback-needs-https = Укажите публичный адрес сервера с HTTPS и без пути — тогда здесь появится URI перенаправления для регистрации.
source-form-backend-ews = Локальный Microsoft Exchange (EWS)

# Global Exchange (EWS) impersonation: admin card and managed sources
sources-admin-managed = Управляется администратором
sources-admin-managed-title = Создан администратором
sources-locked-notice = Администратор управляет Exchange централизованно. Добавлять другие источники календарей нельзя.
sources-locked-by-admin = Администратор запретил добавлять личные источники календарей. Вашим календарём Exchange управляют централизованно.
sources-managed-no-edit = Этот источник календарей создан администратором, изменить его нельзя.
sources-managed-no-remove = Этот источник календарей создан администратором, удалить его нельзя.
sources-managed-global-disabled = Этим источником управляет администратор, но централизованное подключение Exchange выключено.
admin-ews-title = Exchange (EWS): централизованный доступ от имени пользователей
admin-ews-help = Укажите один адрес Exchange Web Services и служебную учётную запись с ролью RBAC <code>ApplicationImpersonation</code>. Календарь каждого пользователя читается от его имени, поэтому вводить учётные данные пользователям не нужно. PowerShell в Exchange: <code>New-ManagementRoleAssignment -Name CalrsImpersonation -Role ApplicationImpersonation -User svc-calrs@yourdomain</code>.
admin-ews-endpoint = Адрес
admin-ews-managed-count = Управляемые источники:
admin-ews-enable = Включить централизованное подключение Exchange (доступ от имени пользователей)
admin-ews-url = Адрес EWS
admin-ews-username = Имя пользователя служебной учётной записи
admin-ews-password = Пароль служебной учётной записи
admin-ews-password-keep = (оставьте пустым, чтобы не менять)
admin-ews-password-unchanged = (без изменений)
admin-ews-password-placeholder = пароль служебной учётной записи
admin-ews-domain = Домен почтовых ящиков
admin-ews-optional = (необязательно)
admin-ews-domain-help = Заполните оба поля, если адреса, известные calrs (UPN в AD, адрес эл. почты из OIDC), находятся не в том домене, что почтовые ящики Exchange. Заменяется только домен, в точности совпадающий с доменом пользователей; локальная часть адреса сохраняется: <code>alice@example.local</code> → <code>alice@example.com</code>. Адреса в других доменах используются как есть. Оставьте оба поля пустыми, чтобы обращаться к каждому пользователю по его адресу в calrs без изменений.
admin-ews-auto = Создавать автоматически для всех пользователей (текущих и будущих)
admin-ews-auto-help = Если включено, каждый активный пользователь с доверенным адресом получает управляемый источник Exchange: аккаунты, созданные командой calrs user create, аккаунты SSO, адрес которых подтвердил поставщик удостоверений, и адреса, подтверждённые ниже. При сохранении источники создаются сразу.
admin-ews-lock = Запретить обычным пользователям добавлять свои источники календарей
admin-ews-lock-help = На администраторов запрет не действует. Источники, уже добавленные пользователями, остаются, пока вы их не удалите.
admin-ews-disable-note = Пока функция выключена, управляемые источники сохраняются, но не синхронизируются, и записи в них не добавляются. После включения они снова заработают.
admin-ews-remove = Удалить управляемые источники и пароль служебной учётной записи
admin-ews-remove-confirm = Удалить все управляемые источники Exchange вместе с календарями и данными о занятости, которые calrs из них сохранил, а также сохранённый пароль служебной учётной записи? Централизованное подключение Exchange будет выключено. Записи, уже добавленные в почтовые ящики, останутся там.
admin-ews-notice-removed = Удалено управляемых источников: { $count }. Пароль служебной учётной записи удалён, централизованное подключение Exchange выключено. Записи, уже добавленные в почтовые ящики, остались там.
admin-ews-notice-https-required = Адрес EWS должен начинаться с https://: пароль служебной учётной записи передаётся в каждом запросе. Ничего не сохранено.
admin-ews-notice-password-cleared = Настройки сохранены. Адрес или служебная учётная запись изменились, поэтому сохранённый пароль удалён: введите его снова, иначе централизованное подключение Exchange не заработает.
admin-ews-notice-source-removed = Управляемый источник удалён вместе с сохранёнными календарями и данными о занятости.
admin-ews-row-remove = Удалить
admin-ews-row-remove-confirm = Удалить управляемый источник Exchange для { $email } вместе с сохранёнными календарями и данными о занятости? Автоматическое создание вернёт его при следующем сохранении, подтверждении адреса или входе через SSO; чтобы источник не появлялся, заблокируйте аккаунт.
admin-ews-trust-no-mailbox = нет (адрес не указывает на почтовый ящик)
admin-ews-notice-selections-reset = Почтовый ящик, который читают некоторые управляемые источники, изменился, поэтому их календари будут найдены заново. Типов встреч со сброшенным выбором календарей: { $count } (список — в журнале сервера).
admin-ews-notice-reconfirm = Из-за замены домена некоторые подтверждённые адреса теперь ведут к другому почтовому ящику. Адресов, которые нужно подтвердить снова: { $count } (см. список ниже).
admin-ews-row-taken-over = Передан от пользователя { $user } ({ $when })
admin-ews-notice-trust-changed = Ничего не подтверждено: адрес этого аккаунта или почтовый ящик, который он открывает, изменился после загрузки страницы. Проверьте список ещё раз.
sources-managed-user-untrusted = Этим источником управляет администратор, но адрес аккаунта ещё не подтверждён, поэтому источник не синхронизируется.
sources-managed-user-suspended = Этим источником управляет администратор, но аккаунт заблокирован, поэтому источник не синхронизируется.
admin-ews-save = Сохранить настройки EWS
admin-ews-save-provision = Сохранить и создать источники для всех
admin-ews-status-title = Статус синхронизации по пользователям
admin-ews-status-summary = Синхронизируется почтовых ящиков: { $ok } из { $total }. У пользователя без почтового ящика Exchange (например, у локального администратора) всегда будет ошибка — это нормально.
admin-ews-col-user = Пользователь
admin-ews-col-mailbox = Почтовый ящик
admin-ews-col-last-sync = Последняя синхронизация
admin-ews-col-status = Статус
admin-ews-row-disabled = выключен
admin-ews-row-synced = Синхронизирован
admin-ews-row-never = Ещё не синхронизирован
admin-ews-row-no-mailbox = Нет почтового ящика Exchange
admin-ews-notice-saved = Настройки Exchange сохранены.
admin-ews-notice-provisioned = Настройки Exchange сохранены. Создано управляемых источников: { $count }.
admin-ews-notice-invalid-url = Некорректный адрес EWS: допустимы только http(s), а частные адреса — только если они разрешены. Ничего не сохранено.
admin-ews-notice-incomplete = Чтобы включить централизованное подключение Exchange, укажите адрес, служебную учётную запись и её пароль. Ничего не сохранено.
admin-ews-notice-provision-failed = Настройки сохранены, но создать управляемые источники не удалось. Подробности — в журнале сервера.
admin-ews-registration-warning = Открыта самостоятельная регистрация. Аккаунты, созданные таким образом, никогда не получают управляемый источник Exchange, даже по кнопке «Сохранить и создать источники для всех»: нет подтверждения, что адрес принадлежит им. Чтобы подключить такому пользователю почтовый ящик, сначала убедитесь, что адрес принадлежит этому человеку, а затем подтвердите адрес ниже или выполните calrs user trust-email.
admin-ews-from-domain = Домен пользователей
admin-ews-notice-override-domains = Для замены домена почтовых ящиков нужны оба домена — домен, под которым входят пользователи, и домен их почтовых ящиков, — каждый в простом виде, например example.com. Ничего не сохранено.
admin-ews-notice-trusted = Адрес подтверждён. Если централизованное подключение Exchange включено, управляемый источник пользователя создаётся сейчас — см. таблицу статусов.
admin-ews-row-suspended = Аккаунт заблокирован: не синхронизируется
admin-ews-row-untrusted = Адрес не подтверждён: не синхронизируется
admin-ews-row-conflict = Не создан: этот почтовый ящик уже читает источник другого пользователя
admin-ews-trust-title = Адреса для подтверждения
admin-ews-trust-help = У этих активных аккаунтов в разрешённых доменах эл. почты адрес никто не подтвердил (аккаунт зарегистрирован самостоятельно или создан до появления этой проверки), поэтому управляемый источник Exchange они не получают. Подтверждайте адрес, только если знаете, что он принадлежит этому человеку: после подтверждения calrs сможет читать для него календарь этого почтового ящика.
admin-ews-trust-confirm = Подтвердить
admin-ews-search = Поиск по пользователю или почтовому ящику
admin-ews-no-match = По этому запросу не найдено ни управляемых почтовых ящиков, ни адресов для подтверждения.
admin-ews-conflicts-more = Почтовый ящик уже используется другим пользователем: показано { $shown } из { $total }.
admin-ews-http-warning = Адрес использует http://: пароль служебной учётной записи передаётся без шифрования. Рекомендуем https://.
# Time off (templates/time_off.html, src/web/time_off.rs, troubleshoot view)

nav-time-off = Отсутствие
time-off-page-title = Отсутствие
time-off-heading = Отсутствие
time-off-subtitle = Периоды, когда вы не принимаете записи. Они действуют для всех типов встреч, где вы организатор, — личных и командных. Гости видят только, что это время недоступно.
time-off-upcoming = Запланировано
time-off-empty = Запланированных периодов отсутствия нет.
time-off-kept-notice = Записи, уже сделанные на этот период, сохраняются: отсутствие ничего не отменяет. При необходимости отмените или перенесите их в разделе «Записи».
time-off-pending-notice = Запросы на время внутри периода отсутствия подтвердить уже нельзя: отклоните их или сначала удалите период.
time-off-in-progress = Идёт сейчас
time-off-entered-in = Указано в часовом поясе { $timezone }
time-off-delete = Удалить
time-off-delete-confirm = Удалить этот период отсутствия? Это время снова станет доступным для записи.
time-off-add-heading = Добавить отсутствие
time-off-from = Начало
time-off-to = Конец
time-off-time-optional = Время (необязательно)
time-off-times-help = Чтобы закрыть дни целиком, оставьте время пустым — обе даты входят в период. Время указывается в вашем часовом поясе: { $timezone }.
time-off-label = Заметка (необязательно)
time-off-label-help = Её видите только вы — здесь и в разделе «Диагностика». Гости и коллеги по команде её не видят.
time-off-team-admins-notice = Администраторы ваших команд и администраторы сервера видят даты и время вашего отсутствия, но не заметку.
time-off-add-submit = Добавить отсутствие
time-off-added = Период отсутствия добавлен.
time-off-deleted = Период отсутствия удалён.
time-off-range = { $start } — { $end }
time-off-at = { $date }, { $time }
time-off-same-day = { $date }, { $start }–{ $end }
time-off-from-time = { $date }, с { $time }
time-off-until-time = { $date }, до { $time }
time-off-impersonating = Вы просматриваете эту страницу от имени пользователя { $name }. Периоды отсутствия здесь доступны только для просмотра, а заметки скрыты.
time-off-error-date = Введите даты в формате ГГГГ-ММ-ДД, а время — в формате ЧЧ:ММ.
time-off-error-order = Конец должен быть позже начала.
time-off-error-too-long = Период не может быть длиннее года.
time-off-error-past = Этот период уже закончился.
time-off-error-label = Заметка может содержать не более 200 символов.
time-off-error-too-many = У вас уже запланировано 100 периодов. Удалите ненужные, прежде чем добавлять новые.
time-off-hint = Чтобы закрыть даты сразу во всех типах встреч, перейдите в раздел «{ $link }».
troubleshoot-label-time-off = Отсутствие
troubleshoot-legend-time-off = Отсутствие

directory-all-favorites = Всё избранное
pager-label = Страницы
pager-range = { $from }–{ $to } из { $total }
pager-previous = Назад
pager-next = Вперёд
admin-users-search = Найти
admin-figures-cached = Статистика записей рассчитана { $seconds } с назад.

# Connected apps (templates/dashboard_apps.html, admin user menu)

apps-page-title = Подключённые приложения
apps-heading = Подключённые приложения
apps-subtitle = Приложения, которым вы разрешили действовать от вашего имени, например ИИ-ассистенты, подключённые по MCP. Если отозвать доступ, он прекратится сразу.
apps-approved = Подтверждено: { $date }
apps-last-used = Последнее использование: { $date }
apps-never-used = Ещё не использовалось
apps-revoke = Отозвать
apps-revoke-confirm = Отозвать доступ у приложения { $name }? Чтобы подключиться снова, приложению понадобится новое подтверждение.
apps-revoked = Доступ отозван.
apps-empty = Подключённых приложений нет. MCP-клиент запросит ваше подтверждение при первом подключении.
oauth-slots = Просматривать свободное время в типах встреч, которыми вы управляете
oauth-bookings-read = Просматривать записи, где вы организатор, вместе с данными гостей
oauth-bookings-write = Создавать, отменять и переносить записи в типах встреч, которыми вы управляете, с отправкой писем гостям
oauth-api-closed = Это приложение запрашивает доступ к REST API, а на этом сервере им могут пользоваться только администраторы. Подтвердить его нельзя.
api-tokens-page-title = API-токены
api-tokens-heading = API-токены
api-tokens-subtitle = Токены, с помощью которых ваши скрипты обращаются к REST API calrs от вашего имени. Токен действует, пока не истечёт его срок или пока вы его не отзовёте.
api-tokens-not-allowed = На этом сервере API-токены могут создавать только администраторы.
api-tokens-new = Новый токен
api-tokens-name = Название
api-tokens-name-hint = Для чего он нужен, например: синхронизация с CRM
api-tokens-scopes = Права
api-tokens-expiry = Срок действия
api-tokens-expiry-7 = 7 дней
api-tokens-expiry-30 = 30 дней
api-tokens-expiry-90 = 90 дней
api-tokens-expiry-365 = 1 год
api-tokens-expiry-never = Бессрочно
api-tokens-create = Создать токен
api-tokens-created = Скопируйте токен сейчас: он показывается только один раз.
api-tokens-created-on = Создан { $date }
api-tokens-expires-on = Действует до { $date }
api-tokens-never-expires = Бессрочный
api-tokens-revoke-confirm = Отозвать токен «{ $name }»? Скрипты, которые его используют, сразу перестанут работать.
api-tokens-revoked = Токен отозван.
api-tokens-empty = API-токенов пока нет.
api-tokens-error-name = Укажите название токена (не длиннее 100 символов).
api-tokens-error-scope = Выберите хотя бы одно право доступа.
api-tokens-error-limit = У вас уже 20 токенов. Сначала отзовите один из них.
api-tokens-docs = Как пользоваться токеном, описано в главе о REST API в документации calrs.
api-docs-title = Справочник API
api-docs-subtitle = Все адреса REST API calrs — в описании, которое формирует сам этот сервер.
api-docs-closed = На этом сервере REST API доступен только администраторам.
api-docs-try = Чтобы отправить пробный запрос, нажмите Authorize и вставьте личный токен доступа. Эта страница не передаёт в API ваш сеанс.
api-docs-tokens = API-токены
api-docs-spec = Документ OpenAPI (JSON)
api-docs-guide = Руководство по REST API
api-docs-noscript = Для интерактивного справочника нужен JavaScript. Тот же API описан в документе OpenAPI.
admin-api-tokens = API-токены
admin-api-tokens-allow-all = Разрешить всем пользователям работать с REST API
admin-api-tokens-allow-all-help = По умолчанию выключено: создавать личные токены доступа и подключать приложения к REST API через OAuth могут только администраторы, потому что и то и другое даёт доступ к данным гостей. Если выключить настройку, токены и API-приложения других пользователей перестанут работать, пока её снова не включат. MCP-клиентов это не касается.
admin-revoke-apps = Отозвать доступ у приложений ({ $count })

# Write destination per event type (#47)
etf-write-destination = Календарь для подтверждённых записей
etf-write-destination-default = По умолчанию (как в настройках источника)
etf-write-destination-help = Только для этого типа встречи. По умолчанию записи добавляются в календарь для новых записей каждого вашего источника. В командном типе встречи это касается только записей, где организатор — вы.
etf-write-destination-team = Каждый участник сам выбирает календарь для записей, где он организатор, в разделе «{ $link }».
sources-destinations-heading = Записи по типам встреч
sources-destinations-help = Выберите, в какой календарь добавлять подтверждённые записи каждого типа встречи. В командном типе встречи это касается записей, где организатор — вы; другие участники выбирают сами. Уже добавленные записи переместятся при следующем обновлении в календаре: при переносе, подтверждении, повторной попытке или когда запись возьмут.
sources-destinations-saved = Сохранено. Новые записи будут добавляться в выбранные здесь календари.
write-destination-not-yours = Ничего не сохранено: выберите свой календарь для типа встречи, где вы организатор.
write-destination-unavailable = { $name } (недоступен)
write-destination-refused = Этот календарь не принял последнюю запись ({ $error }), поэтому она добавлена в календарь из настроек источника. Для следующей записи calrs снова попробует этот календарь.
# Booking webhooks (templates/webhooks.html, admin card, #168)
webhooks-title = Вебхуки
webhooks-back = Вернуться к типам встреч
webhooks-subtitle = calrs отправляет подписанное JSON-сообщение на эти адреса, когда запись создают, запрашивают, подтверждают, отклоняют, отменяют или переносят, а также когда истекает срок запроса.
webhooks-not-allowed = На этом сервере вебхуки доступны только администраторам.
webhooks-endpoints = Адреса
webhooks-empty = Вебхуков пока нет. Добавьте вебхук ниже, чтобы calrs сообщал другой системе о ваших записях.
webhooks-scope = Для каких типов встреч
webhooks-scope-personal = Все мои личные типы встреч
webhooks-scope-event-type = Тип встречи: { $title }
webhooks-scope-team-event-type = Тип встречи: { $title } ({ $team })
webhooks-added-by = Добавил(а): { $name }
webhooks-state-off = Выключен
webhooks-never-sent = Ещё ничего не отправлено
webhooks-state-ok = Последняя доставка прошла успешно
webhooks-state-failing = Последняя доставка не удалась
webhooks-queued =
    { $count ->
        [one] { $count } доставка ждёт повторной попытки
        [few] { $count } доставки ждут повторной попытки
        [many] { $count } доставок ждут повторной попытки
       *[other] { $count } доставки ждут повторной попытки
    }
webhooks-gave-up =
    { $count ->
        [one] { $count } доставка не удалась после всех попыток
        [few] { $count } доставки не удались после всех попыток
        [many] { $count } доставок не удалось после всех попыток
       *[other] { $count } доставки не удались после всех попыток
    }
webhooks-send-test = Отправить тест
webhooks-enable = Включить
webhooks-disable = Выключить
webhooks-delete = Удалить
webhooks-delete-confirm = Удалить этот вебхук? Доставки, которые ещё ждут отправки, будут отменены.
webhooks-edit = Изменить
webhooks-url = Адрес
webhooks-url-hint = (https)
webhooks-secret = Секретный ключ подписи
webhooks-secret-hint = (не менее { $min } символов; подписывает каждый запрос и больше не показывается)
webhooks-save = Сохранить
webhooks-add = Новый вебхук
webhooks-add-submit = Добавить вебхук
webhooks-privacy-note = Каждое сообщение содержит имя, адрес эл. почты, телефон и комментарий гостя. Отправляйте его только в систему, которой вы доверяете эти данные.
webhooks-notice-created = Вебхук добавлен. Нажмите «Отправить тест», чтобы проверить доставку.
webhooks-notice-updated = Вебхук сохранён.
webhooks-notice-deleted = Вебхук удалён.
webhooks-notice-enabled = Вебхук включён.
webhooks-notice-disabled = Вебхук выключен. Доставки, которые ещё ждали отправки, отменены.
webhooks-notice-test-ok = Тест доставлен: получатель вернул код 2xx.
webhooks-error-url = Введите полный адрес, например https://hooks.example.com/calrs.
webhooks-error-https = Адрес должен начинаться с https://. Обычный http работает только для хостов из списка «Разрешённые внутренние хосты».
webhooks-error-credentials = Уберите имя пользователя и пароль из адреса: подлинность calrs подтверждает секретный ключ подписи.
webhooks-error-secret = Секретный ключ подписи должен содержать от 16 до 256 символов.
webhooks-error-scope = Вы не можете добавить вебхук к этому типу встречи.
webhooks-error-limit = Для выбранных типов встреч уже есть 10 вебхуков — это максимум.
webhooks-error-test = Тест не доставлен. Причина указана у вебхука.
admin-webhooks = Вебхуки записей
admin-webhooks-help =
    { $count ->
        [one] На этом сервере настроен { $count } вебхук.
        [few] На этом сервере настроено { $count } вебхука.
        [many] На этом сервере настроено { $count } вебхуков.
       *[other] На этом сервере настроено { $count } вебхука.
    }
admin-webhooks-allow-all = Разрешить всем пользователям добавлять вебхуки
admin-webhooks-allow-all-help = По умолчанию выключено: вебхук отправляет имена, адреса эл. почты и телефоны гостей на адрес, который выбирает пользователь, поэтому добавлять вебхуки могут только администраторы. Если выключить настройку, вебхуки, добавленные другими пользователями, перестанут работать.
admin-webhooks-manage = Настроить свои вебхуки
webhooks-events = События
webhooks-events-all = Все события
webhooks-event-created = Новая запись (подтверждена)
webhooks-event-requested = Новый запрос (ждёт подтверждения)
webhooks-event-confirmed = Подтверждена
webhooks-event-declined = Отклонена
webhooks-event-expired = Срок запроса истёк без ответа
webhooks-event-cancelled = Отменена
webhooks-event-rescheduled = Перенесена
webhooks-error-events = Выберите хотя бы одно событие.
webhooks-error-test-limit = Слишком много тестов за короткое время. Подождите несколько минут и попробуйте ещё раз.
webhooks-added-by-former = Добавил бывший участник
webhooks-signature-help = Каждый запрос подписан: <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> — это HMAC-SHA256 исходного тела запроса, вычисленный с секретным ключом подписи. Формат тела и проверки, которые должен выполнять получатель, описаны на странице о вебхуках в документации calrs.
webhooks-state-suspended = Приостановлен: тот, кто его добавил, больше не может управлять этим типом встречи. Возьмите вебхук на себя, чтобы возобновить его работу.
webhooks-adopt = Взять на себя
webhooks-notice-adopted = Вебхук теперь ваш, его работа возобновлена.
webhooks-event-claimed = Взята наблюдателем
webhooks-error-address = Этот адрес нельзя использовать: calrs отправляет только на публичные адреса в интернете, если хост не входит в список «Разрешённые внутренние хосты».
webhooks-adopt-help = Вебхук станет вашим и снова заработает. Проверьте адрес, на который он отправляет, и задайте новый секретный ключ подписи: прежний может знать тот, кто добавил вебхук. Доставки, которые ещё ждут отправки, будут отменены.
webhooks-error-not-suspended = Взять на себя можно только приостановленный вебхук.
webhooks-error-new-secret = Адрес теперь указывает на другой хост: введите также новый секретный ключ подписи.
webhooks-url-masked-hint = (полный адрес видит только тот, кто добавил вебхук: чтобы сохранить, введите его заново)

# Booking questions (#210): booking form, answers, event type editor

book-question-choose = Выберите вариант
book-question-pick-one = Отметьте хотя бы один вариант.
book-question-read-policy = Прочитать условия
book-question-new-tab = (откроется в новой вкладке)
answer-yes = Да
answer-no = Нет
answer-agreed = Согласие дано
answer-not-agreed = Согласие не дано
answer-you-agreed = Вы дали согласие
answer-you-did-not-agree = Вы не дали согласия
answer-agreed-recorded-by = Согласие дано (отмечено: { $name })
answer-recorded-for-you-by = Отмечено от вашего имени: { $name }
bookings-answers = Ответы
validate-notes-required = Добавьте комментарий для организатора.
validate-question-required = Ответьте на вопрос «{ $question }».
validate-question-too-long = Ответ на вопрос «{ $question }» слишком длинный.
validate-question-unknown-option = Ответ на вопрос «{ $question }» не совпадает ни с одним из предложенных вариантов.
validate-questions-invalid = Не удалось прочитать вопросы при записи. Обновите страницу и попробуйте ещё раз.
validate-questions-too-many = Максимум вопросов при записи для одного типа встречи: { $max }.
validate-question-kind = Вопрос { $position }: этот тип вопроса недоступен.
validate-question-kind-change = Вопрос { $position }: у сохранённого вопроса нельзя изменить тип. Удалите его и добавьте новый.
validate-question-label-empty = Вопрос { $position }: не заполнен текст вопроса.
validate-question-label-too-long = Вопрос { $position }: текст вопроса слишком длинный (не более 200 символов, для флажка — 1000).
validate-question-help-too-long = Вопрос { $position }: подсказка слишком длинная (не более 500 символов).
validate-question-options-few = Вопрос { $position }: укажите хотя бы два варианта.
validate-question-options-many = Вопрос { $position }: не более 30 вариантов.
validate-question-option-empty = Вопрос { $position }: один из вариантов пуст.
validate-question-option-too-long = Вопрос { $position }: вариант слишком длинный (не более 200 символов).
validate-question-option-duplicate = Вопрос { $position }: у двух вариантов одинаковый текст.
validate-question-link = Вопрос { $position }: ссылка на условия должна быть полным адресом, который начинается с https:// или http://.
etf-section-booking-form = Форма записи
etf-booking-form-help = Гости всегда указывают имя и адрес эл. почты. Добавьте вопросы, нужные для этого типа встречи: гости ответят на них при записи, а ответы придут вам вместе с записью.
etf-notes-mode = Поле «Комментарий»
etf-notes-mode-off = Скрыто
etf-notes-mode-optional = Необязательно
etf-notes-mode-required = Обязательно
etf-q-empty = Вопросов пока нет.
etf-q-add = Добавить вопрос
etf-q-kind = Тип
etf-q-kind-short-text = Короткий текст
etf-q-kind-long-text = Длинный текст
etf-q-kind-single-choice = Один вариант
etf-q-kind-multiple-choice = Несколько вариантов
etf-q-kind-checkbox = Флажок
etf-q-label = Вопрос
etf-q-help = Подсказка
etf-q-required = Обязательный
etf-q-options = Варианты
etf-q-option = Вариант %1
etf-q-add-option = Добавить вариант
etf-q-remove-option = Убрать этот вариант
etf-q-consent = Флажок означает согласие (например, с политикой конфиденциальности или на запись разговора)
etf-q-link = Ссылка на условия
etf-q-link-help = Необязательно. Показывается под флажком и открывается в новой вкладке.
etf-q-in-calendar = Добавлять ответ в событие календаря
etf-q-in-calendar-help = Выключите для конфиденциальных ответов: их увидит любой, у кого есть доступ к вашему календарю.
etf-q-not-in-calendar = Не добавляется в событие календаря
etf-q-consent-word = Согласие
etf-q-edit = Изменить
etf-q-done = Готово
etf-q-move-up = Переместить вверх
etf-q-move-down = Переместить вниз
etf-q-remove = Убрать
etf-q-untitled = Вопрос без текста
etf-q-kind-locked = У сохранённого вопроса нельзя изменить тип.
etf-q-limit = Максимум вопросов: %1.
etf-q-preview = Предпросмотр страницы записи
etf-q-key = Ключ для вебхуков: %1
etf-hint-questions-one = 1 вопрос
etf-hint-questions-other = вопросов: %1
etf-hint-no-questions = Нет вопросов

# Language names, for the languages a host speaks (landing.rs).
language-name-en = английский
language-name-fr = французский
language-name-de = немецкий
language-name-es = испанский
language-name-it = итальянский
language-name-pt = португальский
language-name-nl = нидерландский
language-name-pl = польский
language-name-et = эстонский
language-name-ar = арабский
language-name-zh = китайский
language-name-ja = японский
language-name-ko = корейский
language-name-ru = русский
language-name-uk = украинский
language-name-sv = шведский
language-name-da = датский
language-name-nb = норвежский
language-name-fi = финский
language-name-cs = чешский
language-name-ro = румынский
language-name-el = греческий
language-name-tr = турецкий
language-name-hi = хинди
language-name-he = иврит
language-name-hu = венгерский

# Sections and order of the landing pages (landing.rs, event type form, settings).
etf-section = Раздел
etf-section-placeholder = Знакомство
etf-section-help = На публичной странице встречи одного раздела собраны под его названием — точно так, как вы его написали.
etf-section-order-link = Изменить порядок
settings-based-in = Местоположение
settings-based-in-placeholder = Москва, Россия
settings-based-in-help = Город или регион; показывается под вашим именем на публичной странице.
settings-speaks = Языки, на которых вы говорите
settings-speaks-help = Показываются под вашим именем на публичной странице, на языке каждого посетителя.
page-order-heading = Порядок на вашей странице
page-order-team-heading = Порядок на странице команды «{ $team }»
page-order-subtitle = Разделы и встречи показываются на публичной странице в этом порядке. Раздел встречи задаётся в её настройках.
page-order-back = Назад
page-order-empty = Встреч пока нет.
page-order-up = Переместить вверх
page-order-down = Переместить вниз
page-order-section-up = Переместить «{ $section }» вверх
page-order-section-down = Переместить «{ $section }» вниз
page-order-type-up = Переместить «{ $title }» вверх
page-order-type-down = Переместить «{ $title }» вниз
page-order-not-listed = Не показывается на публичной странице (выключен, внутренний или закрытый)
