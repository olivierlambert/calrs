//! Booking questions (#210, #170): the questions a host adds to an event
//! type's booking form, and the answers each booking keeps.
//!
//! One module owns the feature so every booking path (the four POST
//! handlers, through `book_personal` and `book_team` for two of them), the
//! event type editor and every display surface go through the same code:
//! loading the form (`booking_form`), reading and validating what a guest
//! posted (`read_submission`), storing it with the booking
//! (`insert_answers`), reading it back (`booking_answers`,
//! `answers_by_booking`), and the editor's parse, validation and save
//! (`parse_editor`, `save`).
//!
//! Answers can be sensitive (an NGO's "area of concern"), so nothing here
//! logs a label or a value, and the read side never widens who sees them:
//! the callers decide, by surface, as listed in `docs/src/booking-questions.md`.

use super::*;
use crate::email::BookingAnswer;
use serde::Serialize;

/// Questions per event type. The largest example in #210 has 11.
pub(crate) const MAX_QUESTIONS: usize = 20;
/// Options per choice question.
pub(crate) const MAX_OPTIONS: usize = 30;
pub(crate) const MAX_OPTION_LABEL: usize = 200;
pub(crate) const MAX_LABEL: usize = 200;
/// A consent checkbox's label is the sentence the guest agrees to, which
/// can run longer than a question.
pub(crate) const MAX_CONSENT_LABEL: usize = 1000;
pub(crate) const MAX_HELP: usize = 500;
pub(crate) const MAX_SHORT_ANSWER: usize = 500;
/// The same bound as the built-in notes field.
pub(crate) const MAX_LONG_ANSWER: usize = 5000;
pub(crate) const MAX_LINK_URL: usize = 2000;
pub(crate) const MAX_KEY: usize = 40;
/// A single choice with more options than this renders as a `select`.
const RADIO_LIMIT: usize = 5;

/// The five kinds of question. Stored as text, so a later kind needs no
/// migration; a row of a kind this build does not know is skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    ShortText,
    LongText,
    SingleChoice,
    MultipleChoice,
    Checkbox,
}

impl Kind {
    pub(crate) fn parse(value: &str) -> Option<Kind> {
        match value {
            "short_text" => Some(Kind::ShortText),
            "long_text" => Some(Kind::LongText),
            "single_choice" => Some(Kind::SingleChoice),
            "multiple_choice" => Some(Kind::MultipleChoice),
            "checkbox" => Some(Kind::Checkbox),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Kind::ShortText => "short_text",
            Kind::LongText => "long_text",
            Kind::SingleChoice => "single_choice",
            Kind::MultipleChoice => "multiple_choice",
            Kind::Checkbox => "checkbox",
        }
    }

    fn has_options(self) -> bool {
        matches!(self, Kind::SingleChoice | Kind::MultipleChoice)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct QuestionOption {
    pub(crate) id: String,
    pub(crate) label: String,
}

/// One question of an event type's booking form.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Question {
    pub(crate) id: String,
    pub(crate) key: String,
    pub(crate) kind: Kind,
    pub(crate) label: String,
    pub(crate) help_text: Option<String>,
    pub(crate) required: bool,
    pub(crate) options: Vec<QuestionOption>,
    pub(crate) consent: bool,
    pub(crate) link_url: Option<String>,
    pub(crate) in_calendar: bool,
}

/// The three states of the built-in notes field (`event_types.notes_mode`).
/// Anything else stored reads as 'optional', the behaviour before #210.
pub(crate) fn notes_mode(value: &str) -> &'static str {
    match value.trim() {
        "off" => "off",
        "required" => "required",
        _ => "optional",
    }
}

#[allow(clippy::type_complexity)]
type QuestionRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    bool,
    Option<String>,
    bool,
    Option<String>,
    bool,
);

/// The questions of an event type, in form order.
pub(crate) async fn load(pool: &SqlitePool, event_type_id: &str) -> sqlx::Result<Vec<Question>> {
    let rows: Vec<QuestionRow> = sqlx::query_as(
        "SELECT id, key, kind, label, help_text, required, options, consent, link_url, in_calendar
         FROM event_type_questions WHERE event_type_id = ? ORDER BY position, id",
    )
    .bind(event_type_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(
            |(
                id,
                key,
                kind,
                label,
                help_text,
                required,
                options,
                consent,
                link_url,
                in_calendar,
            )| {
                let kind = Kind::parse(&kind)?;
                let options = if kind.has_options() {
                    options
                        .as_deref()
                        .and_then(|o| serde_json::from_str(o).ok())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                Some(Question {
                    id,
                    key,
                    kind,
                    label,
                    help_text: help_text.filter(|h| !h.trim().is_empty()),
                    required,
                    options,
                    consent: consent && kind == Kind::Checkbox,
                    link_url: link_url.filter(|l| !l.trim().is_empty()),
                    in_calendar,
                })
            },
        )
        .collect())
}

/// What a booking form asks beyond name and email: the notes mode and the
/// questions.
#[derive(Clone, Debug)]
pub(crate) struct BookingForm {
    pub(crate) notes_mode: &'static str,
    pub(crate) questions: Vec<Question>,
}

impl Default for BookingForm {
    fn default() -> Self {
        BookingForm {
            notes_mode: "optional",
            questions: Vec::new(),
        }
    }
}

pub(crate) async fn booking_form(
    pool: &SqlitePool,
    event_type_id: &str,
) -> sqlx::Result<BookingForm> {
    let mode: Option<String> =
        sqlx::query_scalar("SELECT notes_mode FROM event_types WHERE id = ?")
            .bind(event_type_id)
            .fetch_optional(pool)
            .await?;
    Ok(BookingForm {
        notes_mode: notes_mode(mode.as_deref().unwrap_or("")),
        questions: load(pool, event_type_id).await?,
    })
}

/// The form field of a question; a multiple-choice box adds `__<option id>`.
/// Every name is unique, since the booking form is read by
/// `serde_urlencoded`, which cannot collect a repeated key.
fn field_name(question_id: &str) -> String {
    format!("q_{question_id}")
}

fn option_field_name(question_id: &str, option_id: &str) -> String {
    format!("q_{question_id}__{option_id}")
}

/// The questions as `book.html` renders them.
pub(crate) fn form_context(form: &BookingForm) -> Vec<minijinja::Value> {
    form.questions
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let options: Vec<minijinja::Value> = q
                .options
                .iter()
                .enumerate()
                .map(|(j, o)| {
                    context! {
                        id => o.id,
                        label => o.label,
                        name => option_field_name(&q.id, &o.id),
                        html_id => format!("q{i}-o{j}"),
                    }
                })
                .collect();
            context! {
                name => field_name(&q.id),
                html_id => format!("q{i}"),
                kind => q.kind.as_str(),
                label => q.label,
                help => q.help_text.as_deref().unwrap_or(""),
                required => q.required,
                consent => q.consent,
                link_url => q.link_url.as_deref().unwrap_or(""),
                options => options,
                as_select => q.kind == Kind::SingleChoice && q.options.len() > RADIO_LIMIT,
                max_len => if q.kind == Kind::LongText { MAX_LONG_ANSWER } else { MAX_SHORT_ANSWER },
            }
        })
        .collect()
}

/// An answer read from a booking POST, before it is stored.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewAnswer {
    pub(crate) question_id: String,
    pub(crate) answer: BookingAnswer,
}

/// What the guest typed beyond name and email, validated.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Submission {
    /// The notes to store: as posted in 'optional' mode (unchanged from
    /// before #210), never in 'off'.
    pub(crate) notes: Option<String>,
    pub(crate) answers: Vec<NewAnswer>,
}

impl Submission {
    /// The answers as the emails and the calendar copy carry them.
    pub(crate) fn booking_answers(&self) -> Vec<BookingAnswer> {
        self.answers.iter().map(|a| a.answer.clone()).collect()
    }
}

/// Why a booking POST was refused. The label is the host's own text, shown
/// back to the guest who just read it on the form.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Refusal {
    NotesRequired,
    Required(String),
    TooLong(String),
    UnknownOption(String),
}

impl Refusal {
    pub(crate) fn message(&self, lang: &str) -> String {
        match self {
            Refusal::NotesRequired => crate::i18n::translate(lang, "validate-notes-required", None),
            Refusal::Required(label) => tr1(lang, "validate-question-required", "question", label),
            Refusal::TooLong(label) => tr1(lang, "validate-question-too-long", "question", label),
            Refusal::UnknownOption(label) => {
                tr1(lang, "validate-question-unknown-option", "question", label)
            }
        }
    }
}

/// Removes control characters a guest or host has no reason to type. Line
/// breaks and tabs survive only where `multiline` allows them, normalised
/// to `\n`.
pub(crate) fn clean_text(value: &str, multiline: bool) -> String {
    let value = value.replace("\r\n", "\n").replace('\r', "\n");
    value
        .chars()
        .filter(|c| !c.is_control() || (multiline && (*c == '\n' || *c == '\t')))
        .collect::<String>()
        .trim()
        .to_string()
}

fn now_utc() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Reads the notes and the answers out of a booking POST, against the
/// event type's current form. Unknown `q_` fields are ignored; a missing
/// required answer, an option the question does not offer or an oversized
/// value refuses the booking.
pub(crate) fn parse_submission(
    form: &BookingForm,
    notes: Option<&str>,
    fields: &HashMap<String, String>,
) -> Result<Submission, Refusal> {
    let notes = match form.notes_mode {
        "off" => None,
        "required" => {
            let n = notes.map(str::trim).unwrap_or("");
            if n.is_empty() {
                return Err(Refusal::NotesRequired);
            }
            notes.map(str::to_string)
        }
        _ => notes.map(str::to_string),
    };

    let answered_at = now_utc();
    let mut answers = Vec::new();
    for q in &form.questions {
        if let Some(answer) = parse_answer(q, fields, &answered_at)? {
            answers.push(answer);
        }
    }
    Ok(Submission { notes, answers })
}

/// Reads one question's answer out of the posted `fields`: `None` when an
/// optional question was left unanswered (a checkbox always answers).
fn parse_answer(
    q: &Question,
    fields: &HashMap<String, String>,
    answered_at: &str,
) -> Result<Option<NewAnswer>, Refusal> {
    let posted = fields.get(&field_name(&q.id)).map(String::as_str);
    let (value, value_ids): (String, Vec<String>) = match q.kind {
        Kind::ShortText | Kind::LongText => {
            let multiline = q.kind == Kind::LongText;
            let max = if multiline {
                MAX_LONG_ANSWER
            } else {
                MAX_SHORT_ANSWER
            };
            let text = clean_text(posted.unwrap_or(""), multiline);
            if text.chars().count() > max {
                return Err(Refusal::TooLong(q.label.clone()));
            }
            if text.is_empty() {
                if q.required {
                    return Err(Refusal::Required(q.label.clone()));
                }
                return Ok(None);
            }
            (text, Vec::new())
        }
        Kind::SingleChoice => {
            let chosen = posted.map(str::trim).unwrap_or("");
            if chosen.is_empty() {
                if q.required {
                    return Err(Refusal::Required(q.label.clone()));
                }
                return Ok(None);
            }
            let Some(option) = q.options.iter().find(|o| o.id == chosen) else {
                return Err(Refusal::UnknownOption(q.label.clone()));
            };
            (option.label.clone(), vec![option.id.clone()])
        }
        Kind::MultipleChoice => {
            let prefix = format!("{}__", field_name(&q.id));
            for (name, value) in fields {
                if let Some(option_id) = name.strip_prefix(&prefix) {
                    if !value.is_empty() && !q.options.iter().any(|o| o.id == option_id) {
                        return Err(Refusal::UnknownOption(q.label.clone()));
                    }
                }
            }
            let chosen: Vec<&QuestionOption> = q
                .options
                .iter()
                .filter(|o| {
                    fields
                        .get(&option_field_name(&q.id, &o.id))
                        .is_some_and(|v| !v.is_empty())
                })
                .collect();
            if chosen.is_empty() {
                if q.required {
                    return Err(Refusal::Required(q.label.clone()));
                }
                return Ok(None);
            }
            (
                chosen
                    .iter()
                    .map(|o| o.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                chosen.iter().map(|o| o.id.clone()).collect(),
            )
        }
        Kind::Checkbox => {
            let checked = posted.is_some_and(|v| !v.is_empty());
            if q.required && !checked {
                return Err(Refusal::Required(q.label.clone()));
            }
            // An unticked box is an answer too: "did not agree".
            (if checked { "yes" } else { "no" }.to_string(), Vec::new())
        }
    };
    Ok(Some(NewAnswer {
        question_id: q.id.clone(),
        answer: BookingAnswer {
            key: q.key.clone(),
            kind: q.kind.as_str().to_string(),
            label: q.label.clone(),
            value,
            option_ids: value_ids,
            consent: q.consent,
            link_url: q.link_url.clone(),
            in_calendar: q.in_calendar,
            answered_at: answered_at.to_string(),
        },
    }))
}

/// What a booking path hands `read_submission`.
#[derive(Clone, Copy)]
pub(super) enum Posted<'a> {
    /// The booking form's fields, `q_<id>` and `q_<id>__<option id>`, read
    /// against the event type's current form.
    Form(&'a HashMap<String, String>),
    /// A manager booking for a guest through the REST API: the answers its
    /// `answers` object carried, already read by `parse_api_answers` (none
    /// when it sent no `answers`). The notes are kept as given, whatever
    /// the event type's `notes_mode`, as they were before #210.
    Api(&'a [NewAnswer]),
}

/// The one call every booking path makes: loads the event type's form,
/// reads the posted notes and answers against it, and returns the refusal
/// to show when they do not pass.
pub(super) async fn read_submission(
    pool: &SqlitePool,
    event_type_id: &str,
    notes: &Option<String>,
    posted: Posted<'_>,
    lang: &str,
) -> Result<Submission, BookingRefusal> {
    let fields = match posted {
        Posted::Form(fields) => fields,
        Posted::Api(answers) => {
            return Ok(Submission {
                notes: notes.clone(),
                answers: answers.to_vec(),
            })
        }
    };
    let form = booking_form(pool, event_type_id)
        .await
        .map_err(|e| BookingRefusal::Internal("booking questions", e.to_string()))?;
    parse_submission(&form, notes.as_deref(), fields).map_err(|refusal| {
        BookingRefusal::Message(
            crate::i18n::translate(lang, "bae-title-invalid-booking", None),
            refusal.message(lang),
        )
    })
}

/// Reads the `answers` object of a `POST /api/v1/bookings` (#169, #210),
/// keyed by question key: a string for a text question, an option id for a
/// single choice, an array of option ids for a multiple choice, a boolean
/// for a checkbox. Each value becomes the field the booking form would post
/// and goes through `parse_answer`, so the API takes exactly what the form
/// takes. Every required question must be answered, a required checkbox
/// with `true`; an optional checkbox left out is stored as not ticked, as on
/// the form. The refusal is an English message naming the key, like the
/// API's other errors.
pub(crate) fn parse_api_answers(
    form: &BookingForm,
    answers: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<NewAnswer>, String> {
    use serde_json::Value;
    let mut fields = HashMap::new();
    for (key, value) in answers {
        let Some(q) = form.questions.iter().find(|q| q.key == *key) else {
            // No stored key is longer, so a longer one is not echoed back.
            return Err(if key.chars().count() <= MAX_KEY {
                format!("Unknown question {key}.")
            } else {
                "Unknown question key.".to_string()
            });
        };
        match (q.kind, value) {
            (Kind::ShortText | Kind::LongText | Kind::SingleChoice, Value::String(text)) => {
                fields.insert(field_name(&q.id), text.clone());
            }
            (Kind::MultipleChoice, Value::Array(items)) => {
                for item in items {
                    let Value::String(option_id) = item else {
                        return Err(format!(
                            "The answer to {key} must be an array of option ids."
                        ));
                    };
                    fields.insert(option_field_name(&q.id, option_id), "on".to_string());
                }
            }
            (Kind::Checkbox, Value::Bool(ticked)) => {
                if *ticked {
                    fields.insert(field_name(&q.id), "on".to_string());
                }
            }
            (kind, _) => {
                let expected = match kind {
                    Kind::ShortText | Kind::LongText => "a string",
                    Kind::SingleChoice => "an option id",
                    Kind::MultipleChoice => "an array of option ids",
                    Kind::Checkbox => "true or false",
                };
                return Err(format!("The answer to {key} must be {expected}."));
            }
        }
    }
    let answered_at = now_utc();
    let mut parsed = Vec::new();
    for q in &form.questions {
        match parse_answer(q, &fields, &answered_at) {
            Ok(Some(answer)) => parsed.push(answer),
            Ok(None) => {}
            Err(refusal) => {
                let key = &q.key;
                return Err(match refusal {
                    Refusal::Required(_) if q.kind == Kind::Checkbox => {
                        format!("Question {key} is required: its answer must be true.")
                    }
                    Refusal::Required(_) => format!("Question {key} is required."),
                    Refusal::TooLong(_) => {
                        let max = if q.kind == Kind::LongText {
                            MAX_LONG_ANSWER
                        } else {
                            MAX_SHORT_ANSWER
                        };
                        format!("The answer to {key} is over {max} characters.")
                    }
                    Refusal::UnknownOption(_) => {
                        format!("The answer to {key} names an option the question does not offer.")
                    }
                    Refusal::NotesRequired => "Invalid answers.".to_string(),
                });
            }
        }
    }
    Ok(parsed)
}

/// Stores a booking's answers, inside the booking's own transaction so a
/// refused booking leaves none behind.
pub(crate) async fn insert_answers(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    booking_id: &str,
    answers: &[NewAnswer],
) -> sqlx::Result<()> {
    for (position, new) in answers.iter().enumerate() {
        let a = &new.answer;
        let value_ids = if a.option_ids.is_empty() {
            None
        } else {
            serde_json::to_string(&a.option_ids).ok()
        };
        sqlx::query(
            "INSERT INTO booking_answers (id, booking_id, question_id, position, key, kind, label, value, value_ids, consent, link_url, in_calendar, answered_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(booking_id)
        .bind(&new.question_id)
        .bind(position as i64)
        .bind(&a.key)
        .bind(&a.kind)
        .bind(&a.label)
        .bind(&a.value)
        .bind(value_ids)
        .bind(a.consent)
        .bind(&a.link_url)
        .bind(a.in_calendar)
        .bind(&a.answered_at)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[allow(clippy::type_complexity)]
type AnswerRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    bool,
    Option<String>,
    bool,
    String,
);

fn answer_from_row(
    (booking, key, kind, label, value, value_ids, consent, link_url, in_calendar, answered_at): AnswerRow,
) -> (String, BookingAnswer) {
    (
        booking,
        BookingAnswer {
            key,
            kind,
            label,
            value,
            option_ids: value_ids
                .as_deref()
                .and_then(|v| serde_json::from_str(v).ok())
                .unwrap_or_default(),
            consent,
            link_url,
            in_calendar,
            answered_at,
        },
    )
}

const ANSWER_COLUMNS: &str = "ba.key, ba.kind, ba.label, ba.value, ba.value_ids, ba.consent, ba.link_url, ba.in_calendar, ba.answered_at";

/// A booking's answers, in form order, by the booking's uid. Read by
/// `booking_guest_input`, so every path that rebuilds `BookingDetails` from
/// the row carries them.
pub(crate) async fn booking_answers(pool: &SqlitePool, booking_uid: &str) -> Vec<BookingAnswer> {
    let rows: Vec<AnswerRow> = sqlx::query_as(&format!(
        "SELECT b.id, {ANSWER_COLUMNS}
         FROM booking_answers ba JOIN bookings b ON b.id = ba.booking_id
         WHERE b.uid = ? ORDER BY ba.position, ba.id"
    ))
    .bind(booking_uid)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    rows.into_iter().map(|r| answer_from_row(r).1).collect()
}

/// The answers of several bookings by booking id, for a list page: one
/// query per hundred bookings rather than one per row.
pub(crate) async fn answers_by_booking(
    pool: &SqlitePool,
    booking_ids: &[&str],
) -> HashMap<String, Vec<BookingAnswer>> {
    let mut by_booking: HashMap<String, Vec<BookingAnswer>> = HashMap::new();
    for chunk in booking_ids.chunks(100) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let sql = format!(
            "SELECT ba.booking_id, {ANSWER_COLUMNS}
             FROM booking_answers ba
             WHERE ba.booking_id IN ({placeholders}) ORDER BY ba.booking_id, ba.position, ba.id"
        );
        let mut query = sqlx::query_as::<_, AnswerRow>(&sql);
        for id in chunk {
            query = query.bind(*id);
        }
        for row in query.fetch_all(pool).await.unwrap_or_default() {
            let (booking, answer) = answer_from_row(row);
            by_booking.entry(booking).or_default().push(answer);
        }
    }
    by_booking
}

/// Answers as label and value pairs for a page, the value written for
/// `reader` in `lang` ("Yes", "You agreed").
pub(crate) fn display_rows(
    answers: &[BookingAnswer],
    lang: &str,
    reader: crate::email::AnswerReader,
) -> Vec<minijinja::Value> {
    answers
        .iter()
        .map(|a| {
            context! {
                label => a.label,
                heading => crate::email::answer_heading(&a.label),
                value => crate::email::answer_display(a, lang, reader),
            }
        })
        .collect()
}

// --- The editor ---

/// A question as the event type form posts it (the hidden `questions`
/// field, JSON) and as it is handed back to the form's script.
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct EditorQuestion {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) key: String,
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) label: String,
    #[serde(default)]
    pub(crate) help: String,
    #[serde(default)]
    pub(crate) required: bool,
    #[serde(default)]
    pub(crate) options: Vec<EditorOption>,
    #[serde(default)]
    pub(crate) consent: bool,
    #[serde(default)]
    pub(crate) link: String,
    #[serde(default = "default_true")]
    pub(crate) in_calendar: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct EditorOption {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) label: String,
}

fn default_true() -> bool {
    true
}

/// The stored questions as the editor script reads them.
pub(crate) fn editor_json(questions: &[Question]) -> String {
    let items: Vec<EditorQuestion> = questions
        .iter()
        .map(|q| EditorQuestion {
            id: q.id.clone(),
            key: q.key.clone(),
            kind: q.kind.as_str().to_string(),
            label: q.label.clone(),
            help: q.help_text.clone().unwrap_or_default(),
            required: q.required,
            options: q
                .options
                .iter()
                .map(|o| EditorOption {
                    id: o.id.clone(),
                    label: o.label.clone(),
                })
                .collect(),
            consent: q.consent,
            link: q.link_url.clone().unwrap_or_default(),
            in_calendar: q.in_calendar,
        })
        .collect();
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

/// Why the editor's questions were refused: a `validate-question*` key and
/// the position (from 1) of the question it is about, 0 for the whole list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EditorError {
    pub(crate) key: &'static str,
    pub(crate) position: usize,
}

impl EditorError {
    pub(crate) fn message(&self, lang: &str) -> String {
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("position", crate::i18n::number(self.position as f64));
        args.set("max", crate::i18n::number(MAX_QUESTIONS as f64));
        crate::i18n::translate(lang, self.key, Some(&args))
    }
}

/// An absolute http(s) URL, for a consent checkbox's policy link.
fn valid_link(link: &str) -> bool {
    link.len() <= MAX_LINK_URL
        && reqwest::Url::parse(link)
            .is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
}

/// A key from a label: lowercase ASCII letters and digits, words joined by
/// `_`, at most `MAX_KEY` characters, "question" when nothing is left.
pub(crate) fn key_from_label(label: &str) -> String {
    let mut key = String::new();
    for c in label.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            key.push(c);
        } else if !key.is_empty() && !key.ends_with('_') {
            key.push('_');
        }
        if key.len() >= MAX_KEY {
            break;
        }
    }
    let key = key.trim_end_matches('_').to_string();
    if key.is_empty() {
        "question".to_string()
    } else {
        key
    }
}

/// Validates the editor's questions against the stored ones and returns the
/// form to save. A posted id that is not one of this event type's questions
/// is a new question (ids are never taken from the client); a stored
/// question keeps its key and its kind, and changing the kind is refused
/// (delete it and add a new one). Option ids are kept for an option the
/// question already had, new ones are minted.
pub(crate) fn parse_editor(
    json: &str,
    existing: &[Question],
) -> Result<Vec<Question>, EditorError> {
    let whole = |key| EditorError { key, position: 0 };
    let posted: Vec<EditorQuestion> = if json.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(json).map_err(|_| whole("validate-questions-invalid"))?
    };
    if posted.len() > MAX_QUESTIONS {
        return Err(whole("validate-questions-too-many"));
    }

    let mut out: Vec<Question> = Vec::with_capacity(posted.len());
    let mut used_keys: Vec<String> = Vec::new();
    // Stored questions that stay keep their key, so a new question cannot
    // take it whatever the order.
    for p in &posted {
        if let Some(stored) = existing.iter().find(|q| q.id == p.id) {
            used_keys.push(stored.key.clone());
        }
    }

    for (i, p) in posted.into_iter().enumerate() {
        let err = |key| EditorError {
            key,
            position: i + 1,
        };
        let kind = Kind::parse(p.kind.trim()).ok_or(err("validate-question-kind"))?;
        let stored = existing.iter().find(|q| q.id == p.id);
        if out
            .iter()
            .any(|q: &Question| stored.is_some_and(|s| s.id == q.id))
        {
            return Err(err("validate-questions-invalid"));
        }
        if let Some(s) = stored {
            if s.kind != kind {
                return Err(err("validate-question-kind-change"));
            }
        }

        let consent = kind == Kind::Checkbox && p.consent;
        let label = clean_text(&p.label, false);
        let max_label = if kind == Kind::Checkbox {
            MAX_CONSENT_LABEL
        } else {
            MAX_LABEL
        };
        if label.is_empty() {
            return Err(err("validate-question-label-empty"));
        }
        if label.chars().count() > max_label {
            return Err(err("validate-question-label-too-long"));
        }
        let help = clean_text(&p.help, false);
        if help.chars().count() > MAX_HELP {
            return Err(err("validate-question-help-too-long"));
        }

        let options = if kind.has_options() {
            if p.options.len() < 2 {
                return Err(err("validate-question-options-few"));
            }
            if p.options.len() > MAX_OPTIONS {
                return Err(err("validate-question-options-many"));
            }
            let stored_options = stored.map(|s| s.options.as_slice()).unwrap_or(&[]);
            let mut next = stored_options
                .iter()
                .filter_map(|o| o.id.strip_prefix('o')?.parse::<u32>().ok())
                .max()
                .unwrap_or(0);
            let mut options: Vec<QuestionOption> = Vec::with_capacity(p.options.len());
            for o in &p.options {
                let label = clean_text(&o.label, false);
                if label.is_empty() {
                    return Err(err("validate-question-option-empty"));
                }
                if label.chars().count() > MAX_OPTION_LABEL {
                    return Err(err("validate-question-option-too-long"));
                }
                if options.iter().any(|x| x.label == label) {
                    return Err(err("validate-question-option-duplicate"));
                }
                let keep = stored_options.iter().any(|s| s.id == o.id)
                    && !options.iter().any(|x| x.id == o.id);
                let id = if keep {
                    o.id.clone()
                } else {
                    next += 1;
                    format!("o{next}")
                };
                options.push(QuestionOption { id, label });
            }
            options
        } else {
            Vec::new()
        };

        let link = p.link.trim();
        let link_url = if kind == Kind::Checkbox && !link.is_empty() {
            if !valid_link(link) {
                return Err(err("validate-question-link"));
            }
            Some(link.to_string())
        } else {
            None
        };

        let (id, key) = match stored {
            Some(s) => (s.id.clone(), s.key.clone()),
            None => {
                let base = key_from_label(&label);
                let mut key = base.clone();
                let mut n = 2;
                while used_keys.contains(&key) {
                    // Shorten the base so the suffix keeps the key within
                    // `MAX_KEY` (keys are ASCII, so bytes are characters).
                    let suffix = format!("_{n}");
                    let room = MAX_KEY - suffix.len();
                    let stem = base[..base.len().min(room)].trim_end_matches('_');
                    key = format!("{stem}{suffix}");
                    n += 1;
                }
                used_keys.push(key.clone());
                (uuid::Uuid::new_v4().to_string(), key)
            }
        };

        out.push(Question {
            id,
            key,
            kind,
            label,
            help_text: Some(help).filter(|h| !h.is_empty()),
            required: p.required,
            options,
            consent,
            link_url,
            in_calendar: p.in_calendar,
        });
    }
    Ok(out)
}

/// The questions an event type form posted, validated against the stored
/// ones (`existing_for`, the event type being edited, `None` on create).
/// `Ok(None)` when the form carried no editor field.
pub(crate) async fn editor_questions(
    pool: &SqlitePool,
    existing_for: Option<&str>,
    posted: Option<&str>,
) -> Result<Option<Vec<Question>>, EditorError> {
    let Some(posted) = posted else {
        return Ok(None);
    };
    let existing = match existing_for {
        Some(et_id) => load(pool, et_id).await.map_err(|_| EditorError {
            key: "validate-questions-invalid",
            position: 0,
        })?,
        None => Vec::new(),
    };
    parse_editor(posted, &existing).map(Some)
}

/// What the event type form's "Booking form" section shows: the editor's
/// JSON, the notes mode, and a link to the public booking page when it has
/// one anyone can open.
pub(crate) struct EditorContext {
    pub(crate) questions_json: String,
    pub(crate) notes_mode: &'static str,
    pub(crate) preview_url: String,
}

/// The form section of a stored event type, `None` for a new one.
pub(crate) async fn editor_context(
    pool: &SqlitePool,
    event_type_id: Option<&str>,
) -> EditorContext {
    let Some(et_id) = event_type_id else {
        return EditorContext {
            questions_json: "[]".to_string(),
            notes_mode: "optional",
            preview_url: String::new(),
        };
    };
    let form = booking_form(pool, et_id).await.unwrap_or_default();
    let page: Option<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT et.slug, et.visibility, t.slug, u.username
         FROM event_types et
         LEFT JOIN teams t ON t.id = et.team_id
         LEFT JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id
         WHERE et.id = ?",
    )
    .bind(et_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    let preview_url = match page {
        Some((slug, visibility, team, user)) if visibility == "public" => match (team, user) {
            (Some(team), _) => format!("/team/{team}/{slug}"),
            (None, Some(user)) => format!("/u/{user}/{slug}"),
            _ => String::new(),
        },
        _ => String::new(),
    };
    EditorContext {
        questions_json: editor_json(&form.questions),
        notes_mode: form.notes_mode,
        preview_url,
    }
}

/// Saves an event type's notes mode and questions on its own, for tests.
#[cfg(test)]
pub(crate) async fn save(
    pool: &SqlitePool,
    event_type_id: &str,
    questions: Option<&[Question]>,
    notes: Option<&str>,
) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    save_in(&mut tx, event_type_id, questions, notes).await?;
    tx.commit().await
}

/// Saves an event type's notes mode and questions inside `tx`, the
/// transaction that writes the event type itself, so the two land together
/// or not at all. `None` leaves that part as stored: a form posted without
/// the editor (an older page, a script) never erases the questions.
pub(crate) async fn save_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event_type_id: &str,
    questions: Option<&[Question]>,
    notes: Option<&str>,
) -> sqlx::Result<()> {
    if let Some(mode) = notes {
        sqlx::query("UPDATE event_types SET notes_mode = ? WHERE id = ?")
            .bind(notes_mode(mode))
            .bind(event_type_id)
            .execute(&mut **tx)
            .await?;
    }
    if let Some(questions) = questions {
        let stored: Vec<String> =
            sqlx::query_scalar("SELECT id FROM event_type_questions WHERE event_type_id = ?")
                .bind(event_type_id)
                .fetch_all(&mut **tx)
                .await?;
        // Removed questions go first, so a new question may reuse the key
        // of one deleted in the same save. Their answers keep the snapshot
        // (`question_id` becomes NULL).
        for id in stored
            .iter()
            .filter(|id| !questions.iter().any(|q| &q.id == *id))
        {
            sqlx::query("DELETE FROM event_type_questions WHERE id = ? AND event_type_id = ?")
                .bind(id)
                .bind(event_type_id)
                .execute(&mut **tx)
                .await?;
        }
        for (position, q) in questions.iter().enumerate() {
            let options = if q.options.is_empty() {
                None
            } else {
                serde_json::to_string(&q.options).ok()
            };
            if stored.contains(&q.id) {
                sqlx::query(
                    "UPDATE event_type_questions SET position = ?, label = ?, help_text = ?, required = ?, options = ?, consent = ?, link_url = ?, in_calendar = ?
                     WHERE id = ? AND event_type_id = ?",
                )
                .bind(position as i64)
                .bind(&q.label)
                .bind(&q.help_text)
                .bind(q.required)
                .bind(options)
                .bind(q.consent)
                .bind(&q.link_url)
                .bind(q.in_calendar)
                .bind(&q.id)
                .bind(event_type_id)
                .execute(&mut **tx)
                .await?;
            } else {
                sqlx::query(
                    "INSERT INTO event_type_questions (id, event_type_id, position, key, kind, label, help_text, required, options, consent, link_url, in_calendar)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&q.id)
                .bind(event_type_id)
                .bind(position as i64)
                .bind(&q.key)
                .bind(q.kind.as_str())
                .bind(&q.label)
                .bind(&q.help_text)
                .bind(q.required)
                .bind(options)
                .bind(q.consent)
                .bind(&q.link_url)
                .bind(q.in_calendar)
                .execute(&mut **tx)
                .await?;
            }
        }
    }
    Ok(())
}
