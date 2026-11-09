//! The leads as a CSV file.

use crate::leads::db::PartialBooking;

const HEADER: [&str; 17] = [
    "status",
    "created_at",
    "updated_at",
    "event_type",
    "team",
    "name",
    "email",
    "phone",
    "notes",
    "slot_date",
    "slot_time",
    "slot_timezone",
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "referrer",
    "contacted",
];

/// One cell. Quoted when it holds a separator, a quote or a line break. A cell
/// a spreadsheet would read as a formula (`=`, `+`, `-`, `@`, tab, return) gets
/// a leading apostrophe, so what a guest typed cannot run in the host's Excel.
fn cell(value: &str) -> String {
    let guarded = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if guarded.contains([',', '"', '\n', '\r', ';']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

fn status(lead: &PartialBooking) -> &'static str {
    if lead.archived_at.is_some() {
        "archived"
    } else if lead.completed_at.is_some() {
        "booked"
    } else {
        "open"
    }
}

/// The whole file, with a byte-order mark so Excel reads the accents.
pub(super) fn leads_csv(leads: &[PartialBooking]) -> String {
    let mut out = String::from("\u{feff}");
    out.push_str(&HEADER.join(","));
    out.push_str("\r\n");
    for lead in leads {
        let opt = |v: &Option<String>| cell(v.as_deref().unwrap_or(""));
        let fields = [
            status(lead).to_string(),
            cell(&lead.created_at),
            cell(&lead.updated_at),
            opt(&lead.event_type_title),
            opt(&lead.team_name),
            opt(&lead.name),
            opt(&lead.email),
            opt(&lead.phone),
            opt(&lead.notes),
            opt(&lead.target_date),
            opt(&lead.target_time),
            opt(&lead.target_tz),
            opt(&lead.utm_source),
            opt(&lead.utm_medium),
            opt(&lead.utm_campaign),
            opt(&lead.referrer),
            opt(&lead.contacted_at),
        ];
        out.push_str(&fields.join(","));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lead(name: &str, notes: &str) -> PartialBooking {
        PartialBooking {
            id: "1".into(),
            event_type_id: "e".into(),
            event_type_title: Some("Intro".into()),
            team_name: None,
            lead_id: "l".into(),
            name: Some(name.into()),
            email: Some("pat@example.com".into()),
            phone: None,
            notes: Some(notes.into()),
            target_date: None,
            target_time: None,
            target_tz: None,
            utm_source: None,
            utm_medium: None,
            utm_campaign: None,
            referrer: None,
            contacted_at: None,
            completed_at: None,
            archived_at: None,
            created_at: "2026-11-01 10:00:00".into(),
            updated_at: "2026-11-01 10:05:00".into(),
        }
    }

    #[test]
    fn cells_with_separators_are_quoted_and_quotes_doubled() {
        assert_eq!(cell("plain"), "plain");
        assert_eq!(cell("a,b"), "\"a,b\"");
        assert_eq!(cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(cell("two\nlines"), "\"two\nlines\"");
    }

    #[test]
    fn formulas_typed_by_a_guest_do_not_run_in_a_spreadsheet() {
        for payload in [
            "=1+1",
            "+cmd|' /C calc'!A0",
            "-2+3",
            "@SUM(A1)",
            "\t=1",
            "\r=1",
        ] {
            assert!(
                cell(payload).trim_start_matches('"').starts_with('\''),
                "{payload:?}"
            );
        }
        assert_eq!(cell("pat@example.com"), "pat@example.com");
    }

    #[test]
    fn the_file_has_a_header_and_one_line_per_lead() {
        let csv = leads_csv(&[lead("Pat", "hi"), lead("=HYPERLINK(\"x\")", "a,b")]);
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert!(lines[0].starts_with("\u{feff}status,created_at"));
        assert_eq!(lines.len(), 4, "header, two leads, trailing break");
        assert!(lines[1].starts_with("open,2026-11-01 10:00:00"));
        assert!(lines[2].contains("'=HYPERLINK"));
        assert!(lines[2].contains("\"a,b\""));
    }
}
