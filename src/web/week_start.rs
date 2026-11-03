//! Calendar display order, independent of availability's 0=Sunday storage.
//! Event types inherit from their account owner, including team events and
//! dynamic group links. Changing a display preference never rewrites hours.

use super::*;
use serde::{Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum WeekStart {
    Sun = 0,
    #[default]
    Mon = 1,
    Tue = 2,
    Wed = 3,
    Thu = 4,
    Fri = 5,
    Sat = 6,
}

impl WeekStart {
    pub(super) fn key(self) -> &'static str {
        match self {
            Self::Sun => "sun",
            Self::Mon => "mon",
            Self::Tue => "tue",
            Self::Wed => "wed",
            Self::Thu => "thu",
            Self::Fri => "fri",
            Self::Sat => "sat",
        }
    }

    pub(super) fn index(self) -> u32 {
        self as u32
    }

    pub(super) fn offset(self, date: NaiveDate) -> u32 {
        (date.weekday().num_days_from_sunday() + 7 - self.index()) % 7
    }
}

/// Missing field preserves the stored setting; empty explicitly inherits.
/// Invalid names are rejected by the form extractor before any writes.
pub(super) fn deserialize_preference<'de, D>(
    deserializer: D,
) -> Result<Option<Option<WeekStart>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        Ok(Some(None))
    } else {
        WeekStart::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(&value))
            .map(|day| Some(Some(day)))
    }
}

fn stored(value: Option<&str>) -> Option<WeekStart> {
    value.and_then(|value| {
        WeekStart::deserialize(
            serde::de::value::StrDeserializer::<serde::de::value::Error>::new(value),
        )
        .ok()
    })
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct WeekStartSetting {
    pub(super) value: Option<WeekStart>,
    pub(super) inherited: WeekStart,
}

impl WeekStartSetting {
    pub(super) fn effective(self) -> WeekStart {
        self.value.unwrap_or(self.inherited)
    }
}

pub(super) async fn site(pool: &SqlitePool) -> WeekStart {
    let value: Option<String> =
        sqlx::query_scalar("SELECT week_start FROM auth_config WHERE id = 'singleton'")
            .fetch_optional(pool)
            .await
            .unwrap_or(None);
    stored(value.as_deref()).unwrap_or_default()
}

pub(super) async fn user(pool: &SqlitePool, user_id: &str) -> WeekStartSetting {
    let value: Option<String> = sqlx::query_scalar("SELECT week_start FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten();
    WeekStartSetting {
        value: stored(value.as_deref()),
        inherited: site(pool).await,
    }
}

pub(super) async fn event(pool: &SqlitePool, event_id: &str) -> WeekStartSetting {
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT et.week_start, u.week_start FROM event_types et
         JOIN accounts a ON a.id = et.account_id
         LEFT JOIN users u ON u.id = a.user_id WHERE et.id = ?",
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);
    let (value, owner) = row.unwrap_or_default();
    WeekStartSetting {
        value: stored(value.as_deref()),
        inherited: match stored(owner.as_deref()) {
            Some(day) => day,
            None => site(pool).await,
        },
    }
}

pub(super) async fn new_event(pool: &SqlitePool, user_id: &str) -> WeekStartSetting {
    WeekStartSetting {
        value: None,
        inherited: user(pool, user_id).await.effective(),
    }
}
