//! Upstream-compatible date/time vectors and exhaustive calendar round trips.

use rs_snap7::codec::datetime::*;

/// BCD fields and Sunday-based weekdays match the upstream documented vector.
#[test]
fn dt_vector_precision_and_century_window() {
    let bytes = [0x20, 0x07, 0x12, 0x17, 0x32, 0x02, 0x85, 0x41];
    let value = get_dt(&bytes, 0).unwrap();
    assert_eq!(value.date(), Date::new(2020, 7, 12).unwrap());
    assert_eq!(value.nanosecond(), 854_000_000);
    assert_eq!(value.date().weekday(), 1);
    let mut raw = [0; 8];
    set_dt(&mut raw, 0, value).unwrap();
    assert_eq!(raw, bytes);
    for year in [1990, 1999, 2000, 2089] {
        let value = DateTime::new(Date::new(year, 1, 1).unwrap(), 0, 0, 0, 999_999_999).unwrap();
        set_dt(&mut raw, 0, value).unwrap();
        let decoded = get_dt(&raw, 0).unwrap();
        assert_eq!(decoded.date(), value.date());
        assert_eq!(decoded.nanosecond(), 999_000_000);
    }
}

/// Every representable DATE day within the supported year range round-trips.
#[test]
fn date_epoch_leap_years_and_complete_range() {
    assert_eq!(
        get_date(&[0, 0], 0).unwrap(),
        Date::new(1990, 1, 1).unwrap()
    );
    assert_eq!(
        get_date(&[0x30, 0xd8], 0).unwrap(),
        Date::new(2024, 3, 27).unwrap()
    );
    for days in 0..=u16::MAX {
        let raw = days.to_be_bytes();
        if let Ok(date) = get_date(&raw, 0) {
            let mut result = [0; 2];
            set_date(&mut result, 0, date).unwrap();
            assert_eq!(result, raw);
        }
    }
    assert!(Date::new(1900, 2, 29).is_err());
    assert!(Date::new(2000, 2, 29).is_ok());
    assert!(Date::new(2024, 0, 1).is_err());
    assert!(get_date(&[255, 255], 0).is_err());
}

/// Long types preserve nanoseconds rather than truncating to Python microseconds.
#[test]
fn long_time_epoch_and_full_integer_limits() {
    for nanos in [0, 1, 1_000_000_001, u64::MAX] {
        let value = get_ldt(&nanos.to_be_bytes(), 0).unwrap();
        let mut raw = [0; 8];
        set_ldt(&mut raw, 0, value).unwrap();
        assert_eq!(raw, nanos.to_be_bytes());
    }
    for value in [i64::MIN, -1, 0, i64::MAX] {
        let mut raw = [0; 8];
        set_ltime(&mut raw, 0, value).unwrap();
        assert_eq!(get_ltime(&raw, 0).unwrap(), value);
    }
    let value = DateTime::new(Date::new(2024, 3, 27).unwrap(), 14, 30, 0, 123_456_789).unwrap();
    let mut raw = [0; 12];
    set_dtl(&mut raw, 0, value).unwrap();
    assert_eq!(raw, [7, 232, 3, 27, 4, 14, 30, 0, 7, 91, 205, 21]);
    assert_eq!(get_dtl(&raw, 0).unwrap(), value);
}

/// Duration and time-of-day limits remain distinct from signed TIME/LTIME.
#[test]
fn durations_and_time_of_day_bounds() {
    assert_eq!(get_s5time(&[0x31, 0x23], 0).unwrap(), 1_230_000);
    assert!(get_s5time(&[0x40, 0], 0).is_err());
    assert!(get_s5time(&[0x0a, 0], 0).is_err());
    let mut raw = [0; 8];
    set_time(&mut raw, 0, i32::MIN).unwrap();
    assert_eq!(get_time(&raw, 0).unwrap(), i32::MIN);
    set_tod(&mut raw, 0, 86_399_999).unwrap();
    assert_eq!(get_tod(&raw, 0).unwrap(), 86_399_999);
    assert!(set_tod(&mut raw, 0, 86_400_000).is_err());
    set_ltod(&mut raw, 0, 86_399_999_999_999).unwrap();
    assert_eq!(get_ltod(&raw, 0).unwrap(), 86_399_999_999_999);
    assert!(set_ltod(&mut raw, 0, 86_400_000_000_000).is_err());
}

/// Invalid BCD, missing bytes, and impossible calendars cannot panic or write.
#[test]
fn malformed_dates_and_out_of_range_writes() {
    let valid = [0x20, 7, 0x12, 0x17, 0x32, 2, 0x85, 0x41];
    for n in 0..8 {
        assert!(get_dt(&valid[..n], 0).is_err());
    }
    for index in 0..8 {
        let mut bytes = valid;
        bytes[index] = 0xff;
        assert!(get_dt(&bytes, 0).is_err());
    }
    let mut raw = [0xaa; 12];
    let old = DateTime::new(Date::new(1989, 1, 1).unwrap(), 0, 0, 0, 0).unwrap();
    assert!(set_dt(&mut raw, 0, old).is_err());
    assert!(set_date(&mut raw, 0, old.date()).is_err());
    assert!(get_dt(&valid, usize::MAX).is_err());
    assert!(DateTime::new(old.date(), 24, 0, 0, 0).is_err());
    assert!(DateTime::new(old.date(), 0, 0, 0, 1_000_000_000).is_err());
    assert_eq!(raw, [0xaa; 12]);
    let before_epoch = DateTime::new(Date::new(1969, 12, 31).unwrap(), 23, 59, 59, 0).unwrap();
    assert!(set_ldt(&mut raw, 0, before_epoch).is_err());
}
