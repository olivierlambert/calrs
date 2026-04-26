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
book-notes-label = Notatki
book-notes-optional = (opcjonalne)
book-notes-placeholder = Czy jest coś, co chcesz omówić?
book-additional-guests-label = Dodatkowi goście
book-additional-guests-hint = (opcjonalne, do { $max })
book-add-guest-btn = + Dodaj gościa
book-guest-email-placeholder = wspolpracownik@example.com
book-confirm-button = Potwierdź rezerwację

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

# Month and weekday names + per-locale date format patterns.
# Polish: nominative month names. The long-date format reads informally
# in this case ("27 kwiecień 2026" instead of the strict-grammar
# "27 kwietnia 2026"); a future refinement could split into separate
# nominative and genitive keys.

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

common-format-month-year = { $month } { $year }
common-format-long-date = { $weekday }, { $day } { $month } { $year }

# Email signatures and shared bits (src/email.rs)

email-signature = — calrs
email-action-reschedule = Zmień termin
email-action-cancel-booking = Anuluj rezerwację

# Email: guest booking confirmation

email-confirm-subject = Potwierdzono: { $event } — { $date }
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
