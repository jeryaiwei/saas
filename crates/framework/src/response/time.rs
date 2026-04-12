//! Time formatting helpers for wire responses.
//!
//! NestJS `BaseResponseDto` uses dayjs `YYYY-MM-DD HH:mm:ss` in
//! `Asia/Shanghai` (UTC+8, no DST). Mirror that exactly so the Vue
//! web frontend parses both backends' strings identically.

use chrono::{DateTime, FixedOffset, Utc};

/// Format a UTC timestamp as `YYYY-MM-DD HH:mm:ss` in Asia/Shanghai.
/// UTC+8 has no DST, so `FixedOffset` matches `Asia/Shanghai` exactly
/// without pulling in the `chrono-tz` IANA database.
pub fn fmt_ts(ts: &DateTime<Utc>) -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("valid UTC+8 offset");
    ts.with_timezone(&offset)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn fmt_ts_formats_in_asia_shanghai() {
        let utc = Utc.with_ymd_and_hms(2026, 4, 11, 14, 0, 0).unwrap();
        assert_eq!(fmt_ts(&utc), "2026-04-11 22:00:00");
    }

    #[test]
    fn fmt_ts_handles_midnight_boundary() {
        let utc = Utc.with_ymd_and_hms(2026, 4, 11, 16, 0, 0).unwrap();
        assert_eq!(fmt_ts(&utc), "2026-04-12 00:00:00");
    }

    #[test]
    fn fmt_ts_handles_year_boundary() {
        let utc = Utc.with_ymd_and_hms(2026, 12, 31, 16, 0, 0).unwrap();
        assert_eq!(fmt_ts(&utc), "2027-01-01 00:00:00");
    }
}
