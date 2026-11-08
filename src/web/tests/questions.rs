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
