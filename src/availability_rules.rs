//! Which windows a date offers: shared by the web slot grid and the CLI so
//! `calrs booking create` cannot accept a time the booking page would not offer.

/// One override row of a date: `(start_time, end_time, is_blocked)`.
pub(crate) type DayOverride = (Option<String>, Option<String>, i32);

/// The `(start, end)` `HH:MM` windows a date offers. A blocked override
/// leaves none; custom-hours overrides replace the weekly rules for that
/// date; with no override the weekday's rules apply.
pub(crate) fn effective_windows_for_day(
    weekly: &[(String, String)],
    overrides: &[DayOverride],
) -> Vec<(String, String)> {
    if overrides.iter().any(|(_, _, blocked)| *blocked != 0) {
        return Vec::new();
    }
    if overrides.is_empty() {
        return weekly.to_vec();
    }
    overrides
        .iter()
        .filter_map(|(s, e, _)| match (s, e) {
            (Some(start), Some(end)) => Some((start.clone(), end.clone())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weekly() -> Vec<(String, String)> {
        vec![("09:00".into(), "17:00".into())]
    }

    #[test]
    fn plain_day_uses_weekly_rules() {
        assert_eq!(effective_windows_for_day(&weekly(), &[]), weekly());
    }

    #[test]
    fn blocked_day_has_no_windows() {
        let o = vec![(None, None, 1)];
        assert!(effective_windows_for_day(&weekly(), &o).is_empty());
    }

    #[test]
    fn blocked_wins_over_custom_hours() {
        let o = vec![
            (Some("13:00".into()), Some("15:00".into()), 0),
            (None, None, 1),
        ];
        assert!(effective_windows_for_day(&weekly(), &o).is_empty());
    }

    #[test]
    fn custom_hours_replace_weekly_rules() {
        let o = vec![(Some("13:00".into()), Some("15:00".into()), 0)];
        assert_eq!(
            effective_windows_for_day(&weekly(), &o),
            vec![("13:00".to_string(), "15:00".to_string())]
        );
    }

    #[test]
    fn override_without_times_and_not_blocked_leaves_nothing() {
        let o = vec![(None, None, 0)];
        assert!(effective_windows_for_day(&weekly(), &o).is_empty());
    }
}
