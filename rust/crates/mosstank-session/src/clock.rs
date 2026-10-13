//! The client's clocks as the session's records read them (the C#'s
//! `TimeProvider`), and the times they write.

use openac_plugin_compat::datetime::DateTime;

/// Ticks (100 ns) per second.
const TICKS_PER_SECOND: i64 = 10_000_000;

/// The clocks a record reads.
pub trait SessionClock {
    /// Wall time, UTC, 100 ns ticks since the Unix epoch (`GetUtcNow`).
    fn utc_now_100ns(&self) -> i64;
    /// Seconds the local time is ahead of UTC (`LocalTimeZone`).
    fn local_offset_seconds(&self) -> i32;
    /// A clock that never goes back, milliseconds (`GetTimestamp`).
    fn monotonic_ms(&self) -> u64;
}

/// The wall time as `DateTime`, UTC.
#[must_use]
pub fn utc(ticks_since_epoch: i64) -> DateTime {
    DateTime::utc_from_unix_millis(0).add_ticks(ticks_since_epoch)
}

/// A `DateTimeOffset` at offset zero as System.Text.Json writes it:
/// `yyyy-MM-ddTHH:mm:ss`, the fraction of a second with its trailing zeros
/// dropped (none when it is zero), and `+00:00`.
#[must_use]
pub fn json_utc(ticks_since_epoch: i64) -> String {
    let time = utc(ticks_since_epoch);
    let whole = time
        .format("yyyy-MM-ddTHH:mm:ss")
        .expect("a pattern the kit writes");
    let fraction = ticks_since_epoch.rem_euclid(TICKS_PER_SECOND);
    if fraction == 0 {
        return format!("{whole}+00:00");
    }
    let digits = format!("{fraction:07}");
    format!("{whole}.{}+00:00", digits.trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::json_utc;

    #[test]
    fn a_time_is_written_as_system_text_json_writes_a_utc_offset() {
        assert_eq!(json_utc(0), "1970-01-01T00:00:00+00:00");
        assert_eq!(json_utc(70 * 10_000_000), "1970-01-01T00:01:10+00:00");
        assert_eq!(json_utc(1_234_500), "1970-01-01T00:00:00.12345+00:00");
    }
}
