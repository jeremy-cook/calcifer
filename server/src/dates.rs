//! Calendar days. The server stores and compares days as canonical ISO
//! "yyyy-MM-dd" strings: no time, no time zone.

/// Whether `s` is a real calendar day in canonical `yyyy-MM-dd` form (no padding
/// left out, no surrounding space). The one date check the server makes: date
/// property values, Resolve's date key and a `dateChip`'s `date` all use it (I-52).
pub(crate) fn is_iso_day(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .is_ok_and(|d| d.format("%Y-%m-%d").to_string() == s)
}

/// Format an ISO calendar day ("2026-06-13") as a long human date
/// ("June 13, 2026"). Falls back to the raw input on a parse failure.
pub(crate) fn format_long_date(date: &str) -> String {
    match chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(d) => d.format("%B %-d, %Y").to_string(),
        Err(_) => date.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_iso_day_accepts_only_canonical_real_days() {
        for good in ["2026-02-03", "2024-02-29"] {
            assert!(is_iso_day(good), "{good}");
        }
        for bad in [
            "2026-13-45",
            "2026-2-3",
            "2025-02-29",
            "June 13",
            "",
            " 2026-02-03",
        ] {
            assert!(!is_iso_day(bad), "{bad:?}");
        }
    }
}
