# Booking confirmation page (templates/confirmed.html)

confirmed-page-title-pending = Rezerwacja oczekująca
confirmed-page-title-booked = Rezerwacja potwierdzona

confirmed-heading-reschedule-requested = Prośba o zmianę terminu
confirmed-heading-rescheduled = Termin zmieniony!
confirmed-heading-pending = Oczekuje na potwierdzenie
confirmed-heading-booked = Zarezerwowano!

confirmed-subtitle-reschedule-requested = Twoja prośba o zmianę terminu została wysłana do { $host }. Otrzymasz e-mail na adres { $email }, gdy zostanie zatwierdzona.
confirmed-subtitle-rescheduled = Twoja rezerwacja została przeniesiona. E-mail potwierdzający został wysłany na adres { $email }.
confirmed-subtitle-pending = Twoja prośba o rezerwację została wysłana do { $host }. Otrzymasz e-mail na adres { $email }, gdy zostanie potwierdzona.
confirmed-subtitle-booked = E-mail potwierdzający został wysłany na adres { $email }.

confirmed-detail-event = Wydarzenie:
confirmed-detail-date = Data:
confirmed-detail-time = Godzina:
confirmed-detail-with = Z:
confirmed-detail-location = Miejsce:
confirmed-detail-notes = Notatki:
confirmed-detail-additional-guests = Dodatkowi goście:

confirmed-book-another = Zarezerwuj inny termin

confirmed-add-to-calendar = Dodaj do kalendarza

# Slot picker (templates/slots.html)

slots-location-video = Wideokonferencja
slots-location-phone = Rozmowa telefoniczna

slots-tz-label = Twoja strefa czasowa
slots-time-format-label = Format czasu

slots-view-month = Widok miesiąca
slots-view-week = Widok tygodnia
slots-view-column = Widok listy

slots-weekday-mon = Pon
slots-weekday-tue = Wt
slots-weekday-wed = Śr
slots-weekday-thu = Czw
slots-weekday-fri = Pt
slots-weekday-sat = Sob
slots-weekday-sun = Nd

slots-weekday-mon-short = P
slots-weekday-tue-short = W
slots-weekday-wed-short = Ś
slots-weekday-thu-short = C
slots-weekday-fri-short = P
slots-weekday-sat-short = S
slots-weekday-sun-short = N

slots-select-date = Wybierz datę
slots-loading-availability = Ładowanie dostępności...
slots-click-highlighted = Kliknij wyróżnioną datę, aby zobaczyć dostępne godziny
slots-no-times-month = Brak dostępnych godzin w tym miesiącu
slots-no-times-day = Brak dostępnych godzin w tym dniu
slots-no-availability-participants = Brak wspólnej dostępności wszystkich uczestników w tym miesiącu
slots-week-more = więcej

# Booking form (templates/book.html)

book-page-title = Zarezerwuj { $title }
book-back-to-times = Powrót do godzin
book-name-label = Twoje imię
book-name-placeholder = Jan Kowalski
book-email-label = E-mail
book-email-placeholder = jan@example.com
book-email-invalid = Proszę podać pełny adres e-mail wraz z domeną (np. jane@example.com).
book-notes-label = Notatki
book-notes-optional = (opcjonalne)
book-notes-placeholder = Czy jest coś, co chcesz omówić?
book-additional-guests-label = Dodatkowi goście
book-additional-guests-hint = (opcjonalne, do { $max })
book-add-guest-btn = + Dodaj gościa
book-guest-email-placeholder = wspolpracownik@example.com
book-phone-label = Numer telefonu
book-phone-placeholder = 601 234 567
book-phone-help = Numery lokalne są w porządku; jeśli nie zaczniesz od +, przyjmujemy { $country }.
book-phone-optional-consequence = Zostaw puste, jeśli wolisz nie dostawać SMS-ów o tej rezerwacji.
book-phone-required = Ta rezerwacja wymaga numeru telefonu.
book-phone-invalid-title = Nieprawidłowy numer telefonu
book-phone-invalid = Proszę podać numer, na który możemy wysłać SMS, albo zostawić pole puste.
book-phone-country-search = Szukaj
book-phone-country-label = Wybierz kraj
book-phone-country-none = Nie wybrano kraju
book-phone-country-no-results = Żaden kraj nie pasuje do wyszukiwania
captcha-label = Weryfikacja bezpieczeństwa
captcha-initial-state = Potwierdź, że jesteś człowiekiem
captcha-verifying = Weryfikacja...
captcha-solved = Jesteś człowiekiem
captcha-error = Błąd
captcha-troubleshooting = Rozwiązywanie problemów
captcha-wasm-disabled = Włącz WASM dla znacznie szybszego rozwiązywania
captcha-verify-aria = Kliknij, aby potwierdzić, że jesteś człowiekiem
captcha-verifying-aria = Weryfikacja, proszę czekać
captcha-verified-aria = Zweryfikowano
captcha-required = Proszę potwierdzić, że jesteś człowiekiem
captcha-error-aria = Wystąpił błąd, spróbuj ponownie
book-confirm-button = Potwierdź rezerwację

# SMS notifications (src/sms/message.rs).
#
# These are text messages, billed per 160-character segment (70 if the text
# contains any character outside the GSM-7 alphabet, which includes most
# accented letters). Keep them short and plain.

sms-confirmed = Rezerwacja potwierdzona: { $event }, { $date } o { $time } ({ $tz }).
sms-cancelled = Rezerwacja odwołana: { $event }, { $date } o { $time } ({ $tz }).
sms-rescheduled = Rezerwacja przeniesiona: { $event } odbędzie się { $date } o { $time } ({ $tz }).
sms-reminder = Przypomnienie: { $event } zaczyna się { $date } o { $time } ({ $tz }).

# Shared labels used across the cancel / decline / approve / reschedule / claim flows

common-detail-guest = Gość:
common-detail-reason = Powód:
common-reason-optional = (opcjonalne)
common-close-page = Możesz zamknąć tę stronę.

# Cancel flow (booking_cancel_form.html, booking_cancelled_guest.html)

cancel-page-title = Anuluj rezerwację
cancel-heading = Anuluj rezerwację
cancel-subtitle = Zamierzasz anulować swoją rezerwację.
cancel-reason-label = Powód
cancel-reason-placeholder-host = Wyjaśnij organizatorowi powód...
cancel-button = Anuluj rezerwację
cancelled-heading = Rezerwacja anulowana
cancelled-subtitle = Twoja rezerwacja została anulowana, a organizator został powiadomiony.

# Decline flow (booking_decline_form.html, booking_declined.html)

decline-page-title = Odrzuć rezerwację
decline-heading = Odrzuć rezerwację
decline-subtitle = Zamierzasz odrzucić tę prośbę o rezerwację.
decline-reason-placeholder-guest = Wyjaśnij gościowi powód...
decline-button = Odrzuć rezerwację
declined-heading = Rezerwacja odrzucona
declined-subtitle = Rezerwacja została odrzucona, a gość został powiadomiony.

# Approve flow (booking_approve_form.html, booking_approved.html)

approve-page-title = Zatwierdź rezerwację
approve-heading = Zatwierdź rezerwację
approve-subtitle = Zamierzasz zatwierdzić tę prośbę o rezerwację.
approve-button = Zatwierdź rezerwację
approved-heading = Rezerwacja zatwierdzona
approved-subtitle = Rezerwacja została potwierdzona, a e-mail z potwierdzeniem został wysłany na adres { $email }.

# Claim flow (booking_claim_form.html, booking_claimed.html, booking_already_claimed.html)

claim-page-title = Przejmij rezerwację
claim-heading = Przejmij rezerwację
claim-subtitle = Zamierzasz przejąć tę rezerwację. Zostaniesz dodany jako uczestnik.
claim-assigned-to = Przypisana do:
claim-button = Przejmij tę rezerwację
claimed-page-title = Rezerwacja przejęta
claimed-heading = Rezerwacja przejęta
claimed-subtitle = Przejąłeś tę rezerwację. Zaproszenie kalendarza zostało wysłane na Twój adres e-mail.
already-claimed-page-title = Już przejęta
already-claimed-heading = Już przejęta
already-claimed-subtitle = Ta rezerwacja została już przejęta przez { $name }.

# Generic error page (booking_action_error.html)

action-error-page-title = Błąd akcji rezerwacji

# Host-initiated reschedule (booking_host_reschedule.html)

host-resched-page-title = Zmień termin rezerwacji — calrs
host-resched-heading = Zmień termin rezerwacji
host-resched-subtitle = Spowoduje to wysłanie do { $guest } e-maila z prośbą o wybranie nowego terminu.
host-resched-currently = Obecnie:
host-resched-button = Wyślij prośbę o zmianę terminu
host-resched-cancel-link = Anuluj

# Guest reschedule confirmation (booking_reschedule_confirm.html)

resched-confirm-page-title = Potwierdź zmianę terminu
resched-confirm-heading = Potwierdź zmianę terminu
resched-confirm-subtitle = Zamierzasz przenieść swoją rezerwację na nowy termin.
resched-was = Było:
resched-new = Nowy:
resched-button = Potwierdź zmianę terminu
resched-back-to-picker = Powrót do wyboru terminu

# Base layout chrome (templates/base.html)

base-loader-checking = Sprawdzanie dostępności
base-loader-please-wait = Proszę czekać, ładowanie najnowszych danych kalendarza...
base-stop-impersonating = Zakończ podszywanie się
base-theme-toggle = Przełącz motyw
base-powered-by = Zasilane przez

# Profile (templates/profile.html)

profile-pick-event-type-invite = Wybierz typ wydarzenia, aby zarezerwować termin.
profile-no-event-type = Nie ma jeszcze dostępnych typów wydarzeń.

# Month and weekday names + per-locale date format patterns.
# Used by server-side date formatters in src/i18n.rs.

common-month-1 = styczeń
common-month-2 = luty
common-month-3 = marzec
common-month-4 = kwiecień
common-month-5 = maj
common-month-6 = czerwiec
common-month-7 = lipiec
common-month-8 = sierpień
common-month-9 = wrzesień
common-month-10 = październik
common-month-11 = listopad
common-month-12 = grudzień

common-weekday-long-mon = poniedziałek
common-weekday-long-tue = wtorek
common-weekday-long-wed = środa
common-weekday-long-thu = czwartek
common-weekday-long-fri = piątek
common-weekday-long-sat = sobota
common-weekday-long-sun = niedziela

# Format patterns are parametric per locale to handle word order. Translators
# pick where each placeholder lands. Example outputs:
#   EN: April 2026  /  Tuesday, March 12, 2026
#   FR: avril 2026  /  mardi 12 mars 2026
#   ES: abril 2026  /  martes, 12 de marzo de 2026
common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday }, { $day } { $month } { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Zmień termin
email-action-cancel-booking = Anuluj rezerwację

# Email: guest booking confirmation

# Kept to "event — date": Exchange titles the guest appointment after the
# email Subject header, not the ICS SUMMARY (#157).
email-confirm-subject = { $event } — { $date }
email-confirm-greeting = Cześć { $name },
email-confirm-headline = Twoja rezerwacja została potwierdzona!
email-confirm-ics-attached-plain = Zaproszenie do kalendarza w załączniku.
email-confirm-ics-attached-html = Zaproszenie do kalendarza w załączniku tego e-maila.
email-confirm-need-to-cancel = Chcesz anulować? { $url }

# Email: guest reminder

email-reminder-subject = Przypomnienie: { $event } o { $time }
email-reminder-headline = Twoje spotkanie wkrótce się rozpocznie.

# Email: guest cancellation

email-cancel-subject = Anulowano: { $event } — { $date }
email-cancel-headline-by-host = Twoja rezerwacja została anulowana przez { $host }.
email-cancel-headline-by-guest = Twoja rezerwacja została anulowana.
email-cancel-ics-attached-plain = Anulowanie kalendarza w załączniku.
email-cancel-ics-attached-html = Anulowanie kalendarza w załączniku tego e-maila.

# Confirmation email: notice-window policy lines (src/email.rs)

email-confirm-cancel-notice = Uwaga: odwołanie wymaga co najmniej { $minutes } minut wyprzedzenia.
email-confirm-reschedule-notice = Uwaga: przeniesienie wymaga co najmniej { $minutes } minut wyprzedzenia.

# Event type form: cancel/reschedule minimum notice (templates/event_type_form.html)

event-type-form-cancel-notice-label = Minimalne wyprzedzenie przy odwołaniu
event-type-form-reschedule-notice-label = Minimalne wyprzedzenie przy przenoszeniu
event-type-form-notice-help = Zostaw puste, aby nie wprowadzać ograniczenia.
event-type-form-resources-label = Wymagane zasoby
event-type-form-resources-hint = Terminy są proponowane tylko wtedy, gdy wybrane zasoby są dostępne, zgodnie z trybem poniżej.
event-type-form-resources-mode-all = Wszystkie wybrane zasoby muszą być wolne
event-type-form-resources-mode-round-robin = Wystarczy jeden wolny zasób (zostanie przypisany do rezerwacji)
event-type-form-notice-unit-minutes = minut
event-type-form-notice-unit-hours = godzin
event-type-form-notice-unit-days = dni
event-type-form-booking-horizon-label = Horyzont rezerwacji
event-type-form-booking-horizon-help = Z jakim wyprzedzeniem goście mogą rezerwować. Puste oznacza brak limitu, 0 tylko dzisiaj.

# Booking confirmation: cancel/reschedule policy notices (templates/confirmed.html)

confirmed-cancel-notice-info = Odwołanie wymaga co najmniej { $minutes } minut wyprzedzenia przed spotkaniem.
confirmed-reschedule-notice-info = Przeniesienie wymaga co najmniej { $minutes } minut wyprzedzenia przed spotkaniem.

# Booking action blocked page (templates/booking_action_blocked.html)

booking-blocked-title-cancel = Tej rezerwacji nie można już odwołać przez internet
booking-blocked-title-reschedule = Tej rezerwacji nie można już przenieść przez internet
booking-blocked-body = Gospodarz wymaga co najmniej { $minutes } minut wyprzedzenia. Jeśli nie możesz się pojawić, napisz bezpośrednio na <a href="mailto:{ $host_email }">{ $host_email }</a>.

# Dashboard event types listing (templates/dashboard_event_types.html)

dashboard-event-types-copy = Kopiuj
dashboard-event-types-copied = Skopiowano!
dashboard-event-types-copy-title = Kopiuj link do rezerwacji
dashboard-event-types-copy-failed = Nie udało się skopiować

# Dashboard sidebar and shared chrome (templates/dashboard_base.html)

nav-section-scheduling = Planowanie
nav-overview = Przegląd
nav-event-types = Typy wydarzeń
nav-bookings = Rezerwacje
nav-teams = Zespoły
nav-section-shared-links = Udostępnione linki
nav-invite-links = Linki z zaproszeniem
nav-section-calendars = Kalendarze
nav-sources = Źródła
nav-section-personal = Osobiste
nav-settings = Profil i ustawienia
nav-troubleshoot = Diagnostyka
nav-section-admin = Administracja
nav-admin-panel = Panel administracyjny
nav-sign-out = Wyloguj się
nav-release-notes = Zobacz informacje o wersji

# Timezone mismatch banner (templates/dashboard_base.html)

tz-banner-text = Strefa czasowa twojej przeglądarki to { $detected }, ale twoja strefa rezerwacji jest ustawiona na { $current }.
tz-banner-update = Zaktualizuj
tz-banner-dismiss = Zamknij

# Markdown editor toolbar (templates/dashboard_base.html)

editor-link-prompt = Podaj adres URL:
editor-link-default-label = tekst linku
editor-placeholder-text = tekst
editor-nothing-to-preview = Nie ma czego podejrzeć

# Dashboard overview (templates/dashboard_overview.html)

overview-page-title = Panel
overview-welcome = Witaj, { $name }
overview-public-page = Strona publiczna:
overview-avail-banner-title = Domyślna dostępność
overview-avail-banner-body = Twoje domyślne godziny pracy ustawiono na poniedziałek–piątek, 9:00–17:00. Są używane, gdy inni dodają cię do dynamicznych spotkań grupowych.
overview-avail-banner-cta = Sprawdź swoją dostępność
overview-dismiss = Zamknij
overview-getting-started = Pierwsze kroki
overview-getting-started-help = Wykonaj te kroki, aby zacząć przyjmować rezerwacje.
overview-step-connect-calendar = Podłącz kalendarz
overview-step-first-event-type = Utwórz swój pierwszy typ wydarzenia
overview-step-share-link = Udostępnij swój link do rezerwacji
overview-pending-approval = Oczekuje na zatwierdzenie
overview-booking-with = { $title } z { $guest }
overview-badge-pending = oczekuje
overview-guest-booked = Zarezerwowane przez gościa:
overview-confirm = Potwierdź
overview-decline = Odrzuć
overview-stat-event-types = Typy wydarzeń
overview-stat-upcoming = Nadchodzące rezerwacje
overview-stat-pending = Oczekujące na zatwierdzenie
overview-stat-sources = Źródła kalendarzy
overview-quick-actions = Utwórz typ wydarzenia
overview-action-public-title = Publiczna strona rezerwacji
overview-action-public-desc = Udostępnij link — każdy może wybrać termin i zarezerwować czas u ciebie.
overview-action-team-title = Planowanie zespołowe
overview-action-team-desc = Rozdzielaj rezerwacje między członków zespołu albo znajdź termin, w którym wszyscy są wolni.
overview-action-team-desc-empty = Najpierw utwórz zespół, a potem skonfiguruj wspólne typy wydarzeń.
overview-action-private-title = Prywatne, tylko z zaproszeniem
overview-action-private-desc = Generuj jednorazowe linki dla wybranych osób. Nikt inny nie zarezerwuje.
overview-action-shared-title = Udostępnione linki z zaproszeniem
overview-action-shared-desc = Każdy współpracownik w zespole może wygenerować link do rezerwacji i udostępnić go na zewnątrz.
overview-action-reason-calendar = Najpierw podłącz kalendarz
overview-action-reason-ask-admin = Poproś administratora o utworzenie zespołu
overview-action-reason-team-admin = Wymaga zespołu — najpierw utwórz jakiś
overview-action-reason-team-member = Wymaga zespołu — poproś administratora

# Dashboard bookings (templates/dashboard_bookings.html)

bookings-page-title = Rezerwacje
bookings-pending-approval = Oczekuje na zatwierdzenie
bookings-available-to-claim = Do przejęcia
bookings-upcoming = Nadchodzące rezerwacje
bookings-with = { $title } z { $guest }
bookings-guest-booked = Zarezerwowane przez gościa:
bookings-resource = Zasób:
bookings-confirm = Potwierdź
bookings-reschedule = Przenieś
bookings-decline = Odrzuć
bookings-claim = Przejmij
bookings-badge-awaiting-reschedule = oczekuje na przeniesienie
bookings-cancel = Odwołaj
bookings-reason-placeholder = Powód (opcjonalnie)
bookings-confirm-cancel = Potwierdź odwołanie
bookings-back = Wstecz
bookings-empty = Nie ma jeszcze nadchodzących rezerwacji.<br>Udostępnij swoje { $link }, aby inni mogli rezerwować czas u ciebie.
bookings-empty-link-label = linki do typów wydarzeń

# Dashboard teams listing (templates/dashboard_teams.html)

teams-page-title = Zespoły
teams-heading = Zespoły
teams-new = Nowy
teams-badge-public = publiczny
teams-badge-private = prywatny
teams-settings = Ustawienia
teams-view = Zobacz
teams-empty = Nie ma jeszcze zespołów.
teams-empty-admin = { $link }, aby współpracować ze swoim zespołem.
teams-empty-admin-link-label = Utwórz jakiś
teams-empty-member = Zespoły tworzą administratorzy. Poproś ich o utworzenie zespołu i dodanie cię jako członka.

# Dashboard invite links (templates/dashboard_internal.html)

invite-links-page-title = Linki z zaproszeniem
invite-links-heading = Linki z zaproszeniem
invite-links-new = Nowe wydarzenie wewnętrzne
invite-links-help = Generuj jednorazowe linki do rezerwacji dla wewnętrznych typów wydarzeń. Każdy zalogowany współpracownik może tu tworzyć i udostępniać linki.
invite-links-duration = { $minutes } min
invite-links-hosted-by = Prowadzi { $host }
invite-links-get-link = Pobierz link
invite-links-invites = Zaproszenia
invite-links-empty = Nie ma jeszcze wewnętrznych typów wydarzeń.<br>{ $link } z widocznością „Wewnętrzny”, aby każdy współpracownik mógł generować linki do rezerwacji.
invite-links-empty-link-label = Utwórz typ wydarzenia
invite-links-js-generating = Generowanie...
invite-links-js-copied = Skopiowano!
invite-links-js-error = Błąd

teams-member-count =
    { $count ->
        [one] { $count } członek
        [few] { $count } członkowie
        [many] { $count } członków
       *[other] { $count } członków
    }

# Dashboard calendar sources (templates/dashboard_sources.html)

sources-page-title = Źródła kalendarzy
sources-heading = Źródła kalendarzy
sources-add = Dodaj
sources-last-sync = Ostatnia synchronizacja:
sources-sync = Synchronizuj
sources-full-resync = Pełna resynchronizacja
sources-full-resync-title = Wyczyść pamięć podręczną i pobierz ponownie wszystkie wydarzenia z serwera
sources-test = Testuj
sources-reconnect = Połącz ponownie
sources-reconnect-title = Powtórz proces zgody Google
sources-edit = Edytuj
sources-remove = Usuń
sources-remove-confirm = Usunąć źródło „{ $name }”? Skasuje to wszystkie wydarzenia zsynchronizowane z tego źródła.
sources-no-write-calendar = Nie wybrano kalendarza do zapisu. Potwierdzone rezerwacje zostają w calrs i nie trafiają do tego kalendarza. Wybierz jeden poniżej, aby włączyć zapis.
sources-write-bookings-to = Zapisuj rezerwacje w:
sources-write-none = Żaden (nie zapisuj)
sources-empty = Nie podłączono żadnych źródeł kalendarza. { $link }, aby sprawdzać dostępność.
sources-empty-link-label = Dodaj jedno

# Dashboard event types listing (templates/dashboard_event_types.html)

event-types-page-title = Typy wydarzeń
event-types-heading = Typy wydarzeń
event-types-new = Nowy
event-types-badge-disabled = wyłączony
event-types-badge-internal = wewnętrzny
event-types-badge-private = prywatny
event-types-badge-resources = zasoby
event-types-send-invites = Wyślij zaproszenia
event-types-duration = { $minutes } min
event-types-mode-collective = zbiorowy
event-types-mode-round-robin = rotacyjny
event-types-edit = Edytuj
event-types-disable = Wyłącz
event-types-enable = Włącz
event-types-embed = Osadź
event-types-overrides = Wyjątki
event-types-team-settings = Ustawienia zespołu
event-types-invites = Zaproszenia
event-types-view-public = Zobacz stronę publiczną
event-types-view-page = Zobacz stronę
event-types-delete = Usuń
event-types-delete-confirm = Usunąć typ wydarzenia „{ $title }”? Tej operacji nie można cofnąć.
event-types-empty = Nie ma jeszcze typów wydarzeń. { $link }, aby zacząć przyjmować rezerwacje.
event-types-empty-link-label = Utwórz jakiś

# Markdown editor toolbar (templates/settings.html, templates/team_form.html)

editor-bold = Pogrubienie (Ctrl+B)
editor-italic = Kursywa (Ctrl+I)
editor-strikethrough = Przekreślenie
editor-code = Kod w tekście
editor-link = Wstaw link (Ctrl+K)
editor-toggle-preview = Pokaż lub ukryj podgląd
editor-preview = Podgląd

# Profile and settings (templates/settings.html)

settings-page-title = Ustawienia
settings-heading = Profil i ustawienia
settings-public-page-label = Twoja publiczna strona rezerwacji
settings-copy = Kopiuj
settings-copied = Skopiowano!
settings-open = Otwórz
settings-avatar = Awatar
settings-upload = Prześlij
settings-remove = Usuń
settings-display-name = Wyświetlana nazwa
settings-display-name-placeholder = Twoje imię i nazwisko
settings-username = Nazwa użytkownika
settings-username-hint = (używana w twoim adresie rezerwacji)
settings-username-pattern-title = Tylko małe litery, cyfry i myślniki
settings-username-help = Twoja publiczna strona rezerwacji:
settings-title = Stanowisko
settings-title-placeholder = np. Inżynierka oprogramowania, Menedżer produktu
settings-title-help = Widoczne na twoim profilu publicznym i na pasku bocznym.
settings-bio = Opis
settings-bio-placeholder = Napisz coś o sobie...
settings-bio-help = Widoczne na twojej publicznej stronie rezerwacji. Obsługuje **pogrubienie**, *kursywę*, ~~przekreślenie~~, `kod` i [linki](url).
settings-booking-email = E-mail do rezerwacji
settings-booking-email-help = Ten adres pojawi się na twoich publicznych stronach rezerwacji i w powiadomieniach e-mail. Zostaw puste, aby użyć adresu logowania.
settings-booking-email-warning = Upewnij się, że ten adres istnieje u twojego dostawcy poczty. W przeciwnym razie powiadomienia nie zostaną dostarczone.
settings-timezone = Strefa czasowa
settings-timezone-help = Twoje reguły dostępności i godziny rezerwacji są liczone w tej strefie czasowej.
settings-language = Język
settings-language-auto = Automatycznie (język przeglądarki)
settings-language-help = Wybierz język interfejsu albo zostaw Automatycznie, aby podążać za ustawieniem przeglądarki.
settings-dynamic-group = Pozwól innym dodawać mnie do dynamicznych linków grupowych
settings-dynamic-group-help = Po włączeniu inni użytkownicy mogą tworzyć doraźne adresy spotkań zbiorowych, które obejmują ciebie (np. { $example }).
settings-lend-resource = Udostępnij mój dostęp do kalendarza na potrzeby rezerwacji zasobów
settings-lend-resource-help = Gdy rezerwacja musi zająć wspólny zasób (laboratorium demo, salę spotkań), do którego twoje konto kalendarza ma prawo zapisu, pozwól calrs użyć do tego twoich zapisanych danych logowania.
settings-default-availability = Domyślna dostępność
settings-default-availability-help = Twoje domyślne godziny pracy. Używane w dynamicznych linkach grupowych, gdy inni dodają cię do spotkania.
settings-copy-to-all = Skopiuj na wszystkie dni
settings-copy-to-all-title = Skopiuj przedziały z pierwszego włączonego dnia na wszystkie pozostałe włączone dni
settings-add-window = Dodaj przedział godzinowy
settings-remove-window = Usuń przedział
settings-save = Zapisz ustawienia
settings-appearance = Wygląd
settings-theme-system = Systemowy
settings-theme-light = Jasny
settings-theme-dark = Ciemny

# Sign in (templates/auth/login.html)

login-page-title = Logowanie
login-heading = Zaloguj się
login-subtitle = Zaloguj się na swoje konto calrs
login-sso = Zaloguj się przez SSO
login-or = lub
login-email = E-mail
login-password = Hasło
login-submit = Zaloguj się e-mailem
login-no-account = Nie masz jeszcze konta? { $link }
login-register-link = Zarejestruj się

# Registration (templates/auth/register.html)

register-page-title = Rejestracja
register-heading = Utwórz konto
register-subtitle = Zarejestruj nowe konto calrs
register-domains-limited = Rejestracja jest ograniczona do: { $domains }
register-name = Imię i nazwisko
register-name-placeholder = Twoje imię i nazwisko
register-email = E-mail
register-password = Hasło
register-password-hint = (min. 12 znaków)
register-submit = Utwórz konto
register-have-account = Masz już konto? { $link }
register-signin-link = Zaloguj się

# Authentication errors (src/auth.rs)

auth-error-rate-limited = Zbyt wiele prób logowania. Proszę spróbować ponownie później.
auth-error-invalid-credentials = Nieprawidłowy e-mail lub hasło
auth-error-internal = Błąd wewnętrzny
auth-error-registration-disabled = Rejestracja jest wyłączona.
auth-error-name-length = Imię i nazwisko muszą mieć od 1 do 255 znaków
auth-error-email-length = Adres e-mail musi mieć od 1 do 255 znaków
auth-error-email-invalid = Proszę podać prawidłowy adres e-mail
auth-error-email-domain = Domena e-mail jest niedozwolona
auth-error-password-length = Hasło musi mieć co najmniej 12 znaków
auth-error-email-taken = Ten adres e-mail jest już zarejestrowany
auth-error-create-failed = Nie udało się utworzyć konta

# Calendar source test and write-back setup (templates/source_test.html, templates/source_write_setup.html)

source-test-page-title = Źródło kalendarza
source-test-sync-heading = Synchronizacja: { $name }
source-test-heading = Test połączenia
source-write-page-title = Skonfiguruj zapis do kalendarza
source-write-back = Wróć do panelu
source-write-heading = Gdzie mają trafiać rezerwacje?
source-write-help = Gdy ktoś zarezerwuje z tobą spotkanie, calrs może automatycznie utworzyć wydarzenie w twoim kalendarzu. Wybierz, do którego kalendarza zapisywać rezerwacje dla { $name }.
source-write-save = Zapisz
source-write-skip = Pomiń na razie
source-write-sync-results = Wyniki synchronizacji

source-write-event-count =
    { $count ->
        [one] { $count } wydarzenie
        [few] { $count } wydarzenia
        [many] { $count } wydarzeń
       *[other] { $count } wydarzeń
    }

# Date overrides (templates/overrides.html)

overrides-page-title = Wyjątki dat
overrides-heading = Wyjątki dat
overrides-back-teams = Wróć do zespołów
overrides-back-event-types = Wróć do typów wydarzeń
overrides-intro = Dodaj wyjątki dla konkretnych dat w { $title }
overrides-add-heading = Dodaj nowy wyjątek
overrides-date = Data
overrides-type = Rodzaj wyjątku
overrides-type-blocked = Zablokuj cały dzień
overrides-type-custom = Własne godziny
overrides-start-time = Godzina rozpoczęcia
overrides-end-time = Godzina zakończenia
overrides-add-submit = Dodaj wyjątek
overrides-existing = Istniejące wyjątki
overrides-badge-blocked = zablokowany
overrides-badge-custom = własne godziny
overrides-delete = Usuń
overrides-delete-confirm = Usunąć ten wyjątek?
overrides-empty = Nie ma jeszcze wyjątków dat.<br>Użyj formularza powyżej, aby zablokować konkretne dni (święta, dni wolne) albo ustawić własne godziny.

# Public team page (templates/team_profile.html)

team-profile-subtitle = Wybierz typ wydarzenia, aby zarezerwować termin.
team-profile-empty = Nie ma jeszcze dostępnych typów wydarzeń.

# Availability troubleshoot (templates/troubleshoot.html, src/web/mod.rs)

troubleshoot-page-title = Diagnostyka
troubleshoot-empty = Nie znaleziono typów wydarzeń. { $link }, aby zacząć diagnozować dostępność.
troubleshoot-empty-link-label = Utwórz jakiś
troubleshoot-subtitle = Sprawdź, dlaczego terminy dla { $title } są dostępne albo zablokowane
troubleshoot-duration = { $minutes } min
troubleshoot-buffer-before = { $minutes } min buforu przed
troubleshoot-buffer-after = { $minutes } min buforu po
troubleshoot-min-notice = { $minutes } min wyprzedzenia
troubleshoot-blocked-override = Zablokowane przez wyjątek daty (dzień wolny)
troubleshoot-custom-hours-active = Aktywny wyjątek z własnymi godzinami (zastępuje reguły tygodniowe)
troubleshoot-legend-available = Dostępne
troubleshoot-legend-calendar-event = Wydarzenie z kalendarza
troubleshoot-legend-booking = Rezerwacja
troubleshoot-legend-resource = Zasób zajęty
troubleshoot-legend-outside = Poza godzinami
troubleshoot-legend-buffer = Bufor / minimalne wyprzedzenie
troubleshoot-blocked-slots = Zablokowane terminy
troubleshoot-none-date-blocked = Ta data jest zablokowana przez wyjątek dostępności (dzień wolny). Brak dostępnych terminów.
troubleshoot-none-custom-hours = Aktywny jest wyjątek z własnymi godzinami, ale żaden przedział nie pasuje. Sprawdź ustawienia wyjątku.
troubleshoot-none-no-rules = Brak reguł dostępności dla tego dnia tygodnia. Tego typu wydarzenia nie można zarezerwować { $date }.
troubleshoot-none-all-bookable = Brak zablokowanych terminów w godzinach dostępności. Można rezerwować o każdej porze.
troubleshoot-label-outside = Poza dostępnością
troubleshoot-label-available = Dostępne
troubleshoot-label-min-notice = Minimalne wyprzedzenie ({ $minutes } min)
troubleshoot-label-beyond-horizon = Poza horyzontem rezerwacji ({ $days } dni)
troubleshoot-label-buffer = Bufor ({ $minutes } min)
troubleshoot-label-resource-busy = Zasób zajęty: { $names }
troubleshoot-detail-around = Wokół: { $label }
troubleshoot-detail-around-booking = Wokół rezerwacji od { $guest }
troubleshoot-reason-calendar-event = Wydarzenie z kalendarza: { $label }
troubleshoot-reason-booking = Rezerwacja: { $label }

# Invite management (templates/invite_form.html)

invites-heading = Zaproszenia
invites-back-teams = Wróć do zespołów
invites-back-event-types = Wróć do typów wydarzeń
invites-intro = Wyślij linki z zaproszeniem do { $title }
invites-capped = <strong>Wpis ograniczono do { $max } odbiorców na jedno wysłanie.</strong> Resztę wyślij w kolejnej turze.
invites-failed-hint = — szczegóły znajdziesz w logach serwera.
invites-quick-link = Szybki link
invites-quick-link-help = Wygeneruj jednorazowy link i skopiuj go do schowka.
invites-get-link = Pobierz link
invites-or-email = Albo wyślij e-mailem
invites-recipients = Odbiorcy
invites-recipients-hint = (jeden adres w wierszu, maksymalnie { $max })
invites-message = Wiadomość osobista
invites-message-hint = (opcjonalnie, trafia do wszystkich odbiorców)
invites-message-placeholder = Chętnie pokażę ci demo...
invites-expires-in = Wygasa za
invites-expires-days = { $days } dni
invites-expires-never = Nigdy
invites-allow-multiple = Zezwól na wiele rezerwacji na odbiorcę
invites-send = Wyślij zaproszenia
invites-sent-heading = Wysłane zaproszenia
invites-badge-expired = wygasłe
invites-badge-used = wykorzystane
invites-badge-active = aktywne
invites-sent-by = Wysłane przez { $name }
invites-uses = { $used }/{ $max } użyć
invites-expires-at = Wygasa { $date }
invites-copy-link = Kopiuj link
invites-delete = Usuń
invites-delete-confirm = Usunąć to zaproszenie?
invites-empty = Nie wysłano jeszcze żadnych zaproszeń. Użyj formularza powyżej, aby przesłać komuś link do rezerwacji.
invites-js-generating = Generowanie...
invites-js-copied = Skopiowano!
invites-js-error = Błąd

invites-sent-count =
    { $count ->
        [one] Wysłano { $count } zaproszenie.
        [few] Wysłano { $count } zaproszenia.
        [many] Wysłano { $count } zaproszeń.
       *[other] Wysłano { $count } zaproszeń.
    }

invites-skipped-invalid =
    { $count ->
        [one] Pominięto { $count } nieprawidłowy wiersz:
        [few] Pominięto { $count } nieprawidłowe wiersze:
        [many] Pominięto { $count } nieprawidłowych wierszy:
       *[other] Pominięto { $count } nieprawidłowych wierszy:
    }

invites-skipped-duplicate =
    { $count ->
        [one] Pominięto { $count } zduplikowany wiersz:
        [few] Pominięto { $count } zduplikowane wiersze:
        [many] Pominięto { $count } zduplikowanych wierszy:
       *[other] Pominięto { $count } zduplikowanych wierszy:
    }

invites-failed =
    { $count ->
        [one] { $count } zaproszenie nie powiodło się (baza lub SMTP):
        [few] { $count } zaproszenia nie powiodły się (baza lub SMTP):
        [many] { $count } zaproszeń nie powiodło się (baza lub SMTP):
       *[other] { $count } zaproszeń nie powiodło się (baza lub SMTP):
    }

# Calendar source form (templates/source_form.html)

source-form-title-edit = Edytuj źródło kalendarza
source-form-title-add = Dodaj kalendarz
source-form-heading-edit = Edytuj źródło kalendarza
source-form-heading-add = Podłącz kalendarz
source-form-subtitle-edit = Zaktualizuj połączenie. Zostaw hasło puste, aby zachować dotychczasowe. Po zmianie adresu lub nazwy użytkownika uruchom synchronizację, aby odświeżyć listę wykrytych kalendarzy.
source-form-subtitle-add = Podłącz serwer CalDAV albo Microsoft Exchange (EWS), aby calrs mógł sprawdzać dostępność, gdy goście rezerwują spotkania.
source-form-backend = Backend
source-form-preset = Ustawienie wstępne
source-form-connect-google = Połącz z Google
source-form-google-unavailable = Kalendarz Google jest niedostępny. Skontaktuj się z administracją.
source-form-name = Wyświetlana nazwa
source-form-name-placeholder = Mój kalendarz
source-form-url-caldav = Adres CalDAV
source-form-url-ews = Adres punktu końcowego EWS
source-form-username = Nazwa użytkownika
source-form-password = Hasło
source-form-password-keep = Zostaw puste, aby zachować dotychczasowe
source-form-password-placeholder = Hasło aplikacji albo hasło konta
source-form-skip-test = Pomiń test połączenia
source-form-skip-test-help = Użyj tego, jeśli test się zawiesza (zdarza się w niektórych instalacjach BlueMind lub Zimbry). Połączenie możesz przetestować później.
source-form-save = Zapisz zmiany
source-form-add = Dodaj źródło kalendarza
source-form-help-google-configured = Kliknij przycisk poniżej, aby zezwolić calrs na dostęp do twojego Kalendarza Google.
source-form-help-google-unconfigured = Integracja z Kalendarzem Google nie jest jeszcze skonfigurowana. Poproś administrację o wprowadzenie danych OAuth2 Google w panelu administracyjnym.

# Calendar source form: provider help (templates/source_form.html)

source-form-help-bluemind = <strong>BlueMind</strong> — Użyj punktu końcowego DAV swojego serwera BlueMind.<br> Zwykle: <code>https://mail.yourcompany.com/dav/</code><br> Nazwa użytkownika to twój <strong>adres e-mail</strong> (np. <code>alice@yourcompany.com</code>), a nie sama nazwa logowania.<br> Jeśli test połączenia się zawiesza, zaznacz „Pomiń test połączenia” i zsynchronizuj bezpośrednio.
source-form-help-nextcloud = <strong>Nextcloud</strong> — Użyj katalogu głównego WebDAV, a nie adresu konkretnego kalendarza.<br> Zwykle: <code>https://cloud.example.com/remote.php/dav</code>
source-form-help-fastmail = <strong>Fastmail</strong> — Podaj pełny adres e-mail w ścieżce adresu.<br> Przykład: <code>https://caldav.fastmail.com/dav/calendars/user/you@fastmail.com/</code><br> Użyj hasła aplikacji (Settings &rarr; Privacy &amp; Security &rarr; Integrations).
source-form-help-icloud = <strong>iCloud</strong> — Użyj <code>https://caldav.icloud.com/</code><br> Potrzebujesz hasła aplikacji z <a href="https://appleid.apple.com" target="_blank" style="color: var(--accent);">appleid.apple.com</a> (Bezpieczeństwo &rarr; Hasła aplikacji).
source-form-help-zimbra = <strong>Zimbra</strong> — Użyj punktu końcowego DAV swojego serwera Zimbra.<br> Zwykle: <code>https://mail.example.com/dav/</code>
source-form-help-sogo = <strong>SOGo</strong> — Użyj punktu końcowego DAV SOGo.<br> Zwykle: <code>https://mail.example.com/SOGo/dav/</code>
source-form-help-radicale = <strong>Radicale</strong> — Użyj adresu głównego serwera.<br> Zwykle: <code>https://cal.example.com/</code>
source-form-help-exchange = <strong>Microsoft Exchange (EWS)</strong>. Użyj punktu końcowego SOAP:<br> <code>https://mail.example.com/EWS/Exchange.asmx</code><br> Nazwa użytkownika to adres skrzynki; hasło musi działać z HTTP Basic po TLS (włącz je na skrzynce serwisowej, jeśli w twojej organizacji Basic jest wyłączone).<br> Pamiętaj też, aby wybrać <strong>Microsoft Exchange (EWS)</strong> na liście Backend powyżej.
source-form-help-google = <strong>Kalendarz Google</strong>: połączenie przez OAuth2. Hasło nie jest potrzebne.<br>
source-form-help-other = Podaj <strong>główny adres DAV</strong> swojego serwera CalDAV — nie adres pojedynczego kalendarza ani link publiczny.<br> calrs sam wykryje twoje kalendarze przez PROPFIND (RFC 4791).

# Markdown editor toolbar, short labels (templates/team_form.html, templates/team_settings.html)

editor-bold-short = Pogrubienie
editor-italic-short = Kursywa
editor-link-short = Wstaw link

# Team creation (templates/team_form.html)

team-form-heading = Nowy zespół
team-form-name = Nazwa zespołu
team-form-name-placeholder = Inżynieria
team-form-slug = Identyfikator
team-form-slug-hint = (identyfikator przyjazny adresom URL)
team-form-slug-pattern-title = Tylko małe litery, cyfry i myślniki
team-form-description = Opis
team-form-optional = (opcjonalnie)
team-form-description-placeholder = Czym zajmuje się ten zespół...
team-form-description-help = Widoczny na stronie zespołu. Obsługuje **pogrubienie**, *kursywę* i [linki](url).
team-form-visibility = Widoczność
team-form-public = Publiczny
team-form-private = Prywatny
team-form-visibility-help = Zespoły prywatne dostają token zaproszenia do udostępniania. Publiczne są widoczne na stronie profilu zespołu.
team-form-members = Członkowie
team-form-members-help = Zostaniesz automatycznie dodany jako administrator zespołu. Dodaj pojedynczych użytkowników albo powiąż grupy OIDC.
team-form-search-placeholder = Szukaj użytkowników lub grup...
team-form-search-users = Użytkownicy
team-form-search-groups = Grupy OIDC
team-form-you = (ty)
team-form-submit = Utwórz zespół

# Team settings (templates/team_settings.html)

team-settings-page-title = Ustawienia
team-settings-subtitle = Ustawienia zespołu — mogą je edytować administratorzy zespołu.
team-settings-public-url = Adres publiczny
team-settings-public-url-help = Przez ten link może rezerwować każdy.
team-settings-invite-link = Link z zaproszeniem
team-settings-invite-link-help = Udostępnij ten link, aby dać dostęp do strony rezerwacji tego prywatnego zespołu.
team-settings-avatar = Awatar zespołu
team-settings-profile = Profil
team-settings-description-placeholder = Opowiedz o tym zespole...
team-settings-description-help = Widoczny na publicznej stronie rezerwacji zespołu. Obsługuje **pogrubienie**, *kursywę* i [linki](url).
team-settings-visibility-help = Zespoły publiczne są widoczne na stronie profilu zespołu. Prywatne wymagają linku z zaproszeniem.
team-settings-members-help = Zarządzaj składem tego zespołu. Dodaj pojedynczych użytkowników albo powiąż grupy OIDC, aby synchronizowały się automatycznie.
team-settings-role-member = Członek
team-settings-role-admin = Administrator
team-settings-oidc-group = Grupa OIDC
team-settings-remove = Usuń
team-settings-save = Zapisz zmiany
team-settings-danger-zone = Strefa zagrożenia
team-settings-danger-help = Trwale usuń ten zespół. Typy wydarzeń zostaną odłączone, a nie usunięte. Tej operacji nie można cofnąć.
team-settings-delete = Usuń ten zespół
team-settings-delete-confirm = Usunąć zespół „{ $name }”? Tej operacji nie można cofnąć.

# Event type form (templates/event_type_form.html)

etf-heading-edit = Edytuj typ wydarzenia
etf-heading-new = Nowy typ wydarzenia
etf-team = Zespół
etf-team-hint = (opcjonalnie — zostaw puste dla osobistego typu wydarzenia)
etf-team-personal = Osobisty
etf-scheduling-mode = Tryb przydzielania
etf-mode-round-robin = Rotacyjny — przydziel jednemu dostępnemu członkowi
etf-mode-collective = Zbiorowy — wszyscy członkowie muszą być dostępni
etf-scheduling-mode-help = Tryb rotacyjny przydziela rezerwację jednemu dostępnemu członkowi (najpierw najmniej obciążonemu). Tryb zbiorowy wymaga, aby wszyscy członkowie byli wolni w tym samym czasie.
etf-title = Tytuł
etf-title-placeholder = 30-minutowa rozmowa wstępna
etf-slug = Identyfikator
etf-slug-placeholder = tworzony automatycznie z tytułu
etf-description-placeholder = Krótka rozmowa wstępna, żeby omówić...
etf-description-help = Widoczny na stronie rezerwacji. Obsługuje **pogrubienie**, *kursywę* i [linki](url).
etf-location = Miejsce
etf-location-link = Rozmowa wideo (stały adres)
etf-location-jitsi = Jitsi (pokój tworzony automatycznie)
etf-location-webhook = Webhook (własny dostawca)
etf-location-phone = Telefon
etf-location-in-person = Osobiście
etf-location-custom = Własne
etf-location-details = Szczegóły
etf-location-details-placeholder = https://meet.example.com/moj-pokoj
etf-pattern-placeholder = Zostaw puste, aby użyć domyślnego wzorca organizacji
etf-duration = Czas trwania (minuty)
etf-slot-interval = Odstęp między terminami (minuty)
etf-slot-interval-placeholder = Tak jak czas trwania
etf-slot-interval-help = Jak często zaczynają się terminy. Zostaw puste, aby odpowiadały czasowi trwania.
etf-required-members = Wymagani członkowie
etf-required-members-help = Wszyscy zaznaczeni członkowie muszą być wolni, aby termin został zaproponowany. Odznacz osoby, które chcesz wykluczyć (ich dostępność zostanie pominięta).
etf-member-priority = Priorytet członków
etf-member-priority-help = Członkowie z wyższym priorytetem dostają rezerwacje w pierwszej kolejności, jeśli są dostępni. Przy równym priorytecie decyduje liczba ostatnich rezerwacji.
etf-member-timezone-title = Strefa czasowa członka. Jego osobiste godziny pracy są liczone w tej strefie.
etf-priority-high = Wysoki
etf-priority-medium = Średni
etf-priority-low = Niski
etf-section-availability = Dostępność
etf-timezone-help = Godziny poniżej są liczone w tej strefie czasowej. Przy zespołowych typach wydarzeń wybierz strefę pracy zespołu (niekoniecznie strefę osoby tworzącej).
etf-reset-default = Przywróć moje domyślne
etf-reset-default-title = Zastąp te godziny domyślną dostępnością z twojego profilu
etf-availability-prefilled = Wypełnione na podstawie twojej { $link }. Możesz to tutaj zmienić dla tego typu wydarzenia.
etf-availability-prefilled-link = domyślnej dostępności
etf-section-buffers = Bufory i wyprzedzenie
etf-buffer-before = Bufor przed (min)
etf-buffer-after = Bufor po (min)
etf-min-notice = Minimalne wyprzedzenie
etf-min-notice-help = Z jakim wyprzedzeniem trzeba rezerwować.
etf-section-limits = Limity rezerwacji
etf-first-slot-only = Jeden termin dziennie
etf-first-slot-only-help = Pokazuj tylko najwcześniejszy dostępny termin każdego dnia.
etf-freq-limit = Ogranicz częstotliwość rezerwacji
etf-freq-limit-help = Ogranicz, ile razy w danym okresie można zarezerwować to wydarzenie.
etf-add-limit = Dodaj limit
etf-section-options = Opcje rezerwacji
etf-requires-confirmation = Wymaga potwierdzenia
etf-requires-confirmation-help = Rezerwacje pozostaną oczekujące, dopóki nie zatwierdzisz ich w panelu.
etf-sms = Powiadomienia SMS
etf-sms-off = Wyłączone, nie pytamy o numer
etf-sms-optional = Opcjonalne, goście mogą podać numer
etf-sms-required = Wymagane, goście muszą podać numer
etf-sms-help = Wysyła gościowi SMS, oprócz e-maila, gdy jego rezerwacja zostanie potwierdzona, przeniesiona, odwołana albo ma się zaraz zacząć. Kto zostawi pole puste, po prostu nie dostanie SMS-a. Wymaga bramki SMS w { $link }.
etf-admin-panel-link = panelu administracyjnym
etf-additional-guests = Dodatkowi goście
etf-guests-none = Goście nie mogą dodawać innych osób
etf-additional-guests-help = Pozwól osobie rezerwującej zaprosić kolejnych uczestników, którzy dostaną zaproszenie do kalendarza.
etf-default-view = Domyślny widok kalendarza
etf-view-month = Miesiąc — siatka kalendarza z listą terminów
etf-view-week = Tydzień — kolumny dla 7 dni z terminami
etf-view-column = Kolumna — dni na liście wraz z terminami
etf-view-week-short = tygodniowy
etf-view-column-short = kolumnowy
etf-default-view-help = Widok, który goście zobaczą najpierw. Mogą go zmienić w dowolnej chwili.
etf-conflict-calendars = Kalendarze do sprawdzania kolizji
etf-conflict-calendars-help = Wybierz, które kalendarze sprawdzać pod kątem kolizji. Bez wyboru używane są wszystkie.
etf-no-resources = Nie skonfigurowano jeszcze wspólnych zasobów. Dodaj jakiś (laboratorium demo, salę spotkań) w { $link }, aby móc go tutaj wymagać.
etf-section-access = Dostęp i powiadomienia
etf-visibility-public = Publiczny — widoczny w twoim profilu
etf-visibility-internal = Wewnętrzny — każdy współpracownik może generować linki z zaproszeniem
etf-visibility-private = Prywatny — tylko przez link z zaproszeniem
etf-visibility-help = Określa, kto może zobaczyć i zarezerwować ten typ wydarzenia.
etf-vis-internal = Wewnętrzny
etf-reminder = Przypomnienie o rezerwacji
etf-reminder-none = Bez przypomnienia
etf-reminder-help = Wyślij przypomnienie e-mailem tobie i twojemu gościowi przed spotkaniem.
etf-dynamic-group = Dynamiczny link grupowy
etf-dynamic-group-help = Utwórz doraźny link do spotkania, który sprawdza dostępność twoją i innych użytkowników.
etf-dynamic-group-search = Znajdź użytkownika do dodania...
etf-dynamic-group-note = Pokazywani są tylko użytkownicy, którzy zezwalają na dynamiczne linki grupowe.
etf-dynamic-group-url = Adres linku grupowego
etf-watcher-teams = Zespoły obserwujące
etf-watcher-teams-help = Wybrane zespoły dostaną powiadomienie o każdej rezerwacji. Ich członkowie mogą przejąć rezerwację, aby w niej uczestniczyć.
etf-save = Zapisz zmiany
etf-create = Utwórz typ wydarzenia
etf-js-loading = Wczytywanie...
etf-js-no-default = Brak wartości domyślnej
etf-js-reset-done = Przywrócono!
etf-js-error = Błąd
etf-js-remove-limit = Usuń limit
etf-period-day = Dziennie
etf-period-week = Tygodniowo
etf-period-month = Miesięcznie
etf-period-year = Rocznie

# Event type form: runtime summary hints (templates/event_type_form.html)


# %1 and %2 are substituted client-side; the values are only known once a field is edited.

etf-hint-no-days = Nie ustawiono dni
etf-hint-every-day = Codziennie
etf-fmt-day-one = %1 dzień
etf-fmt-day-other = %1 dni
etf-fmt-hours = %1 godz.
etf-fmt-minutes = %1 min
etf-hint-buffer-both = %1 min przed, %2 min po
etf-hint-buffer-before = %1 min buforu przed
etf-hint-buffer-after = %1 min buforu po
etf-hint-notice = %1 wyprzedzenia
etf-hint-no-buffers = Bez buforów, można rezerwować o każdej porze
etf-hint-max = Maks. %1
etf-hint-period-day = /dzień
etf-hint-period-week = /tydzień
etf-hint-period-month = /miesiąc
etf-hint-period-year = /rok
etf-hint-no-limits = Bez limitów
etf-hint-confirmation-required = Wymaga potwierdzenia
etf-hint-auto-confirmed = Potwierdzane automatycznie
etf-hint-extra-guests-one = do %1 dodatkowego gościa
etf-hint-extra-guests-other = do %1 dodatkowych gości
etf-hint-view = widok %1
etf-hint-reminder = przypomnienie %1 wcześniej
etf-hint-no-reminder = bez przypomnienia

etf-guests-up-to =
    { $count ->
        [one] Do { $count } dodatkowego gościa
        [few] Do { $count } dodatkowych gości
        [many] Do { $count } dodatkowych gości
       *[other] Do { $count } dodatkowych gości
    }

etf-reminder-hours =
    { $count ->
        [one] { $count } godzinę wcześniej
        [few] { $count } godziny wcześniej
        [many] { $count } godzin wcześniej
       *[other] { $count } godzin wcześniej
    }

etf-reminder-days =
    { $count ->
        [one] { $count } dzień wcześniej
        [few] { $count } dni wcześniej
        [many] { $count } dni wcześniej
       *[other] { $count } dni wcześniej
    }

# Event type form: preset banners and meeting-pattern help (templates/event_type_form.html)
# Literal braces are escaped as {"{"} because Fluent reads a bare { as a placeable.

etf-preset-public = Tworzysz <strong>publiczny</strong> typ wydarzenia &mdash; zarezerwuje każdy, kto ma link.
etf-preset-private = Tworzysz <strong>prywatny</strong> typ wydarzenia &mdash; zarezerwują tylko osoby, które zaprosisz.
etf-preset-internal = Tworzysz <strong>wewnętrzny</strong> typ wydarzenia &mdash; każdy współpracownik może udostępnić link do rezerwacji.
etf-preset-team = Tworzysz <strong>zespołowy</strong> typ wydarzenia &mdash; rezerwacje są rozdzielane między członków zespołu.
etf-pattern-hint = Opcjonalny własny wzorzec. Znaczniki: <code>{"{"}username{"}"}</code>, <code>{"{"}event{"}"}</code>, <code>{"{"}date{"}"}</code>, <code>{"{"}random{"}"}</code>. Zostaw puste, aby użyć domyślnego wzorca organizacji ustawionego przez administrację.
etf-pattern-random-warning = Ten wzorzec nie zawiera znacznika <code>{"{"}random{"}"}</code>. Dwie rezerwacje tego typu wydarzenia w tym samym dniu trafią do tego samego pokoju, a drugi gość może wejść na spotkanie pierwszego. Stałych pokojów używaj tylko wtedy, gdy właśnie o to ci chodzi.
etf-webhook-hint = Adres spotkania dla każdej rezerwacji pobierany jest z webhooka skonfigurowanego przez administrację w Administracja &rarr; Webhook spotkań. Tutaj adres nie jest potrzebny.

# Admin panel (templates/admin.html)

admin-page-title = Administracja
admin-heading = Panel administracyjny
admin-action-refused = Odmowa wykonania:
admin-logo = Logo firmy
admin-logo-help = Wyświetlane na publicznych stronach rezerwacji. Zalecane: PNG lub SVG, maks. 2 MB.
admin-company-link = Link firmowy
admin-company-link-help = Na publicznych stronach rezerwacji logo prowadzi pod ten adres. Zostaw puste, aby nie tworzyć linku.
admin-theme = Motyw
admin-theme-help = Wybierz motyw kolorystyczny dla wszystkich stron. Przełącznik jasny/ciemny działa niezależnie — motywy dostosowują się do obu trybów.
admin-theme-default = Domyślny
admin-theme-default-desc = Czysty błękit
admin-theme-nord-desc = Arktyczny szron
admin-theme-dracula-desc = Ciemny fiolet
admin-theme-gruvbox-desc = Ciepłe retro
admin-theme-solarized-desc = Klasyk Ethana
admin-theme-tokyo-desc = Neonowe miasto
admin-theme-custom = Własny
admin-theme-custom-desc = Twoje kolory
admin-custom-colors = Własne kolory
admin-color-accent = Kolor akcentu
admin-color-accent-hover = Akcent po najechaniu
admin-color-bg = Tło
admin-color-surface = Powierzchnia
admin-color-text = Tekst
admin-save-theme = Zapisz motyw
admin-users = Użytkownicy ({ $count })
admin-user-filter = Filtruj po nazwie lub adresie e-mail…
admin-badge-admin = administrator
admin-badge-disabled = wyłączony
admin-impersonate = Wciel się
admin-demote = Odbierz uprawnienia
admin-promote = Nadaj uprawnienia
admin-disable = Wyłącz
admin-enable = Włącz
admin-delete = Usuń
admin-no-users-match = Żaden użytkownik nie pasuje do filtra.
admin-no-users = Nie ma jeszcze użytkowników.
admin-groups = Grupy ({ $count })
admin-group-filter = Filtruj po nazwie grupy…
admin-group-name = Nazwa grupy
admin-weight = waga:
admin-no-groups-match = Żadna grupa nie pasuje do filtra.
admin-no-groups = Nie zsynchronizowano jeszcze żadnych grup. Grupy są pobierane automatycznie od twojego dostawcy OIDC.
admin-auth-settings = Ustawienia logowania
admin-registration-enabled = Rejestracja włączona
admin-allowed-domains = Dozwolone domeny e-mail
admin-allowed-domains-hint = (oddzielone przecinkami, puste oznacza wszystkie)
admin-save-auth = Zapisz ustawienia logowania
admin-system-settings = Ustawienia systemowe
admin-base-url = Adres bazowy
admin-base-url-help = Publiczny adres tej instancji. Używany przy przekierowaniach OIDC i w linkach w e-mailach (zatwierdź/odrzuć, odwołaj, przypomnienia).
admin-private-hosts = Lista dozwolonych hostów prywatnych
admin-private-hosts-help = Nazwy hostów, oddzielone przecinkami, którym wolno wskazywać prywatne lub zarezerwowane adresy IP w źródłach CalDAV/EWS (wyjątek od ochrony przed SSRF). Dodawaj tylko hosty, które kontrolujesz (na przykład serwer kalendarza w tej samej sieci Dockera). Zostaw puste, aby ochrona działała dla wszystkich hostów.
admin-unset-env = Usuń zmienną środowiskową, aby móc edytować to tutaj.
admin-save-system = Zapisz ustawienia systemowe
admin-status = Stan:
admin-status-enabled = włączone
admin-status-disabled = wyłączone
admin-status-disabled-paren = (wyłączone)
admin-status-configured = skonfigurowane
admin-status-not-configured = nieskonfigurowane
admin-via-environment = (przez środowisko)
admin-issuer = Wystawca:
admin-client-id = Identyfikator klienta:
admin-instance = Instancja:
admin-oidc-settings = Ustawienia OIDC
admin-oidc-enabled = OIDC włączone
admin-issuer-url = Adres wystawcy
admin-client-id-label = Identyfikator klienta
admin-client-secret = Sekret klienta
admin-keep-current-hint = (zostaw puste, aby zachować obecny)
admin-keep-current-set-hint = (zostaw puste, aby zachować obecny — jest już ustawiony)
admin-keep-unchanged = Zostaw puste, aby nic nie zmieniać
admin-oidc-auto-register = Automatycznie rejestruj nowych użytkowników z OIDC
admin-save-oidc = Zapisz ustawienia OIDC
admin-google = Kalendarz Google (OAuth2)
admin-save-google = Zapisz ustawienia OAuth2 Google
admin-captcha = Captcha
admin-instance-url = Adres instancji
admin-site-key = Klucz witryny
admin-secret = Sekret
admin-widget-url = Adres skryptu widżetu
admin-widget-url-help = Zmień, jeśli CDN jest zablokowany. Zmiany działają od razu po zapisaniu.
admin-captcha-disable-help = Zostaw adres instancji, klucz witryny i sekret puste, aby wyłączyć captchę na stronach rezerwacji.
admin-save-captcha = Zapisz ustawienia captchy
admin-resources = Zasoby
admin-resources-help = Wspólne zasoby do rezerwacji (laboratorium demo, sale spotkań) oparte na kanale kalendarza. Po powiązaniu z typami wydarzeń zajęty zasób blokuje rezerwacje.
admin-resource-stats = Wydarzenia w pamięci podręcznej: { $events } &middot; Powiązany z { $attached } typami wydarzeń
admin-never = nigdy
admin-resource-sync-failed = (ostatnia próba nie powiodła się: { $error })
admin-writeback-enabled = Zapis: włączony ({ $via })
admin-writeback-readonly = Zapis: tylko do odczytu
admin-teams-allowed = Dozwolone zespoły:
admin-teams-allowed-none = brak (tylko administratorzy globalni)
admin-sync-now = Synchronizuj teraz
admin-test-write = Przetestuj zapis
admin-delete-resource-confirm = Usunąć ten zasób? Typy wydarzeń, które go używają, przestaną go sprawdzać.
admin-name = Nazwa
admin-name-help = Zostaw puste, aby pobrać nazwę z kanału.
admin-feed-url = Adres kanału ICS (adres publikacji)
admin-feed-url-help = BlueMind: publiczny lub prywatny adres kalendarza danego zasobu.
admin-caldav-url = Adres kolekcji CalDAV (do zapisu)
admin-caldav-url-help = Opcjonalne. W BlueMind jest wyprowadzany automatycznie z adresu kanału.
admin-caldav-username = Nazwa użytkownika CalDAV
admin-caldav-password = Hasło CalDAV
admin-resource-teams = Zespoły uprawnione do korzystania z tego zasobu
admin-resource-teams-help = Administratorzy tych zespołów mogą powiązać zasób ze swoimi zespołowymi typami wydarzeń. Puste: tylko administratorzy globalni.
admin-no-teams = Nie ma jeszcze zespołów.
admin-save-resource = Zapisz zasób
admin-add-resource = Dodaj zasób
admin-jitsi = Jitsi (automatycznie tworzone linki do spotkań)
admin-jitsi-help = Gdy miejscem typu wydarzenia jest „Jitsi (pokój tworzony automatycznie)”, calrs buduje dla każdej rezerwacji nowy adres pokoju, dodając poniższy wzorzec do twojego adresu bazowego Jitsi. Nie jest potrzebne żadne wywołanie zewnętrznego API.
admin-display-name = Wyświetlana nazwa
admin-jitsi-display-name-placeholder = np. Meet DYB
admin-jitsi-display-name-help = Pokazywana gościom w wyborze terminu i w formularzu rezerwacji. Puste oznacza „Rozmowa wideo”.
admin-room-pattern = Wzorzec nazwy pokoju
admin-jitsi-disable-help = Zostaw adres bazowy pusty, aby wyłączyć automatyczne tworzenie linków Jitsi.
admin-save-jitsi = Zapisz ustawienia Jitsi
admin-meeting-webhook = Webhook spotkań (własny dostawca)
admin-webhook-url = Adres webhooka
admin-webhook-display-name-placeholder = np. Zoom, Whereby, Custom Meet
admin-webhook-display-name-help = Pokazywana gościom zamiast ogólnej etykiety „Rozmowa wideo”.
admin-authentication = Uwierzytelnianie
admin-auth-none = Brak
admin-auth-hmac = HMAC-SHA256 (nagłówek X-Calrs-Signature)
admin-shared-secret = Wspólny sekret
admin-webhook-disable-help = Zostaw adres pusty, aby wyłączyć webhook spotkań.
admin-save-webhook = Zapisz ustawienia webhooka
admin-smtp = Ustawienia SMTP
admin-smtp-test-sent = Wysłano testowy e-mail.
admin-smtp-test-failed = Nie udało się wysłać testowego e-maila. Sprawdź logi serwera i swoje ustawienia SMTP.
admin-smtp-env-error = Błąd konfiguracji SMTP ze środowiska:
admin-smtp-host = Host:
admin-smtp-from = Nadawca:
admin-smtp-enabled = SMTP włączone
admin-host = Host
admin-port = Port
admin-tls-mode = Tryb TLS
admin-tls-starttls = STARTTLS (port 587)
admin-tls-implicit = Niejawny TLS (port 465)
admin-tls-none = Brak, bez szyfrowania (tylko lokalny MTA)
admin-smtp-username-hint = (zostaw puste dla przekaźnika bez uwierzytelniania)
admin-from-email = Adres nadawcy
admin-from-name = Nazwa nadawcy
admin-save-smtp = Zapisz ustawienia SMTP
admin-send-test-email = Wyślij testowy e-mail na
admin-send-test-email-hint = (domyślnie adres twojego konta)
admin-send-test-email-btn = Wyślij testowy e-mail
admin-smtp-clear-confirm = Usunąć konfigurację SMTP zapisaną w bazie danych?
admin-clear-db-config = Wyczyść konfigurację z bazy danych
admin-sms = Ustawienia SMS
admin-sms-help = Opcjonalne. SMS-y są wysyłane tylko przy rezerwacjach typów wydarzeń z włączonymi „Powiadomieniami SMS” i tylko wtedy, gdy gość podał numer telefonu.
admin-sms-test-sent = Wysłano wiadomość testową.
admin-sms-test-checked = Dane logowania zaakceptowane.
admin-sms-test-error = Bramka SMS odrzuciła żądanie.
admin-sms-captcha-warning = Formularz rezerwacji jest publiczny, a numer odbiorcy podaje gość, więc SMS bez captchy to otwarty przekaźnik, za który ktoś inny może obciążyć twój rachunek. Skonfiguruj captchę powyżej i ogranicz kraje docelowe w ustawieniach swojej bramki.
admin-sms-sent-today = Wysłane dzisiaj:
admin-sms-of-cap = z { $cap }
admin-sms-config-error = Błąd konfiguracji SMS:
admin-sms-gateway = Bramka:
admin-sms-account = Konto:
admin-sms-sender = Nadawca:
admin-sms-enabled = SMS włączone
admin-sms-gateway-label = Bramka
admin-required-on-switch = Wymagane przy zmianie bramki
admin-sms-docs = Dokumentacja API { $provider }
admin-sms-country = Domyślny numer kierunkowy kraju
admin-sms-country-hint = (używany, gdy goście podają numer lokalny)
admin-sms-daily-cap = Dzienny limit
admin-sms-daily-cap-hint = (wiadomości dziennie dla całej instancji, 0 oznacza brak limitu)
admin-sms-daily-cap-help = Po przekroczeniu limitu calrs przestaje wysyłać SMS-y i nadal wysyła e-maile, więc żadna rezerwacja nie zawiedzie z powodu wyczerpanego budżetu na SMS-y.
admin-save-sms = Zapisz ustawienia SMS
admin-send-test-sms = Wyślij wiadomość testową na
admin-send-test-sms-hint-check = (zostaw puste, aby tylko sprawdzić dane logowania)
admin-send-test-sms-hint-e164 = (format E.164)
admin-test-gateway = Przetestuj bramkę
admin-sms-clear-confirm = Usunąć konfigurację SMS zapisaną w bazie danych?
admin-sms-allow-all = Pozwól każdemu użytkownikowi włączać SMS-y w swoich typach wydarzeń
admin-sms-allow-all-help = Domyślnie wyłączone: SMS-y zużywają środki z konta skonfigurowanego tutaj, więc tylko administratorzy mogą przełączyć typ wydarzenia w tryb SMS.
admin-save-policy = Zapisz zasadę
admin-page-of = Strona %1 z %2
admin-show-more-js = Pokaż o %1 więcej
admin-show-fewer = Pokaż mniej

# Admin panel: strings carrying markup or literal braces (templates/admin.html)

admin-delete-user-confirm = Trwale usunąć użytkownika { $email }?{"\u000A"}{"\u000A"}Usunie to jego konto, profil planowania, źródła kalendarzy, typy wydarzeń oraz wszystkie dane należące wyłącznie do niego. Przeszłe rezerwacje zostaną usunięte razem z jego typami wydarzeń.{"\u000A"}{"\u000A"}W przypadku użytkowników OIDC/SSO: jeśli automatyczna rejestracja jest włączona, ta osoba zostanie utworzona ponownie przy następnym logowaniu.{"\u000A"}{"\u000A"}Tej operacji nie można cofnąć.
admin-system-settings-help = Publiczny adres i ustawienia bezpieczeństwa sieci. Można je też ustawić zmiennymi środowiskowymi <code>CALRS_BASE_URL</code> i <code>CALRS_ALLOW_PRIVATE_HOSTS</code>. Gdy zmienna środowiskowa jest ustawiona, <strong>ma pierwszeństwo</strong> przed wartością poniżej.
admin-set-by-env = — ustawione przez środowisko ({ $var }), ma pierwszeństwo przed zapisaną wartością
admin-google-help = Aby włączyć integrację z Kalendarzem Google, utwórz dane OAuth2 w <a href="https://console.cloud.google.com/apis/credentials" target="_blank" style="color: var(--accent);">Google Cloud Console</a>. Włącz <strong>Google Calendar API</strong>, a następnie dodaj { $redirect_uri } jako dozwolony adres przekierowania.
admin-room-pattern-help = Dostępne znaczniki: <code>{"{"}username{"}"}</code> (gospodarz), <code>{"{"}event{"}"}</code> (identyfikator typu wydarzenia), <code>{"{"}date{"}"}</code> (RRRRMMDD), <code>{"{"}random{"}"}</code> (8 znaków). Domyślnie: { $default }.
admin-room-pattern-warning = Bez <code>{"{"}random{"}"}</code> nazwa pokoju jest przewidywalna: dwoje gości rezerwujących ten sam typ wydarzenia tego samego dnia trafi do jednego pokoju i zobaczy nawzajem swoje spotkania. Stałe pokoje są dozwolone (na przykład jeden pokój osobisty na gospodarza), ale włączaj to tylko wtedy, gdy rozumiesz konsekwencje.
admin-meeting-webhook-help = Gdy miejscem typu wydarzenia jest „Webhook (własny dostawca)”, calrs wysyła przy potwierdzeniu dane rezerwacji metodą POST na ten adres i oczekuje w odpowiedzi treści JSON <code>{"{"}"url": "https://..."{"}"}</code>.
admin-auth-hmac-help = Przy HMAC calrs wysyła <code>X-Calrs-Signature: sha256=&lt;hex&gt;</code> policzone z surowej treści żądania.
admin-tls-none-warning = Wybieraj <strong>Brak</strong> tylko dla przekaźnika na tej maszynie, który nie obsługuje STARTTLS albo ma certyfikat podpisany samodzielnie. Poczta, a wraz z nią wszelkie dane logowania, pójdzie wtedy otwartym tekstem.
admin-smtp-env-error-help = Popraw zmienne środowiskowe <code>CALRS_SMTP_*</code> albo usuń je, aby zarządzać SMTP z bazy danych tutaj.
admin-smtp-env-managed = Zarządzane przez <strong>zmienne środowiskowe</strong> (mają pierwszeństwo przed bazą danych). Zmień zmienne <code>CALRS_SMTP_*</code> albo usuń je, aby zarządzać SMTP stąd.
admin-smtp-env-help = Możesz też skonfigurować to zmiennymi środowiskowymi (mają pierwszeństwo przed tym): <code>CALRS_SMTP_HOST</code>, <code>CALRS_SMTP_PORT</code>, <code>CALRS_SMTP_TLS_MODE</code> (<code>starttls</code>, <code>tls</code> lub <code>none</code>), <code>CALRS_SMTP_USERNAME</code>, <code>CALRS_SMTP_PASSWORD</code>, <code>CALRS_SMTP_FROM_EMAIL</code>, <code>CALRS_SMTP_FROM_NAME</code>. Obowiązkowe są tylko <code>CALRS_SMTP_HOST</code> i <code>CALRS_SMTP_FROM_EMAIL</code>; pomiń nazwę użytkownika i hasło, aby przekazywać pocztę przez lokalny MTA bez uwierzytelniania.
admin-sms-env-error-help = Popraw zmienne środowiskowe <code>CALRS_SMS_*</code> albo usuń je, aby zarządzać SMS-ami z bazy danych tutaj.
admin-sms-env-managed = Zarządzane przez <strong>zmienne środowiskowe</strong> (mają pierwszeństwo przed bazą danych). Zmień zmienne <code>CALRS_SMS_*</code> albo usuń je, aby zarządzać SMS-ami stąd.
admin-sms-env-help = Możesz też skonfigurować to zmiennymi środowiskowymi (mają pierwszeństwo przed tym): <code>CALRS_SMS_PROVIDER</code>, <code>CALRS_SMS_API_KEY</code>, <code>CALRS_SMS_API_SECRET</code>, <code>CALRS_SMS_SENDER</code>, <code>CALRS_SMS_BASE_URL</code>, <code>CALRS_SMS_DAILY_CAP</code>, <code>CALRS_SMS_DEFAULT_COUNTRY_CODE</code>.
admin-sms-trial-warning = <strong>Tryb próbny Twilio jest włączony</strong> (<code>CALRS_SMS_TWILIO_TRIAL</code>). Goście dostają predefiniowany szablon Twilio <code>sms_appointment_reminders</code> zamiast prawdziwej wiadomości, a dotrzeć można tylko na numery zweryfikowane w twojej konsoli Twilio. To pomoc przy testowaniu na kontach próbnych. Usuń tę zmienną, zanim zaczniesz przyjmować rezerwacje.

admin-show-more =
    { $count ->
        [one] Pokaż o { $count } więcej
        [few] Pokaż o { $count } więcej
        [many] Pokaż o { $count } więcej
       *[other] Pokaż o { $count } więcej
    }

# Calendar source form: backend picker (templates/source_form.html)

source-form-backend-help = Wybierz protokół, którym mówi twój serwer. EWS dotyczy Exchange 2019/2016/2013 we własnej serwerowni.

admin-sms-going-live = <strong>Zanim ruszysz produkcyjnie:</strong> ogranicz kraje docelowe w swojej bramce (w Twilio nazywa się to Geo Permissions), trzymaj konto przedpłacone bez automatycznego doładowania i zostaw włączoną captchę. Te trzy rzeczy razem ograniczają koszt ewentualnej próby SMS pumpingu.

troubleshoot-heading = Diagnostyka dostępności

# Host-side form validation errors (src/web/mod.rs)

form-error-team-name-slug-required = Nazwa i identyfikator są wymagane.
form-error-team-name-length = Nazwa może mieć najwyżej 255 znaków.
form-error-team-description-length = Opis może mieć najwyżej 5000 znaków.
form-error-slug-charset = Identyfikator może zawierać tylko małe litery, cyfry i myślniki.
form-error-slug-reserved = Ten identyfikator jest zarezerwowany. Proszę wybrać inny.
form-error-team-slug-taken = Zespół o tym identyfikatorze już istnieje.
form-error-title-required = Do wygenerowania identyfikatora potrzebny jest tytuł.
form-error-event-type-slug-taken = Typ wydarzenia o tym identyfikatorze już istnieje.
form-error-event-type-slug-taken-team = W tym zespole istnieje już typ wydarzenia o tym identyfikatorze.
form-error-location-required = Szczegóły miejsca są wymagane (na przykład link do rozmowy wideo, numer telefonu albo adres).
form-error-not-team-admin = Nie jesteś administratorem tego zespołu.
form-error-no-account = Nie znaleziono profilu planowania. Proszę skontaktować się z administracją.
form-error-all-fields-required = Wszystkie pola są wymagane.
form-error-encryption = Błąd szyfrowania.
form-error-connection-failed = Połączenie nie powiodło się: { $error }. Sprawdź adres i dane logowania albo zaznacz „Pomiń test połączenia”, aby mimo to zapisać.

# Settings page flash (src/web/mod.rs)

settings-saved = Zapisano ustawienia.

# Profile settings validation and flash messages (src/web/mod.rs)

settings-error-name-length = Imię i nazwisko muszą mieć od 1 do 255 znaków.
settings-error-username-length = Nazwa użytkownika musi mieć co najmniej 2 znaki.
settings-error-username-taken = Ta nazwa użytkownika jest już zajęta.
settings-error-booking-email = Proszę podać prawidłowy adres e-mail do rezerwacji.
settings-error-save-failed = Nie udało się zapisać ustawień.

# Host-facing error responses (src/web/mod.rs)

error-team-not-found-or-not-admin = Nie znaleziono zespołu albo nie jesteś jego administratorem.
error-team-not-found = Nie znaleziono zespołu.
error-event-type-not-found = Nie znaleziono typu wydarzenia.
error-decrypt-failed = Nie udało się odszyfrować zapisanych danych logowania.
error-source-not-found = Nie znaleziono źródła.
error-source-no-password = To źródło nie ma zapisanego hasła.
error-oauth-invalid-state = Nieprawidłowy parametr stanu. Proszę spróbować ponownie.
error-oauth-no-code = Nie otrzymano kodu autoryzacji.
error-oauth-not-configured = Google OAuth2 nie jest skonfigurowane.
error-no-scheduling-account = Nie znaleziono profilu planowania.
error-private-event-type-not-found = Nie znaleziono prywatnego typu wydarzenia.
error-access-denied = Odmowa dostępu.

# Guest booking-flow errors (src/web/mod.rs)

error-slot-unavailable = Ten termin nie jest już dostępny.
error-slot-too-soon = Ten termin nie jest już dostępny (zbyt blisko).
error-slot-beyond-horizon = Ten termin wykracza poza okno rezerwacji.
error-invite-required = Ten typ wydarzenia wymaga linku z zaproszeniem.
error-invite-invalid = Nieprawidłowy link z zaproszeniem.
error-invite-expired = Ten link z zaproszeniem wygasł.
error-invite-used = Ten link z zaproszeniem został już wykorzystany.
error-invalid-date = Nieprawidłowa data.
error-invalid-time = Nieprawidłowa godzina.
error-invalid-date-format = Nieprawidłowy format daty.
error-invalid-time-format = Nieprawidłowy format godziny.
error-too-many-bookings = Zbyt wiele prób rezerwacji. Proszę spróbować ponownie za kilka minut.
error-too-many-requests = Zbyt wiele żądań. Proszę spróbować ponownie później.
error-no-members-available = Żaden członek zespołu nie jest dostępny w tym terminie.
error-dynamic-group-public-only = Dynamiczne linki grupowe są dostępne tylko dla publicznych typów wydarzeń.
error-user-not-found = Nie znaleziono użytkownika.

# Booking action error page: titles (templates/booking_action_error.html)

bae-title-captcha = Weryfikacja captcha nie powiodła się
bae-title-invalid-booking = Nieprawidłowe dane rezerwacji
bae-title-unavailable = Obecnie niedostępne
bae-title-cannot-approve = Nie można zatwierdzić tej rezerwacji
bae-title-invalid-link = Nieprawidłowy link
bae-title-invalid-or-expired = Nieprawidłowy lub wygasły link
bae-title-booking-not-found = Nie znaleziono rezerwacji
bae-title-already-approved = Już zatwierdzona
bae-title-already-declined = Już odrzucona
bae-title-already-cancelled = Już odwołana
bae-title-booking-cancelled = Rezerwacja odwołana
bae-title-booking-declined = Rezerwacja odrzucona

# Booking action error page: bodies

bae-body-go-back = Proszę wrócić i spróbować ponownie.
bae-body-unavailable = Gospodarz nie przyjmuje już rezerwacji na ten dzień. Proszę wybrać inną datę albo zajrzeć później.
bae-body-resource-gone = Wymagany zasób nie jest już dostępny o tej porze. Poproś gościa o wybranie innego terminu.
bae-body-no-claim-token = Nie podano tokenu.
bae-body-claim-invalid = Ten link nie jest już ważny.
bae-body-booking-gone = Ta rezerwacja już nie istnieje.
bae-body-decline-link-invalid = Ten link odrzucenia jest nieprawidłowy, wygasł albo rezerwacja została już obsłużona.
bae-body-cancel-link-invalid = Ten link odwołania jest nieprawidłowy, wygasł albo rezerwacja została już odwołana.
bae-body-cancel-link-invalid-short = Ten link odwołania jest nieprawidłowy albo wygasł.
bae-body-reschedule-link-invalid = Ten link przeniesienia jest nieprawidłowy, wygasł albo rezerwacja została już obsłużona.
bae-body-approval-link-invalid = Ten link zatwierdzenia jest nieprawidłowy albo wygasł.
bae-body-already-approved = Ta rezerwacja została już zatwierdzona.
bae-body-already-declined = Ta rezerwacja została już odrzucona.
bae-body-already-cancelled = Ta rezerwacja została już odwołana.
bae-body-was-cancelled = Ta rezerwacja została odwołana.
bae-body-declined-by-host = Ta rezerwacja została odrzucona przez gospodarza.

# Booking form validation (src/web/mod.rs)

validate-name-length = Imię i nazwisko muszą mieć od 1 do 255 znaków.
validate-email-length = Adres e-mail musi mieć od 1 do 255 znaków.
validate-email-invalid = Proszę podać prawidłowy adres e-mail.
validate-notes-length = Notatki nie mogą przekraczać 5000 znaków.
validate-date-too-far = Nie można rezerwować z wyprzedzeniem większym niż rok.

# Additional guests and dynamic group links (src/web/mod.rs)

guests-not-allowed = Ten typ wydarzenia nie dopuszcza dodatkowych gości.
guests-too-many =
    { $max ->
        [one] Można dodać najwyżej jednego dodatkowego gościa.
        [few] Można dodać najwyżej { $max } dodatkowych gości.
        [many] Można dodać najwyżej { $max } dodatkowych gości.
       *[other] Można dodać najwyżej { $max } dodatkowych gości.
    }
guests-invalid-email = Nieprawidłowy adres e-mail dodatkowego gościa: { $email }
dynamic-group-min-usernames = Linki grupy dynamicznej wymagają co najmniej dwóch nazw użytkowników.
dynamic-group-user-not-found = Nie znaleziono użytkownika „{ $username }”.
dynamic-group-user-opted-out = Użytkownik „{ $username }” nie włączył linków grupy dynamicznej.

error-slot-unavailable-member = Ten termin nie jest już dostępny ({ $username } ma konflikt).
