//! Marketing attribution of a booking: the `utm_*` parameters of the link a
//! guest arrived by (`/u/alice/intro?utm_source=website&utm_campaign=pricing`).
//!
//! They travel from page to page in the URL, the way `invite` does (no
//! cookie, nothing kept in the visitor's browser), are posted with the
//! booking form as hidden fields, and are stored in `booking_attribution`
//! inside the booking's transaction. Hosts only: the bookings page, the host
//! notes page, webhooks and the REST API read them; nothing a guest, an
//! attendee or a calendar receives does (`guest_facing_code_never_reads_attribution`).

use serde::{Deserialize, Deserializer, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Longest value kept, in characters; a longer one is cut.
const MAX_LEN: usize = 100;

/// The five standard UTM parameters, each `None` when absent or blank.
/// Deserialized from any query or form map: other keys are ignored, values
/// are cleaned (`clean`), so whatever a visitor puts in the URL is bounded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub(crate) struct Attribution {
    pub(crate) utm_source: Option<String>,
    pub(crate) utm_medium: Option<String>,
    pub(crate) utm_campaign: Option<String>,
    pub(crate) utm_term: Option<String>,
    pub(crate) utm_content: Option<String>,
}

impl<'de> Deserialize<'de> for Attribution {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        HashMap::<String, String>::deserialize(deserializer).map(|map| Self::from_params(&map))
    }
}

/// Trimmed, without control characters, at most `MAX_LEN` characters;
/// `None` when nothing is left.
fn clean(value: &str) -> Option<String> {
    let value: String = super::questions::clean_text(value, false)
        .chars()
        .take(MAX_LEN)
        .collect();
    let value = value.trim_end();
    (!value.is_empty()).then(|| value.to_string())
}

impl Attribution {
    /// Reads the parameters out of a query string or a posted form.
    pub(crate) fn from_params(params: &HashMap<String, String>) -> Self {
        let get = |key: &str| params.get(key).and_then(|v| clean(v));
        Self {
            utm_source: get("utm_source"),
            utm_medium: get("utm_medium"),
            utm_campaign: get("utm_campaign"),
            utm_term: get("utm_term"),
            utm_content: get("utm_content"),
        }
    }

    /// The parameters that are set, by name, in the usual UTM order.
    pub(crate) fn pairs(&self) -> Vec<(&'static str, &str)> {
        [
            ("utm_source", &self.utm_source),
            ("utm_medium", &self.utm_medium),
            ("utm_campaign", &self.utm_campaign),
            ("utm_term", &self.utm_term),
            ("utm_content", &self.utm_content),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.as_deref().map(|v| (name, v)))
        .collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pairs().is_empty()
    }

    /// One line for a list row: the values set, "website · cpc · pricing".
    pub(crate) fn summary(&self) -> String {
        self.pairs()
            .into_iter()
            .map(|(_, v)| v)
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// Stores the attribution of a new booking, in its transaction. Nothing
    /// is written when no parameter is set.
    pub(crate) async fn insert(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        booking_id: &str,
    ) -> sqlx::Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO booking_attribution (booking_id, utm_source, utm_medium, utm_campaign, utm_term, utm_content)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(booking_id)
        .bind(&self.utm_source)
        .bind(&self.utm_medium)
        .bind(&self.utm_campaign)
        .bind(&self.utm_term)
        .bind(&self.utm_content)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

/// The attribution of several bookings by booking id, for a list page: one
/// query per hundred bookings. Bookings without one are absent.
pub(crate) async fn by_booking(
    pool: &SqlitePool,
    booking_ids: &[&str],
) -> HashMap<String, Attribution> {
    #[derive(sqlx::FromRow)]
    struct Row {
        booking_id: String,
        #[sqlx(flatten)]
        attribution: Attribution,
    }
    let mut by_booking = HashMap::new();
    for chunk in booking_ids.chunks(100) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let sql = format!(
            "SELECT booking_id, utm_source, utm_medium, utm_campaign, utm_term, utm_content
             FROM booking_attribution WHERE booking_id IN ({placeholders})"
        );
        let mut query = sqlx::query_as::<_, Row>(&sql);
        for id in chunk {
            query = query.bind(*id);
        }
        for row in query.fetch_all(pool).await.unwrap_or_default() {
            by_booking.insert(row.booking_id, row.attribution);
        }
    }
    by_booking
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn reads_the_five_parameters_and_ignores_the_rest() {
        let a = Attribution::from_params(&params(&[
            ("utm_source", "website"),
            ("utm_medium", "cpc"),
            ("utm_campaign", "pricing"),
            ("utm_term", "calendar"),
            ("utm_content", "hero"),
            ("utm_id", "42"),
            ("invite", "tok"),
        ]));
        assert_eq!(
            a.pairs(),
            vec![
                ("utm_source", "website"),
                ("utm_medium", "cpc"),
                ("utm_campaign", "pricing"),
                ("utm_term", "calendar"),
                ("utm_content", "hero"),
            ]
        );
        assert_eq!(a.summary(), "website · cpc · pricing · calendar · hero");
    }

    #[test]
    fn values_are_cleaned_and_bounded() {
        let long = "é".repeat(MAX_LEN + 20);
        let a = Attribution::from_params(&params(&[
            ("utm_source", "  news\u{0}letter\n "),
            ("utm_medium", "   "),
            ("utm_campaign", &long),
        ]));
        assert_eq!(a.utm_source.as_deref(), Some("newsletter"));
        assert_eq!(a.utm_medium, None);
        assert_eq!(a.utm_campaign.unwrap().chars().count(), MAX_LEN);
    }

    #[test]
    fn nothing_set_is_empty() {
        let a = Attribution::from_params(&params(&[("utm_source", ""), ("tz", "UTC")]));
        assert!(a.is_empty());
    }

    #[test]
    fn deserializes_from_a_query_string() {
        let uri = "/u/a/b?month=2026-11&utm_source=linked%20in&utm_campaign=q4"
            .parse()
            .unwrap();
        let axum::extract::Query(a) =
            axum::extract::Query::<Attribution>::try_from_uri(&uri).unwrap();
        assert_eq!(a.summary(), "linked in · q4");
    }
}
