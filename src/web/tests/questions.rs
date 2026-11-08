//! Booking questions (#210): the editor's validation, the parsing of a
//! booking POST per kind, and the questions through the web handlers and
//! every display surface.

use super::*;
use crate::web::questions::*;

fn editor(json: serde_json::Value) -> Result<Vec<Question>, EditorError> {
    parse_editor(&json.to_string(), &[])
}

fn err_key(json: serde_json::Value) -> &'static str {
    editor(json).expect_err("should be refused").key
}

fn q(kind: Kind, label: &str) -> Question {
    Question {
        id: format!("id-{label}"),
        key: key_from_label(label),
        kind,
        label: label.to_string(),
        help_text: None,
        required: false,
        options: Vec::new(),
        consent: false,
        link_url: None,
        in_calendar: true,
    }
}

fn choice(kind: Kind, label: &str, options: &[&str]) -> Question {
    Question {
        options: options
            .iter()
            .enumerate()
            .map(|(i, l)| QuestionOption {
                id: format!("o{}", i + 1),
                label: l.to_string(),
            })
            .collect(),
        ..q(kind, label)
    }
}

fn form(questions: Vec<Question>) -> BookingForm {
    BookingForm {
        notes_mode: "optional",
        questions,
    }
}

fn fields(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn editor_accepts_the_five_kinds_and_mints_ids_and_keys() {
    let qs = editor(serde_json::json!([
        {"kind": "short_text", "label": "VIN / licence plate", "required": true},
        {"kind": "long_text", "label": "Anything else?", "help": "Dietary needs"},
        {"kind": "single_choice", "label": "Where are the tyres?", "options": [{"label": "At the shop"}, {"label": "I bring them"}]},
        {"kind": "multiple_choice", "label": "Topics", "options": [{"label": "Tax"}, {"label": "Accounting"}]},
        {"kind": "checkbox", "label": "I agree to the privacy policy", "consent": true, "link": "https://example.com/privacy", "required": true, "in_calendar": false}
    ]))
    .unwrap();
    assert_eq!(qs.len(), 5);
    assert_eq!(qs[0].key, "vin_licence_plate");
    assert!(qs[0].required);
    assert_eq!(qs[1].help_text.as_deref(), Some("Dietary needs"));
    assert_eq!(qs[2].options[0].id, "o1");
    assert_eq!(qs[2].options[1].id, "o2");
    assert!(qs[4].consent);
    assert_eq!(
        qs[4].link_url.as_deref(),
        Some("https://example.com/privacy")
    );
    assert!(!qs[4].in_calendar);
    assert!(qs.iter().all(|q| uuid::Uuid::parse_str(&q.id).is_ok()));
}

#[test]
fn editor_enforces_the_limits() {
    let many: Vec<serde_json::Value> = (0..=MAX_QUESTIONS)
        .map(|i| serde_json::json!({"kind": "short_text", "label": format!("Q{i}")}))
        .collect();
    assert_eq!(
        err_key(serde_json::Value::Array(many)),
        "validate-questions-too-many"
    );
    assert_eq!(
        err_key(serde_json::json!([{"kind": "short_text", "label": "  "}])),
        "validate-question-label-empty"
    );
    assert_eq!(
        err_key(serde_json::json!([{"kind": "short_text", "label": "x".repeat(MAX_LABEL + 1)}])),
        "validate-question-label-too-long"
    );
    // A consent sentence may be longer than a question.
    assert!(editor(serde_json::json!([{"kind": "checkbox", "consent": true, "label": "x".repeat(MAX_LABEL + 1)}])).is_ok());
    assert_eq!(
        err_key(
            serde_json::json!([{"kind": "checkbox", "label": "x".repeat(MAX_CONSENT_LABEL + 1)}])
        ),
        "validate-question-label-too-long"
    );
    assert_eq!(
        err_key(
            serde_json::json!([{"kind": "short_text", "label": "A", "help": "h".repeat(MAX_HELP + 1)}])
        ),
        "validate-question-help-too-long"
    );
    assert_eq!(
        err_key(
            serde_json::json!([{"kind": "single_choice", "label": "A", "options": [{"label": "only"}]}])
        ),
        "validate-question-options-few"
    );
    let options: Vec<serde_json::Value> = (0..=MAX_OPTIONS)
        .map(|i| serde_json::json!({"label": format!("o{i}")}))
        .collect();
    assert_eq!(
        err_key(serde_json::json!([{"kind": "multiple_choice", "label": "A", "options": options}])),
        "validate-question-options-many"
    );
    assert_eq!(
        err_key(
            serde_json::json!([{"kind": "single_choice", "label": "A", "options": [{"label": "x"}, {"label": "x"}]}])
        ),
        "validate-question-option-duplicate"
    );
    assert_eq!(
        err_key(
            serde_json::json!([{"kind": "single_choice", "label": "A", "options": [{"label": "x"}, {"label": "y".repeat(MAX_OPTION_LABEL + 1)}]}])
        ),
        "validate-question-option-too-long"
    );
    assert_eq!(
        err_key(
            serde_json::json!([{"kind": "single_choice", "label": "A", "options": [{"label": "x"}, {"label": " "}]}])
        ),
        "validate-question-option-empty"
    );
    assert_eq!(
        err_key(serde_json::json!([{"kind": "date", "label": "When"}])),
        "validate-question-kind"
    );
    assert_eq!(
        parse_editor("not json", &[]).unwrap_err().key,
        "validate-questions-invalid"
    );
}

#[test]
fn editor_reports_the_position_of_the_refused_question() {
    let e = editor(serde_json::json!([
        {"kind": "short_text", "label": "Fine"},
        {"kind": "short_text", "label": ""}
    ]))
    .unwrap_err();
    assert_eq!(e.position, 2);
    assert!(e.message("en").contains('2'));
}

#[test]
fn editor_accepts_only_absolute_http_links() {
    for bad in [
        "javascript:alert(1)",
        "data:text/html,x",
        "/privacy",
        "ftp://example.com/p",
        "https://",
    ] {
        assert_eq!(
            err_key(
                serde_json::json!([{"kind": "checkbox", "consent": true, "label": "I agree", "link": bad}])
            ),
            "validate-question-link",
            "{bad}"
        );
    }
    let qs = editor(serde_json::json!([{"kind": "checkbox", "label": "I agree", "link": "http://example.com/p"}])).unwrap();
    assert_eq!(qs[0].link_url.as_deref(), Some("http://example.com/p"));
    // A link only belongs to a checkbox.
    let qs = editor(serde_json::json!([{"kind": "short_text", "label": "Company", "link": "https://example.com", "consent": true}])).unwrap();
    assert_eq!(qs[0].link_url, None);
    assert!(!qs[0].consent);
}

#[test]
fn editor_keeps_stored_ids_keys_and_options_and_refuses_a_kind_change() {
    let stored = vec![choice(
        Kind::SingleChoice,
        "Where are the tyres",
        &["At the shop", "I bring them"],
    )];
    let id = stored[0].id.clone();
    // Renamed, an option renamed and one added: the key and the kept ids stay.
    let qs = parse_editor(
        &serde_json::json!([{"id": id, "kind": "single_choice", "label": "Tyres", "options": [
            {"id": "o2", "label": "I will bring them"},
            {"id": "o1", "label": "At the shop"},
            {"id": "", "label": "New ones please"}
        ]}])
        .to_string(),
        &stored,
    )
    .unwrap();
    assert_eq!(qs[0].id, id);
    assert_eq!(qs[0].key, "where_are_the_tyres");
    let ids: Vec<&str> = qs[0].options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["o2", "o1", "o3"]);

    let e = parse_editor(
        &serde_json::json!([{"id": id, "kind": "short_text", "label": "Tyres"}]).to_string(),
        &stored,
    )
    .unwrap_err();
    assert_eq!(e.key, "validate-question-kind-change");

    // The same stored question twice is refused.
    let e = parse_editor(
        &serde_json::json!([
            {"id": id, "kind": "single_choice", "label": "A", "options": [{"label": "x"}, {"label": "y"}]},
            {"id": id, "kind": "single_choice", "label": "B", "options": [{"label": "x"}, {"label": "y"}]}
        ])
        .to_string(),
        &stored,
    )
    .unwrap_err();
    assert_eq!(e.key, "validate-questions-invalid");
}

#[test]
fn editor_never_takes_an_id_from_the_client() {
    // An id that is not one of this event type's questions (another event
    // type's, made up) is a new question with a fresh id.
    let qs = parse_editor(
        &serde_json::json!([{"id": "someone-elses-question", "kind": "short_text", "label": "Company"}]).to_string(),
        &[q(Kind::ShortText, "Other")],
    )
    .unwrap();
    assert_ne!(qs[0].id, "someone-elses-question");
    assert!(uuid::Uuid::parse_str(&qs[0].id).is_ok());
    // Option ids of a new question are minted too.
    let qs = editor(serde_json::json!([{"kind": "single_choice", "label": "A", "options": [{"id": "x\"y", "label": "x"}, {"id": "o1", "label": "y"}]}])).unwrap();
    let ids: Vec<&str> = qs[0].options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["o1", "o2"]);
}

#[test]
fn editor_dedupes_keys_and_keeps_stored_ones() {
    let stored = vec![q(Kind::ShortText, "Company")];
    let qs = parse_editor(
        &serde_json::json!([
            {"kind": "short_text", "label": "Company"},
            {"id": stored[0].id, "kind": "short_text", "label": "Firm"},
            {"kind": "short_text", "label": "Company!"}
        ])
        .to_string(),
        &stored,
    )
    .unwrap();
    assert_eq!(qs[0].key, "company_2");
    assert_eq!(qs[1].key, "company");
    assert_eq!(qs[2].key, "company_3");
}

#[test]
fn keys_from_labels() {
    assert_eq!(
        key_from_label("Where are the tyres located?"),
        "where_are_the_tyres_located"
    );
    assert_eq!(key_from_label("Événement"), "v_nement");
    assert_eq!(key_from_label("???"), "question");
    assert!(key_from_label(&"word ".repeat(40)).len() <= MAX_KEY);
}

#[test]
fn editor_strips_control_characters() {
    let qs = editor(serde_json::json!([{"kind": "short_text", "label": "Com\u{0}pa\nny\u{1b}", "help": "a\tb"}])).unwrap();
    assert_eq!(qs[0].label, "Company");
    assert_eq!(qs[0].help_text.as_deref(), Some("ab"));
}

#[test]
fn an_empty_editor_field_clears_the_questions() {
    assert_eq!(parse_editor("", &[]).unwrap(), Vec::new());
    assert_eq!(parse_editor("[]", &[]).unwrap(), Vec::new());
}

#[test]
fn editor_json_round_trips() {
    let stored = vec![
        choice(Kind::MultipleChoice, "Topics", &["Tax", "Accounting"]),
        Question {
            consent: true,
            link_url: Some("https://example.com/p".into()),
            in_calendar: false,
            ..q(Kind::Checkbox, "I agree")
        },
    ];
    let back = parse_editor(&editor_json(&stored), &stored).unwrap();
    assert_eq!(back, stored);
}

// --- Booking POST ---

#[test]
fn text_answers_are_trimmed_cleaned_and_bounded() {
    let f = form(vec![
        Question {
            required: true,
            ..q(Kind::ShortText, "Plate")
        },
        q(Kind::LongText, "More"),
    ]);
    let s = parse_submission(
        &f,
        None,
        &fields(&[
            ("q_id-Plate", "  AB-123\u{0}  "),
            ("q_id-More", "line one\r\nline\u{7} two"),
        ]),
    )
    .unwrap();
    assert_eq!(s.answers[0].answer.value, "AB-123");
    assert_eq!(s.answers[1].answer.value, "line one\nline two");
    assert_eq!(s.answers[0].question_id, "id-Plate");
    assert_eq!(s.answers[0].answer.key, "plate");

    assert_eq!(
        parse_submission(&f, None, &fields(&[("q_id-Plate", "   ")])),
        Err(Refusal::Required("Plate".into()))
    );
    let long = "x".repeat(MAX_SHORT_ANSWER + 1);
    assert_eq!(
        parse_submission(&f, None, &fields(&[("q_id-Plate", &long)])),
        Err(Refusal::TooLong("Plate".into()))
    );
    let long = "x".repeat(MAX_LONG_ANSWER + 1);
    assert_eq!(
        parse_submission(
            &f,
            None,
            &fields(&[("q_id-Plate", "A"), ("q_id-More", &long)])
        ),
        Err(Refusal::TooLong("More".into()))
    );
    // An optional question left empty stores nothing.
    let s = parse_submission(&f, None, &fields(&[("q_id-Plate", "A")])).unwrap();
    assert_eq!(s.answers.len(), 1);
}

#[test]
fn single_choice_takes_one_offered_option() {
    let f = form(vec![Question {
        required: true,
        ..choice(
            Kind::SingleChoice,
            "Tyres",
            &["At the shop", "I bring them"],
        )
    }]);
    let s = parse_submission(&f, None, &fields(&[("q_id-Tyres", "o2")])).unwrap();
    assert_eq!(s.answers[0].answer.value, "I bring them");
    assert_eq!(s.answers[0].answer.option_ids, ["o2"]);
    assert_eq!(
        parse_submission(&f, None, &fields(&[("q_id-Tyres", "o9")])),
        Err(Refusal::UnknownOption("Tyres".into()))
    );
    assert_eq!(
        parse_submission(&f, None, &fields(&[])),
        Err(Refusal::Required("Tyres".into()))
    );
}

#[test]
fn multiple_choice_reads_one_field_per_box() {
    let f = form(vec![Question {
        required: true,
        ..choice(
            Kind::MultipleChoice,
            "Topics",
            &["Tax", "Accounting", "Strategy"],
        )
    }]);
    let s = parse_submission(
        &f,
        None,
        &fields(&[("q_id-Topics__o3", "1"), ("q_id-Topics__o1", "1")]),
    )
    .unwrap();
    // In the host's order, whatever the order posted.
    assert_eq!(s.answers[0].answer.value, "Tax, Strategy");
    assert_eq!(s.answers[0].answer.option_ids, ["o1", "o3"]);
    assert_eq!(
        parse_submission(
            &f,
            None,
            &fields(&[("q_id-Topics__o1", "1"), ("q_id-Topics__bogus", "1")])
        ),
        Err(Refusal::UnknownOption("Topics".into()))
    );
    assert_eq!(
        parse_submission(&f, None, &fields(&[])),
        Err(Refusal::Required("Topics".into()))
    );
}

#[test]
fn an_unticked_optional_checkbox_is_stored_as_no() {
    let f = form(vec![
        Question {
            consent: true,
            ..q(Kind::Checkbox, "Recording")
        },
        Question {
            required: true,
            consent: true,
            link_url: Some("https://example.com/p".into()),
            ..q(Kind::Checkbox, "Privacy")
        },
    ]);
    let s = parse_submission(&f, None, &fields(&[("q_id-Privacy", "1")])).unwrap();
    assert_eq!(s.answers.len(), 2);
    assert_eq!(s.answers[0].answer.value, "no");
    assert!(s.answers[0].answer.consent);
    assert_eq!(s.answers[1].answer.value, "yes");
    assert_eq!(
        s.answers[1].answer.link_url.as_deref(),
        Some("https://example.com/p")
    );
    assert!(!s.answers[1].answer.answered_at.is_empty());
    assert_eq!(
        parse_submission(&f, None, &fields(&[])),
        Err(Refusal::Required("Privacy".into()))
    );
}

#[test]
fn unknown_fields_are_ignored() {
    let f = form(vec![q(Kind::ShortText, "Company")]);
    let s = parse_submission(
        &f,
        None,
        &fields(&[("q_not-a-question", "x"), ("whatever", "y")]),
    )
    .unwrap();
    assert!(s.answers.is_empty());
}

#[test]
fn notes_follow_the_notes_mode() {
    let mut f = form(Vec::new());
    // Optional: as posted, exactly as before #210.
    let s = parse_submission(&f, Some(""), &fields(&[])).unwrap();
    assert_eq!(s.notes.as_deref(), Some(""));
    f.notes_mode = "off";
    let s = parse_submission(&f, Some("sneaked in"), &fields(&[])).unwrap();
    assert_eq!(s.notes, None);
    f.notes_mode = "required";
    assert_eq!(
        parse_submission(&f, Some("  "), &fields(&[])),
        Err(Refusal::NotesRequired)
    );
    let s = parse_submission(&f, Some("Hello"), &fields(&[])).unwrap();
    assert_eq!(s.notes.as_deref(), Some("Hello"));
    assert_eq!(notes_mode("bogus"), "optional");
}

#[test]
fn refusals_name_the_question_in_every_language() {
    for lang in ["en", "fr", "de", "de-formal", "pl"] {
        let m = Refusal::Required("Plate".into()).message(lang);
        assert!(m.contains("Plate"), "{lang}: {m}");
    }
}

#[test]
fn form_context_marks_long_single_choices_as_select() {
    let few = choice(Kind::SingleChoice, "Few", &["a", "b"]);
    let many = choice(Kind::SingleChoice, "Many", &["a", "b", "c", "d", "e", "f"]);
    let ctx = form_context(&form(vec![few, many]));
    assert!(!ctx[0].get_attr("as_select").unwrap().is_true());
    assert!(ctx[1].get_attr("as_select").unwrap().is_true());
    assert_eq!(ctx[1].get_attr("name").unwrap().as_str(), Some("q_id-Many"));
}

/// Answers can be sensitive: the module never logs.
#[test]
fn the_module_logs_nothing() {
    let src = include_str!("../questions.rs");
    let code = src.split("#[cfg(test)]").next().unwrap();
    for mac in [
        "tracing::",
        "println!",
        "eprintln!",
        "dbg!",
        "info!(",
        "warn!(",
        "debug!(",
    ] {
        assert!(!code.contains(mac), "questions.rs uses {mac}");
    }
}

#[test]
fn answers_are_written_out_per_reader() {
    use crate::email::{answer_display, AnswerReader, BookingAnswer};
    let consent = |value: &str| BookingAnswer {
        kind: "checkbox".into(),
        consent: true,
        value: value.into(),
        ..Default::default()
    };
    assert_eq!(
        answer_display(&consent("yes"), "en", AnswerReader::Host),
        "Agreed"
    );
    assert_eq!(
        answer_display(&consent("no"), "en", AnswerReader::Host),
        "Not agreed"
    );
    assert_eq!(
        answer_display(&consent("yes"), "en", AnswerReader::Guest),
        "You agreed"
    );
    assert_eq!(
        answer_display(&consent("yes"), "de-formal", AnswerReader::Guest),
        "Sie haben zugestimmt"
    );
    let plain = BookingAnswer {
        kind: "checkbox".into(),
        value: "no".into(),
        ..Default::default()
    };
    assert_eq!(answer_display(&plain, "fr", AnswerReader::Guest), "Non");
    let text = BookingAnswer {
        kind: "short_text".into(),
        value: "AB-123".into(),
        ..Default::default()
    };
    assert_eq!(answer_display(&text, "pl", AnswerReader::Host), "AB-123");
}

/// The editor's refusals reach Fluent through a variable, out of the Rust
/// key guard's sight: each one is checked here, in every locale.
#[test]
fn every_editor_refusal_is_translated() {
    let keys = [
        "validate-questions-invalid",
        "validate-questions-too-many",
        "validate-question-kind",
        "validate-question-kind-change",
        "validate-question-label-empty",
        "validate-question-label-too-long",
        "validate-question-help-too-long",
        "validate-question-options-few",
        "validate-question-options-many",
        "validate-question-option-empty",
        "validate-question-option-too-long",
        "validate-question-option-duplicate",
        "validate-question-link",
    ];
    let src = include_str!("../questions.rs");
    for key in keys {
        assert!(src.contains(&format!("\"{key}\"")), "{key} is not used");
        for lang in ["en", "fr", "de", "es", "it", "pl", "et", "pt", "nl"] {
            let text = EditorError { key, position: 3 }.message(lang);
            assert_ne!(text, key, "{lang}: {key} missing");
        }
    }
    let message = EditorError {
        key: "validate-questions-too-many",
        position: 0,
    }
    .message("en");
    assert!(message.contains("20"), "{message}");
}

// --- Through the web handlers ---

/// The five kinds on an event type: a required short text, a single choice,
/// a multiple choice, a required consent with a policy link, and an
/// optional checkbox kept out of the calendar event.
async fn seed_questions(pool: &SqlitePool, et_id: &str) -> Vec<Question> {
    let json = serde_json::json!([
        {"kind": "short_text", "label": "Licence plate", "required": true},
        {"kind": "single_choice", "label": "Where are the tyres", "options": [{"label": "At the shop"}, {"label": "I bring them"}]},
        {"kind": "multiple_choice", "label": "Topics", "options": [{"label": "Tax"}, {"label": "Accounting"}, {"label": "Strategy"}]},
        {"kind": "checkbox", "label": "I agree to the privacy policy", "consent": true, "required": true, "link": "https://example.com/privacy"},
        {"kind": "checkbox", "label": "Area of concern: housing", "in_calendar": false}
    ]);
    let qs = parse_editor(&json.to_string(), &[]).unwrap();
    save(pool, et_id, Some(&qs), None).await.unwrap();
    load(pool, et_id).await.unwrap()
}

/// The fields a browser posts for `seed_questions` with every box ticked
/// but the last.
fn answer_fields(qs: &[Question]) -> String {
    format!(
        "&q_{}=AB-123&q_{}=o2&q_{}__o1=1&q_{}__o3=1&q_{}=1",
        qs[0].id, qs[1].id, qs[2].id, qs[2].id, qs[3].id
    )
}

async fn answers_of(pool: &SqlitePool, et_id: &str) -> Vec<(String, String, String, bool)> {
    sqlx::query_as(
        "SELECT ba.label, ba.value, COALESCE(ba.value_ids, ''), ba.in_calendar
         FROM booking_answers ba JOIN bookings b ON b.id = ba.booking_id
         WHERE b.event_type_id = ? ORDER BY ba.position",
    )
    .bind(et_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn bookings_of(pool: &SqlitePool, et_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM bookings WHERE event_type_id = ?")
        .bind(et_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn assert_stored(answers: &[(String, String, String, bool)]) {
    let got: Vec<(&str, &str)> = answers
        .iter()
        .map(|(l, v, _, _)| (l.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("Licence plate", "AB-123"),
            ("Where are the tyres", "I bring them"),
            ("Topics", "Tax, Strategy"),
            ("I agree to the privacy policy", "yes"),
            ("Area of concern: housing", "no"),
        ]
    );
    assert_eq!(answers[2].2, r#"["o1","o3"]"#);
    assert!(!answers[4].3, "in_calendar is snapshotted");
}

#[tokio::test]
async fn the_booking_form_renders_the_questions_accessibly() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let qs = seed_questions(&pool, &et_id).await;
    let body = body_string(app.oneshot(get(&booking_form_uri(None))).await.unwrap()).await;
    assert!(body.contains(&format!(r#"name="q_{}""#, qs[0].id)));
    assert!(
        body.contains(r#"<label for="q0">Licence plate</label>"#),
        "{body}"
    );
    // A choice group is a fieldset with a legend; each box its own name.
    assert!(body.contains("<legend>Where are the tyres"));
    assert!(body.contains(&format!(r#"name="q_{}__o3""#, qs[2].id)));
    // The required consent is natively required and links its policy.
    let consent = &body[body.find(&format!(r#"name="q_{}""#, qs[3].id)).unwrap()..];
    assert!(consent[..80].contains("required"), "{}", &consent[..80]);
    assert!(body.contains(
        r#"href="https:&#x2f;&#x2f;example.com&#x2f;privacy" target="_blank" rel="noopener noreferrer""#
    ));
    // In the host's order, before the notes.
    let first = body.find("Licence plate").unwrap();
    let last = body.find("Area of concern").unwrap();
    let notes = body.find(r#"id="notes""#).unwrap();
    assert!(first < last && last < notes);
}

#[tokio::test]
async fn the_notes_field_follows_its_mode() {
    let (app, pool, _, et_id) = setup_test_app().await;
    save(&pool, &et_id, None, Some("off")).await.unwrap();
    let body = body_string(
        app.clone()
            .oneshot(get(&booking_form_uri(None)))
            .await
            .unwrap(),
    )
    .await;
    assert!(!body.contains(r#"name="notes""#));
    save(&pool, &et_id, None, Some("required")).await.unwrap();
    let body = body_string(
        app.clone()
            .oneshot(get(&booking_form_uri(None)))
            .await
            .unwrap(),
    )
    .await;
    assert!(body.contains(r#"placeholder"#));
    let notes = &body[body.find(r#"id="notes""#).unwrap()..];
    assert!(notes[..200].contains(" required"));

    // Required: an empty note refuses the booking.
    let csrf = "csrf-q-notes";
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &format!("{}&notes=+", book_body(csrf)),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(bookings_of(&pool, &et_id).await, 0);

    // Off: a posted note is not stored.
    save(&pool, &et_id, None, Some("off")).await.unwrap();
    app.oneshot(post_form_unauthed(
        "/u/testuser/test-meeting/book",
        csrf,
        &format!("{}&notes=sneaked", book_body(csrf)),
    ))
    .await
    .unwrap();
    let notes: Option<String> =
        sqlx::query_scalar("SELECT notes FROM bookings WHERE event_type_id = ?")
            .bind(&et_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(notes, None);
}

#[tokio::test]
async fn a_personal_booking_stores_the_answers_with_the_booking() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let qs = seed_questions(&pool, &et_id).await;
    let csrf = "csrf-q-user";
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &format!("{}{}", book_body(csrf), answer_fields(&qs)),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let page = body_string(response).await;
    assert_stored(&answers_of(&pool, &et_id).await);
    // The guest reads their own answers on the confirmation page.
    assert!(page.contains("AB-123"));
    assert!(page.contains("You agreed"));
    assert!(page.contains("I bring them"));
}

#[tokio::test]
async fn the_legacy_route_stores_the_answers() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let qs = seed_questions(&pool, &et_id).await;
    let csrf = "csrf-q-legacy";
    let response = app
        .oneshot(post_form_unauthed(
            "/test-meeting/book",
            csrf,
            &format!("{}{}", book_body(csrf), answer_fields(&qs)),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_stored(&answers_of(&pool, &et_id).await);
}

#[tokio::test]
async fn a_team_booking_stores_the_answers() {
    let (app, pool, _, _) = setup_test_app().await;
    let (_team_id, et_id) = seed_team_event_type(&pool).await;
    let qs = seed_questions(&pool, &et_id).await;
    let csrf = "csrf-q-team";
    let response = app
        .oneshot(post_form_unauthed(
            "/team/horizon-team/team-sync/book",
            csrf,
            &format!("{}{}", book_body(csrf), answer_fields(&qs)),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_stored(&answers_of(&pool, &et_id).await);
}

#[tokio::test]
async fn a_dynamic_group_booking_stores_the_answers() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let qs = seed_questions(&pool, &et_id).await;
    sqlx::query("UPDATE users SET allow_dynamic_group = 1 WHERE username = 'testuser'")
        .execute(&pool)
        .await
        .unwrap();
    let (viewer, _) =
        insert_test_user_with_session(&pool, "Viewer", "viewer@example.com", "viewer").await;
    sqlx::query("UPDATE users SET allow_dynamic_group = 1 WHERE id = ?")
        .bind(&viewer)
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "csrf-q-dyn";
    let response = app
        .oneshot(post_form_unauthed(
            "/u/testuser+viewer/test-meeting/book",
            csrf,
            &format!("{}{}", book_body(csrf), answer_fields(&qs)),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(bookings_of(&pool, &et_id).await, 1);
    assert_stored(&answers_of(&pool, &et_id).await);
}

#[tokio::test]
async fn a_missing_required_answer_leaves_no_booking() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let qs = seed_questions(&pool, &et_id).await;
    let csrf = "csrf-q-missing";
    // Everything but the required consent.
    let fields = format!("&q_{}=AB-123", qs[0].id);
    let response = app
        .clone()
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &format!("{}{fields}", book_body(csrf)),
        ))
        .await
        .unwrap();
    let page = body_string(response).await;
    assert!(page.contains("I agree to the privacy policy"), "{page}");
    assert_eq!(bookings_of(&pool, &et_id).await, 0);
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM booking_answers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 0);

    // An option the question does not offer is refused too.
    let fields = format!("{}&q_{}=o9", answer_fields(&qs), qs[1].id);
    let fields = fields.replacen(&format!("&q_{}=o2", qs[1].id), "", 1);
    app.oneshot(post_form_unauthed(
        "/u/testuser/test-meeting/book",
        csrf,
        &format!("{}{fields}", book_body(csrf)),
    ))
    .await
    .unwrap();
    assert_eq!(bookings_of(&pool, &et_id).await, 0);
}

/// A private event type's questions are read only once the invite is
/// checked: a POST without one never hears what the form asks.
#[tokio::test]
async fn a_private_event_types_questions_stay_behind_the_invite() {
    let (app, pool, _, et_id) = setup_test_app().await;
    seed_questions(&pool, &et_id).await;
    sqlx::query("UPDATE event_types SET visibility = 'private' WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let csrf = "csrf-q-private";
    let page = body_string(
        app.oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &book_body(csrf),
        ))
        .await
        .unwrap(),
    )
    .await;
    assert!(!page.contains("Licence plate"), "{page}");
    assert_eq!(bookings_of(&pool, &et_id).await, 0);
}

/// With notes hidden, a posted note reaches neither the row nor the host.
#[tokio::test]
async fn a_hidden_notes_field_keeps_a_posted_note_out_of_the_emails() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    save(&pool, &et_id, None, Some("off")).await.unwrap();
    let csrf = "csrf-q-off-mail";
    app.oneshot(post_form_unauthed(
        "/u/testuser/test-meeting/book",
        csrf,
        &format!("{}&notes=sneaked+in", book_body(csrf)),
    ))
    .await
    .unwrap();
    let host = mail_to(&mails, "test@example.com").await;
    assert!(!host.contains("sneaked"));
}

#[tokio::test]
async fn editing_or_deleting_a_question_keeps_past_answers() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let qs = seed_questions(&pool, &et_id).await;
    let csrf = "csrf-q-snap";
    app.oneshot(post_form_unauthed(
        "/u/testuser/test-meeting/book",
        csrf,
        &format!("{}{}", book_body(csrf), answer_fields(&qs)),
    ))
    .await
    .unwrap();
    // Rename the first question, drop the others.
    let json = serde_json::json!([{"id": qs[0].id, "kind": "short_text", "label": "Plate number"}]);
    let kept = parse_editor(&json.to_string(), &qs).unwrap();
    save(&pool, &et_id, Some(&kept), None).await.unwrap();
    assert_eq!(load(&pool, &et_id).await.unwrap().len(), 1);
    assert_stored(&answers_of(&pool, &et_id).await);
    let orphans: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM booking_answers WHERE question_id IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(orphans, 4);
}

// --- Display surfaces ---

#[tokio::test]
async fn the_bookings_page_shows_the_answers_to_the_host() {
    let (app, pool, session, et_id) = setup_test_app().await;
    let qs = seed_questions(&pool, &et_id).await;
    let csrf = "csrf-q-dash";
    app.clone()
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &format!("{}{}", book_body(csrf), answer_fields(&qs)),
        ))
        .await
        .unwrap();
    let page = body_string(
        app.oneshot(get_authed("/dashboard/bookings", &session))
            .await
            .unwrap(),
    )
    .await;
    assert!(page.contains(r#"<details class="booking-answers">"#));
    assert!(
        page.contains("<dt>Licence plate</dt><dd>AB-123</dd>"),
        "{page}"
    );
    // The host reads a consent as "Agreed", an unticked box as "No".
    assert!(page.contains("<dd>Agreed</dd>"));
    assert!(page.contains("<dd>No</dd>"));
}

#[tokio::test]
async fn emails_carry_the_answers_to_the_host_and_the_guest_only() {
    let (app, pool, _, et_id) = setup_test_app().await;
    let (port, mails) = spawn_capturing_mta().await;
    use_capturing_mta(&pool, port).await;
    sqlx::query("UPDATE event_types SET max_additional_guests = 3 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let qs = seed_questions(&pool, &et_id).await;
    let csrf = "csrf-q-mail";
    app.oneshot(post_form_unauthed(
        "/u/testuser/test-meeting/book",
        csrf,
        &format!(
            "{}{}&additional_guests=friend%40test.com",
            book_body(csrf),
            answer_fields(&qs)
        ),
    ))
    .await
    .unwrap();
    let host = mail_to(&mails, "test@example.com").await;
    assert!(host.contains("Licence plate"));
    assert!(host.contains("AB-123"));
    assert!(host.contains("Agreed"));
    let guest = mail_to(&mails, "guest@test.com").await;
    assert!(guest.contains("AB-123"));
    assert!(guest.contains("You agreed"));
    let subject = mail_header(&guest, "Subject").unwrap_or_default();
    assert!(!subject.contains("AB-123"));
    let attendee = mail_to(&mails, "friend@test.com").await;
    assert!(
        !attendee.contains("AB-123"),
        "attendee copies carry no answers"
    );
    assert!(!attendee.contains("Licence plate"));
}

#[tokio::test]
async fn an_approval_carries_the_answers_into_the_confirmed_emails() {
    let (app, pool, _, et_id) = setup_test_app().await;
    sqlx::query("UPDATE event_types SET requires_confirmation = 1 WHERE id = ?")
        .bind(&et_id)
        .execute(&pool)
        .await
        .unwrap();
    let qs = seed_questions(&pool, &et_id).await;
    let csrf = "csrf-q-approve";
    app.clone()
        .oneshot(post_form_unauthed(
            "/u/testuser/test-meeting/book",
            csrf,
            &format!("{}{}", book_body(csrf), answer_fields(&qs)),
        ))
        .await
        .unwrap();
    let uid: String = sqlx::query_scalar("SELECT uid FROM bookings WHERE event_type_id = ?")
        .bind(&et_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    // Every rebuild of the booking reads them through `booking_guest_input`.
    let input = crate::web::booking_guest_input(&pool, &uid).await;
    assert_eq!(input.answers.len(), 5);
    assert_eq!(input.answers[0].value, "AB-123");
    assert_eq!(
        input.answers[3].link_url.as_deref(),
        Some("https://example.com/privacy")
    );
}

// --- Calendar event and email attachment ---

fn details_with_answers() -> crate::email::BookingDetails {
    crate::email::BookingDetails {
        event_title: "Tyres".into(),
        date: "2026-12-01".into(),
        start_time: "10:00".into(),
        end_time: "10:30".into(),
        guest_name: "Guest".into(),
        guest_email: "guest@test.com".into(),
        guest_timezone: "UTC".into(),
        host_name: "Host".into(),
        host_email: "host@test.com".into(),
        uid: "u@calrs".into(),
        answers: vec![
            crate::email::BookingAnswer {
                kind: "long_text".into(),
                label: "Situation".into(),
                value: format!("{}\nsecond line", "word ".repeat(40)),
                in_calendar: true,
                ..Default::default()
            },
            crate::email::BookingAnswer {
                kind: "checkbox".into(),
                label: "Recording".into(),
                value: "yes".into(),
                consent: true,
                in_calendar: true,
                ..Default::default()
            },
            crate::email::BookingAnswer {
                kind: "short_text".into(),
                label: "Area of concern".into(),
                value: "Eviction".into(),
                in_calendar: false,
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn unfold(ics: &str) -> String {
    ics.replace("\r\n ", "")
}

#[test]
fn the_host_calendar_copy_carries_the_answers_flagged_for_it() {
    let ics = crate::email::generate_ics_caldav(&details_with_answers());
    for line in ics.split("\r\n") {
        assert!(line.len() <= 75, "folded: {line:?}");
    }
    let ics = unfold(&ics);
    assert!(ics.contains("Situation: word word"));
    // A line break in an answer cannot break the property: a space, as for
    // the notes.
    assert!(ics.contains("word  second line"));
    assert!(ics.contains("Recording: Agreed"));
    assert!(!ics.contains("Eviction"), "kept out of the calendar event");
    assert!(!ics.contains("Area of concern"));
}

#[test]
fn the_emailed_invite_carries_no_answers() {
    let ics = unfold(&crate::email::generate_ics(
        &details_with_answers(),
        "REQUEST",
    ));
    assert!(!ics.contains("Situation"));
    assert!(!ics.contains("Recording"));
}
