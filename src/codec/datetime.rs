//! Calendar and duration codecs without runtime or timezone dependencies.
//!
//! Unlike Python's microsecond-limited datetime, long types retain nanoseconds.
//! TIME uses signed milliseconds, LTIME signed nanoseconds, TOD/LTOD unsigned
//! units since midnight. S5TIME is read-only, matching upstream support.

use super::{range, read, write};
use crate::{Error, Result};

/// A validated Gregorian calendar date, without a timezone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Date {
    /// Validate a date in years 1 through 9999, including Gregorian leap years.
    pub fn new(year: u16, month: u8, day: u8) -> Result<Self> {
        if !(1..=9999).contains(&year)
            || !(1..=12).contains(&month)
            || day == 0
            || day > month_days(year, month)
        {
            return Err(Error::InvalidInput("invalid calendar date"));
        }
        Ok(Self { year, month, day })
    }
    /// Full Gregorian year.
    pub const fn year(self) -> u16 {
        self.year
    }
    /// Calendar month, 1 through 12.
    pub const fn month(self) -> u8 {
        self.month
    }
    /// Day of month, starting at 1.
    pub const fn day(self) -> u8 {
        self.day
    }
    /// S7 weekday convention: Sunday=1 through Saturday=7.
    pub fn weekday(self) -> u8 {
        ((self.ordinal() + 1) % 7 + 1) as u8
    }

    /// Zero-based day count from 0001-01-01 (a Monday).
    fn ordinal(self) -> i64 {
        let y = i64::from(self.year) - 1;
        let days: i64 = (1..self.month)
            .map(|m| i64::from(month_days(self.year, m)))
            .sum();
        365 * y + y / 4 - y / 100 + y / 400 + days + i64::from(self.day) - 1
    }

    /// Recover a Gregorian date from a bounded day count using binary search.
    fn from_ordinal(ordinal: i64) -> Result<Self> {
        if !(0..=3_652_058).contains(&ordinal) {
            return Err(Error::InvalidInput("date outside supported calendar"));
        }
        let (mut low, mut high) = (1u16, 9999u16);
        while low < high {
            let mid = low + (high - low).div_ceil(2);
            if (Self {
                year: mid,
                month: 1,
                day: 1,
            })
            .ordinal()
                <= ordinal
            {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        let mut remaining = ordinal
            - Self {
                year: low,
                month: 1,
                day: 1,
            }
            .ordinal();
        let mut month = 1;
        while remaining >= i64::from(month_days(low, month)) {
            remaining -= i64::from(month_days(low, month));
            month += 1;
        }
        Self::new(low, month, remaining as u8 + 1)
    }
}

/// Return the number of days in a validated month.
fn month_days(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A validated local/PLC date and time with nanosecond precision, without timezone conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateTime {
    date: Date,
    hour: u8,
    minute: u8,
    second: u8,
    nanos: u32,
}

impl DateTime {
    /// Validate clock fields; leap seconds are not representable by classic S7.
    pub fn new(date: Date, hour: u8, minute: u8, second: u8, nanos: u32) -> Result<Self> {
        if hour > 23 || minute > 59 || second > 59 || nanos >= 1_000_000_000 {
            return Err(Error::InvalidInput("invalid clock fields"));
        }
        Ok(Self {
            date,
            hour,
            minute,
            second,
            nanos,
        })
    }
    /// Calendar date.
    pub const fn date(self) -> Date {
        self.date
    }
    /// Hour, 0 through 23.
    pub const fn hour(self) -> u8 {
        self.hour
    }
    /// Minute, 0 through 59.
    pub const fn minute(self) -> u8 {
        self.minute
    }
    /// Second, 0 through 59.
    pub const fn second(self) -> u8 {
        self.second
    }
    /// Nanoseconds within the second.
    pub const fn nanosecond(self) -> u32 {
        self.nanos
    }
}

/// Decode one strict BCD byte, rejecting nondecimal nibbles.
pub(crate) fn from_bcd(value: u8) -> Result<u8> {
    if value >> 4 > 9 || value & 15 > 9 {
        return Err(Error::InvalidInput("invalid BCD digit"));
    }
    Ok((value >> 4) * 10 + (value & 15))
}

/// Encode a value known to be below 100.
fn bcd(value: u8) -> u8 {
    (value / 10) << 4 | (value % 10)
}

/// Read a signed TIME duration in milliseconds.
pub fn get_time(data: &[u8], offset: usize) -> Result<i32> {
    read(data, offset)
}
/// Write a signed TIME duration in milliseconds.
pub fn set_time(data: &mut [u8], offset: usize, millis: i32) -> Result<()> {
    write(data, offset, millis)
}
/// Read a signed LTIME duration in nanoseconds.
pub fn get_ltime(data: &[u8], offset: usize) -> Result<i64> {
    read(data, offset)
}
/// Write a signed LTIME duration in nanoseconds.
pub fn set_ltime(data: &mut [u8], offset: usize, nanos: i64) -> Result<()> {
    write(data, offset, nanos)
}

/// Read an S5TIME BCD timer as a duration in milliseconds.
pub fn get_s5time(data: &[u8], offset: usize) -> Result<u32> {
    let raw = read::<u16>(data, offset)?;
    let base = raw >> 12;
    let hundreds = (raw >> 8) & 15;
    let digits = from_bcd(raw as u8)?;
    if base > 3 || hundreds > 9 {
        return Err(Error::InvalidInput("invalid S5TIME"));
    }
    Ok((u32::from(hundreds) * 100 + u32::from(digits)) * 10u32.pow(u32::from(base) + 1))
}

/// Read milliseconds since midnight, strictly less than 24 hours.
pub fn get_tod(data: &[u8], offset: usize) -> Result<u32> {
    let value = read::<u32>(data, offset)?;
    if value >= 86_400_000 {
        return Err(Error::InvalidInput("TOD exceeds one day"));
    }
    Ok(value)
}
/// Write milliseconds since midnight, rejecting 24 hours and above.
pub fn set_tod(data: &mut [u8], offset: usize, millis: u32) -> Result<()> {
    if millis >= 86_400_000 {
        return Err(Error::InvalidInput("TOD exceeds one day"));
    }
    write(data, offset, millis)
}
/// Read nanoseconds since midnight, strictly less than 24 hours.
pub fn get_ltod(data: &[u8], offset: usize) -> Result<u64> {
    let value = read::<u64>(data, offset)?;
    if value >= 86_400_000_000_000 {
        return Err(Error::InvalidInput("LTOD exceeds one day"));
    }
    Ok(value)
}
/// Write nanoseconds since midnight, rejecting 24 hours and above.
pub fn set_ltod(data: &mut [u8], offset: usize, nanos: u64) -> Result<()> {
    if nanos >= 86_400_000_000_000 {
        return Err(Error::InvalidInput("LTOD exceeds one day"));
    }
    write(data, offset, nanos)
}

/// Read DATE (days since 1990-01-01), bounded by 2168-12-31.
pub fn get_date(data: &[u8], offset: usize) -> Result<Date> {
    let value = Date::from_ordinal(726_467 + i64::from(read::<u16>(data, offset)?))?;
    if value.year > 2168 {
        return Err(Error::InvalidInput("DATE exceeds 2168"));
    }
    Ok(value)
}
/// Write DATE within the upstream 1990-01-01 through 2168-12-31 range.
pub fn set_date(data: &mut [u8], offset: usize, value: Date) -> Result<()> {
    if !(1990..=2168).contains(&value.year) {
        return Err(Error::InvalidInput("DATE outside supported range"));
    }
    write(data, offset, (value.ordinal() - 726_467) as u16)
}

/// Read eight-byte BCD DATE_AND_TIME (1990–2089) with strict field validation.
pub fn get_dt(data: &[u8], offset: usize) -> Result<DateTime> {
    let raw = &data[range(data.len(), offset, 8)?];
    let year = from_bcd(raw[0])?;
    let year = if year < 90 {
        2000 + u16::from(year)
    } else {
        1900 + u16::from(year)
    };
    let date = Date::new(year, from_bcd(raw[1])?, from_bcd(raw[2])?)?;
    if raw[7] & 15 == 0 || raw[7] & 15 > 7 || raw[7] >> 4 > 9 {
        return Err(Error::InvalidInput("invalid DT millisecond or weekday"));
    }
    let millis = u32::from(from_bcd(raw[6])?) * 10 + u32::from(raw[7] >> 4);
    DateTime::new(
        date,
        from_bcd(raw[3])?,
        from_bcd(raw[4])?,
        from_bcd(raw[5])?,
        millis * 1_000_000,
    )
}

/// Write BCD DATE_AND_TIME, truncating sub-millisecond precision as upstream does.
pub fn set_dt(data: &mut [u8], offset: usize, value: DateTime) -> Result<()> {
    if !(1990..=2089).contains(&value.date.year) {
        return Err(Error::InvalidInput("DT year outside 1990–2089"));
    }
    let millis = value.nanos / 1_000_000;
    let bytes = [
        bcd((value.date.year % 100) as u8),
        bcd(value.date.month),
        bcd(value.date.day),
        bcd(value.hour),
        bcd(value.minute),
        bcd(value.second),
        bcd((millis / 10) as u8),
        ((millis % 10) as u8) << 4 | value.date.weekday(),
    ];
    let target = range(data.len(), offset, 8)?;
    data[target].copy_from_slice(&bytes);
    Ok(())
}

/// Read DTL with full nanosecond precision, bounded by upstream's upper datetime.
pub fn get_dtl(data: &[u8], offset: usize) -> Result<DateTime> {
    let raw = &data[range(data.len(), offset, 12)?];
    if !(1..=7).contains(&raw[4]) {
        return Err(Error::InvalidInput("invalid DTL weekday"));
    }
    let date = Date::new(read(raw, 0)?, raw[2], raw[3])?;
    let value = DateTime::new(date, raw[5], raw[6], raw[7], read(raw, 8)?)?;
    validate_dtl(value)?;
    Ok(value)
}

/// Enforce upstream's exact final supported DTL instant.
fn validate_dtl(value: DateTime) -> Result<()> {
    let maximum = DateTime::new(Date::new(2554, 12, 31)?, 23, 59, 59, 0)?;
    if value > maximum {
        return Err(Error::InvalidInput("DTL exceeds upper limit"));
    }
    Ok(())
}

/// Write DTL, retaining nanoseconds and validating before mutating the destination.
pub fn set_dtl(data: &mut [u8], offset: usize, value: DateTime) -> Result<()> {
    validate_dtl(value)?;
    let target = range(data.len(), offset, 12)?;
    let raw = &mut data[target];
    write(raw, 0, value.date.year)?;
    raw[2..8].copy_from_slice(&[
        value.date.month,
        value.date.day,
        value.date.weekday(),
        value.hour,
        value.minute,
        value.second,
    ]);
    write(raw, 8, value.nanos)
}

/// Read unsigned nanoseconds since 1970-01-01 as a calendar value.
pub fn get_ldt(data: &[u8], offset: usize) -> Result<DateTime> {
    let nanos = read::<u64>(data, offset)?;
    let seconds = nanos / 1_000_000_000;
    let date = Date::from_ordinal(719_162 + (seconds / 86400) as i64)?;
    DateTime::new(
        date,
        (seconds / 3600 % 24) as u8,
        (seconds / 60 % 60) as u8,
        (seconds % 60) as u8,
        (nanos % 1_000_000_000) as u32,
    )
}

/// Write LDT with checked unsigned epoch arithmetic; dates before 1970 are invalid.
pub fn set_ldt(data: &mut [u8], offset: usize, value: DateTime) -> Result<()> {
    let days = i128::from(value.date.ordinal() - 719_162);
    let seconds = days * 86400
        + i128::from(value.hour) * 3600
        + i128::from(value.minute) * 60
        + i128::from(value.second);
    let nanos = u64::try_from(seconds * 1_000_000_000 + i128::from(value.nanos))
        .map_err(|_| Error::InvalidInput("LDT outside unsigned 64-bit epoch range"))?;
    write(data, offset, nanos)
}
