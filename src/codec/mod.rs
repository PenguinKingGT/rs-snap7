//! Checked, big-endian S7 value codecs with explicit string encodings.

pub mod datetime;

use crate::{Error, Result};
use std::ops::Range;

/// Validate a buffer range without overflowing offset arithmetic.
pub(crate) fn range(len: usize, offset: usize, size: usize) -> Result<Range<usize>> {
    let end = offset
        .checked_add(size)
        .ok_or(Error::InvalidInput("buffer offset overflow"))?;
    if end > len {
        return Err(Error::InvalidInput("buffer too short"));
    }
    Ok(offset..end)
}

/// A fixed-width S7 scalar encoded in network byte order.
///
/// Implementations must reject invalid representations and short slices.
/// Read/write helpers validate the complete range before calling the codec.
pub trait Scalar: Sized {
    /// Encoded byte width.
    const SIZE: usize;
    /// Decode from a slice of exactly `SIZE` bytes.
    fn decode(bytes: &[u8]) -> Result<Self>;
    /// Encode into a slice of exactly `SIZE` bytes.
    fn encode(self, bytes: &mut [u8]) -> Result<()>;
}

/// Read a scalar at a byte offset, rejecting incomplete or overflowing ranges.
///
/// ```
/// let value = rs_snap7::codec::read::<i16>(&[0xff, 0xfe], 0)?;
/// assert_eq!(value, -2);
/// # Ok::<(), rs_snap7::Error>(())
/// ```
pub fn read<T: Scalar>(data: &[u8], offset: usize) -> Result<T> {
    T::decode(&data[range(data.len(), offset, T::SIZE)?])
}

/// Write a scalar without modifying bytes outside its validated range.
pub fn write<T: Scalar>(data: &mut [u8], offset: usize, value: T) -> Result<()> {
    let target = range(data.len(), offset, T::SIZE)?;
    value.encode(&mut data[target])
}

/// Implement the same checked network-byte-order contract for numeric scalars.
macro_rules! scalar {
    ($($ty:ty),+ $(,)?) => {$ (
        impl Scalar for $ty {
            const SIZE: usize = std::mem::size_of::<Self>();

            /// Decode an exactly sized big-endian scalar.
            fn decode(bytes: &[u8]) -> Result<Self> {
                let raw = bytes.try_into().map_err(|_| Error::InvalidInput("wrong scalar width"))?;
                Ok(Self::from_be_bytes(raw))
            }

            /// Encode into an exactly sized slice.
            fn encode(self, bytes: &mut [u8]) -> Result<()> {
                if bytes.len() != Self::SIZE {
                    return Err(Error::InvalidInput("wrong scalar width"));
                }
                bytes.copy_from_slice(&self.to_be_bytes());
                Ok(())
            }
        }
    )+};
}
scalar!(u8, i8, u16, i16, u32, i32, u64, i64, f32, f64);

/// Decode a bit, numbered from the least significant bit (0 through 7).
pub fn get_bool(data: &[u8], offset: usize, bit: u8) -> Result<bool> {
    if bit > 7 {
        return Err(Error::InvalidInput("bit must be 0 through 7"));
    }
    Ok(read::<u8>(data, offset)? & (1 << bit) != 0)
}

/// Update one bit while preserving all other bits. Reject invalid bit indexes.
pub fn set_bool(data: &mut [u8], offset: usize, bit: u8, value: bool) -> Result<()> {
    if bit > 7 {
        return Err(Error::InvalidInput("bit must be 0 through 7"));
    }
    let previous = read::<u8>(data, offset)?;
    let mask = 1 << bit;
    write(
        data,
        offset,
        if value {
            previous | mask
        } else {
            previous & !mask
        },
    )
}

/// Character encoding for byte strings; WSTRING always uses UTF-16BE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    /// One byte per code point, compatible with upstream's default.
    Latin1,
    /// UTF-8 bytes; lengths count encoded bytes rather than Unicode characters.
    Utf8,
}

impl TextEncoding {
    /// Encode text, rejecting values that cannot fit in Latin-1.
    fn encode(self, text: &str) -> Result<Vec<u8>> {
        match self {
            Self::Latin1 => text
                .chars()
                .map(|c| u8::try_from(c as u32).map_err(|_| Error::InvalidInput("not Latin-1")))
                .collect(),
            Self::Utf8 => Ok(text.as_bytes().to_vec()),
        }
    }

    /// Decode a complete byte string with strict UTF-8 validation when selected.
    fn decode(self, bytes: &[u8]) -> Result<String> {
        match self {
            Self::Latin1 => Ok(bytes.iter().map(|b| char::from(*b)).collect()),
            Self::Utf8 => {
                String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidInput("invalid UTF-8"))
            }
        }
    }
}

/// Read a Latin-1 CHAR.
pub fn get_char(data: &[u8], offset: usize) -> Result<char> {
    Ok(char::from(read::<u8>(data, offset)?))
}

/// Write a CHAR, rejecting code points above 255.
pub fn set_char(data: &mut [u8], offset: usize, value: char) -> Result<()> {
    let value =
        u8::try_from(value as u32).map_err(|_| Error::InvalidInput("CHAR exceeds Latin-1"))?;
    write(data, offset, value)
}

/// Read a single BMP WCHAR; isolated UTF-16 surrogates are invalid.
pub fn get_wchar(data: &[u8], offset: usize) -> Result<char> {
    char::from_u32(u32::from(read::<u16>(data, offset)?))
        .ok_or(Error::InvalidInput("invalid WCHAR"))
}

/// Write one BMP WCHAR, rejecting characters that require a surrogate pair.
pub fn set_wchar(data: &mut [u8], offset: usize, value: char) -> Result<()> {
    let value =
        u16::try_from(value as u32).map_err(|_| Error::InvalidInput("WCHAR exceeds BMP"))?;
    write(data, offset, value)
}

/// Read an S7 STRING using upstream's default Latin-1 encoding.
pub fn get_string(data: &[u8], offset: usize) -> Result<String> {
    get_string_with_encoding(data, offset, TextEncoding::Latin1)
}

/// Read an S7 STRING with a two-byte capacity/length header (maximum 254 bytes).
/// Only the header and current content must be present; unused capacity is not read.
pub fn get_string_with_encoding(
    data: &[u8],
    offset: usize,
    encoding: TextEncoding,
) -> Result<String> {
    let header = &data[range(data.len(), offset, 2)?];
    let capacity = usize::from(header[0]);
    let size = usize::from(header[1]);
    if capacity > 254 || size > capacity {
        return Err(Error::InvalidInput("invalid STRING header"));
    }
    let value = &data[range(data.len(), offset + 2, size)?];
    encoding.decode(value)
}

/// Write a Latin-1 S7 STRING, zero-filling unused capacity.
pub fn set_string(data: &mut [u8], offset: usize, value: &str, capacity: u8) -> Result<()> {
    set_string_with_encoding(data, offset, value, capacity, TextEncoding::Latin1)
}

/// Write an encoded S7 STRING. Validate everything before modifying the buffer.
pub fn set_string_with_encoding(
    data: &mut [u8],
    offset: usize,
    value: &str,
    capacity: u8,
    encoding: TextEncoding,
) -> Result<()> {
    let encoded = encoding.encode(value)?;
    if capacity > 254 || encoded.len() > usize::from(capacity) {
        return Err(Error::InvalidInput("STRING exceeds capacity"));
    }
    let target = range(data.len(), offset, 2 + usize::from(capacity))?;
    let target = &mut data[target];
    target.fill(0);
    target[0] = capacity;
    target[1] = encoded.len() as u8;
    target[2..2 + encoded.len()].copy_from_slice(&encoded);
    Ok(())
}

/// Read a fixed-width Latin-1 string, optionally removing trailing spaces.
pub fn get_fstring(data: &[u8], offset: usize, size: usize, trim: bool) -> Result<String> {
    let value = TextEncoding::Latin1.decode(&data[range(data.len(), offset, size)?])?;
    Ok(if trim {
        value.trim_end_matches(' ').to_owned()
    } else {
        value
    })
}

/// Write a fixed-width Latin-1 string, padding the remaining range with spaces.
pub fn set_fstring(data: &mut [u8], offset: usize, value: &str, size: usize) -> Result<()> {
    let encoded = TextEncoding::Latin1.encode(value)?;
    if encoded.len() > size {
        return Err(Error::InvalidInput("fixed string exceeds capacity"));
    }
    let target = range(data.len(), offset, size)?;
    data[target.clone()].fill(b' ');
    data[target.start..target.start + encoded.len()].copy_from_slice(&encoded);
    Ok(())
}

/// Read an S7 WSTRING with two big-endian u16 capacity/length fields.
pub fn get_wstring(data: &[u8], offset: usize) -> Result<String> {
    let header = &data[range(data.len(), offset, 4)?];
    let capacity = read::<u16>(header, 0)?;
    let size = read::<u16>(header, 2)?;
    if capacity > 16382 || size > capacity {
        return Err(Error::InvalidInput("invalid WSTRING header"));
    }
    let raw = &data[range(data.len(), offset + 4, usize::from(size) * 2)?];
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| Error::InvalidInput("invalid UTF-16"))
}

/// Write a BMP-only S7 WSTRING, matching upstream's character restriction.
/// Requires space for the header and actual content; unused capacity is untouched.
pub fn set_wstring(data: &mut [u8], offset: usize, value: &str, capacity: u16) -> Result<()> {
    if capacity > 16382 || value.chars().any(|c| c as u32 > 0xffff) {
        return Err(Error::InvalidInput(
            "WSTRING capacity or character outside limits",
        ));
    }
    let units: Vec<u16> = value.encode_utf16().collect();
    if units.len() > usize::from(capacity) {
        return Err(Error::InvalidInput("WSTRING exceeds capacity"));
    }
    let target = range(data.len(), offset, 4 + units.len() * 2)?;
    let target = &mut data[target];
    write(target, 0, capacity)?;
    write(target, 2, units.len() as u16)?;
    for (chunk, value) in target[4..].chunks_exact_mut(2).zip(units) {
        chunk.copy_from_slice(&value.to_be_bytes());
    }
    Ok(())
}

/// Read a big-endian S7 BYTE, rejecting incomplete ranges.
pub fn get_byte(data: &[u8], offset: usize) -> Result<u8> {
    read(data, offset)
}

/// Write a big-endian S7 BYTE, preserving surrounding bytes.
pub fn set_byte(data: &mut [u8], offset: usize, value: u8) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 USINT, rejecting incomplete ranges.
pub fn get_usint(data: &[u8], offset: usize) -> Result<u8> {
    read(data, offset)
}

/// Write a big-endian S7 USINT, preserving surrounding bytes.
pub fn set_usint(data: &mut [u8], offset: usize, value: u8) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 SINT, rejecting incomplete ranges.
pub fn get_sint(data: &[u8], offset: usize) -> Result<i8> {
    read(data, offset)
}

/// Write a big-endian S7 SINT, preserving surrounding bytes.
pub fn set_sint(data: &mut [u8], offset: usize, value: i8) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 WORD, rejecting incomplete ranges.
pub fn get_word(data: &[u8], offset: usize) -> Result<u16> {
    read(data, offset)
}

/// Write a big-endian S7 WORD, preserving surrounding bytes.
pub fn set_word(data: &mut [u8], offset: usize, value: u16) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 UINT, rejecting incomplete ranges.
pub fn get_uint(data: &[u8], offset: usize) -> Result<u16> {
    read(data, offset)
}

/// Write a big-endian S7 UINT, preserving surrounding bytes.
pub fn set_uint(data: &mut [u8], offset: usize, value: u16) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 INT, rejecting incomplete ranges.
pub fn get_int(data: &[u8], offset: usize) -> Result<i16> {
    read(data, offset)
}

/// Write a big-endian S7 INT, preserving surrounding bytes.
pub fn set_int(data: &mut [u8], offset: usize, value: i16) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 DWORD, rejecting incomplete ranges.
pub fn get_dword(data: &[u8], offset: usize) -> Result<u32> {
    read(data, offset)
}

/// Write a big-endian S7 DWORD, preserving surrounding bytes.
pub fn set_dword(data: &mut [u8], offset: usize, value: u32) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 UDINT, rejecting incomplete ranges.
pub fn get_udint(data: &[u8], offset: usize) -> Result<u32> {
    read(data, offset)
}

/// Write a big-endian S7 UDINT, preserving surrounding bytes.
pub fn set_udint(data: &mut [u8], offset: usize, value: u32) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 DINT, rejecting incomplete ranges.
pub fn get_dint(data: &[u8], offset: usize) -> Result<i32> {
    read(data, offset)
}

/// Write a big-endian S7 DINT, preserving surrounding bytes.
pub fn set_dint(data: &mut [u8], offset: usize, value: i32) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 LWORD, rejecting incomplete ranges.
pub fn get_lword(data: &[u8], offset: usize) -> Result<u64> {
    read(data, offset)
}

/// Write a big-endian S7 LWORD, preserving surrounding bytes.
pub fn set_lword(data: &mut [u8], offset: usize, value: u64) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 ULINT, rejecting incomplete ranges.
pub fn get_ulint(data: &[u8], offset: usize) -> Result<u64> {
    read(data, offset)
}

/// Write a big-endian S7 ULINT, preserving surrounding bytes.
pub fn set_ulint(data: &mut [u8], offset: usize, value: u64) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 LINT, rejecting incomplete ranges.
pub fn get_lint(data: &[u8], offset: usize) -> Result<i64> {
    read(data, offset)
}

/// Write a big-endian S7 LINT, preserving surrounding bytes.
pub fn set_lint(data: &mut [u8], offset: usize, value: i64) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 REAL, rejecting incomplete ranges.
pub fn get_real(data: &[u8], offset: usize) -> Result<f32> {
    read(data, offset)
}

/// Write a big-endian S7 REAL, preserving surrounding bytes.
pub fn set_real(data: &mut [u8], offset: usize, value: f32) -> Result<()> {
    write(data, offset, value)
}

/// Read a big-endian S7 LREAL, rejecting incomplete ranges.
pub fn get_lreal(data: &[u8], offset: usize) -> Result<f64> {
    read(data, offset)
}

/// Write a big-endian S7 LREAL, preserving surrounding bytes.
pub fn set_lreal(data: &mut [u8], offset: usize, value: f64) -> Result<()> {
    write(data, offset, value)
}
