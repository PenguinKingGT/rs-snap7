//! Typed client conveniences over the same raw memory workflows.

use super::Client;
use crate::{
    Area, Error, Result,
    codec::{self, Scalar},
};

impl Client {
    /// Read one fixed-width scalar; numeric values use big-endian encoding.
    pub fn db_read_value<T: Scalar>(&self, db: u16, start: u32) -> Result<T> {
        if T::SIZE == 0 {
            return Err(Error::InvalidInput("zero-width scalar"));
        }
        codec::read(&self.db_read(db, start, T::SIZE)?, 0)
    }

    /// Encode a scalar before sending it; an encoding error performs no network I/O.
    pub fn db_write_value<T: Scalar>(&self, db: u16, start: u32, value: T) -> Result<()> {
        if T::SIZE == 0 || T::SIZE > 0x200000 {
            return Err(Error::InvalidInput("invalid scalar width"));
        }
        let mut data = vec![0; T::SIZE];
        codec::write(&mut data, 0, value)?;
        self.db_write(db, start, &data)
    }

    /// Read a contiguous scalar array, splitting wire transfers as necessary.
    pub fn db_read_array<T: Scalar>(&self, db: u16, start: u32, count: usize) -> Result<Vec<T>> {
        let size = array_size::<T>(count)?;
        let bytes = self.db_read(db, start, size)?;
        bytes.chunks_exact(T::SIZE).map(T::decode).collect()
    }

    /// Write a contiguous array after validating and encoding every value.
    pub fn db_write_array<T: Scalar + Copy>(
        &self,
        db: u16,
        start: u32,
        values: &[T],
    ) -> Result<()> {
        let size = array_size::<T>(values.len())?;
        let mut bytes = vec![0; size];
        for (target, value) in bytes.chunks_exact_mut(T::SIZE).zip(values) {
            value.encode(target)?;
        }
        self.db_write(db, start, &bytes)
    }

    /// Read one bit (0–7) from a data block byte.
    pub fn db_read_bool(&self, db: u16, start: u32, bit: u8) -> Result<bool> {
        if bit > 7 {
            return Err(Error::InvalidInput("bit must be 0 through 7"));
        }
        codec::get_bool(&self.db_read(db, start, 1)?, 0, bit)
    }

    /// Update one DB bit while preserving its neighbors under one session guard.
    /// This read/modify/write is serialized against clones, but is not atomic
    /// against external PLC programs or independent client connections.
    pub fn db_write_bool(&self, db: u16, start: u32, bit: u8, value: bool) -> Result<()> {
        if bit > 7 {
            return Err(Error::InvalidInput("bit must be 0 through 7"));
        }
        let mut session = self.session()?;
        let mut bytes = session.read_area(Area::Db, db, start, 1)?;
        codec::set_bool(&mut bytes, 0, bit, value)?;
        session.write_area(Area::Db, db, start, &bytes)
    }

    /// Read an S7 STRING in Latin-1; malformed headers are rejected before reading content.
    pub fn db_read_string(&self, db: u16, start: u32) -> Result<String> {
        let mut session = self.session()?;
        let mut data = session.read_area(Area::Db, db, start, 2)?;
        if data[0] > 254 || data[1] > data[0] {
            return Err(Error::Protocol("invalid PLC STRING header"));
        }
        let offset = start
            .checked_add(2)
            .ok_or(Error::InvalidInput("STRING address overflow"))?;
        data.extend(session.read_area(Area::Db, db, offset, usize::from(data[1]))?);
        codec::get_string(&data, 0)
    }

    /// Write a Latin-1 S7 STRING, clearing unused capacity like the scalar codec.
    pub fn db_write_string(&self, db: u16, start: u32, value: &str, capacity: u8) -> Result<()> {
        let mut bytes = vec![0; 2 + usize::from(capacity)];
        codec::set_string(&mut bytes, 0, value, capacity)?;
        self.db_write(db, start, &bytes)
    }

    /// Read a UTF-16BE S7 WSTRING after validating its capacity and current length.
    pub fn db_read_wstring(&self, db: u16, start: u32) -> Result<String> {
        let mut session = self.session()?;
        let mut data = session.read_area(Area::Db, db, start, 4)?;
        let capacity = codec::read::<u16>(&data, 0)?;
        let length = codec::read::<u16>(&data, 2)?;
        if capacity > 16382 || length > capacity {
            return Err(Error::Protocol("invalid PLC WSTRING header"));
        }
        let offset = start
            .checked_add(4)
            .ok_or(Error::InvalidInput("WSTRING address overflow"))?;
        data.extend(session.read_area(Area::Db, db, offset, usize::from(length) * 2)?);
        codec::get_wstring(&data, 0)
    }

    /// Write a BMP-only WSTRING header and actual content; unused capacity is untouched.
    pub fn db_write_wstring(&self, db: u16, start: u32, value: &str, capacity: u16) -> Result<()> {
        if capacity > 16382 || value.chars().any(|c| c as u32 > 0xffff) {
            return Err(Error::InvalidInput("invalid WSTRING capacity or character"));
        }
        let units = value.chars().count();
        if units > usize::from(capacity) {
            return Err(Error::InvalidInput("WSTRING exceeds capacity"));
        }
        let mut bytes = vec![0; 4 + units * 2];
        codec::set_wstring(&mut bytes, 0, value, capacity)?;
        self.db_write(db, start, &bytes)
    }
}

/// Bound generic widths before allocation or chunk iteration.
fn array_size<T: Scalar>(count: usize) -> Result<usize> {
    let size = count
        .checked_mul(T::SIZE)
        .ok_or(Error::InvalidInput("array size overflow"))?;
    if T::SIZE == 0 || size > 0x200000 {
        return Err(Error::InvalidInput("array exceeds address space"));
    }
    Ok(size)
}

impl Client {
    /// Read one S7 BYTE value from a DB byte offset.
    pub fn db_read_byte(&self, db: u16, start: u32) -> Result<u8> {
        self.db_read_value(db, start)
    }
    /// Write one S7 BYTE value to a DB byte offset.
    pub fn db_write_byte(&self, db: u16, start: u32, value: u8) -> Result<()> {
        self.db_write_value(db, start, value)
    }
}

impl Client {
    /// Read one S7 INT value from a DB byte offset.
    pub fn db_read_int(&self, db: u16, start: u32) -> Result<i16> {
        self.db_read_value(db, start)
    }
    /// Write one S7 INT value to a DB byte offset.
    pub fn db_write_int(&self, db: u16, start: u32, value: i16) -> Result<()> {
        self.db_write_value(db, start, value)
    }
}

impl Client {
    /// Read one S7 UINT value from a DB byte offset.
    pub fn db_read_uint(&self, db: u16, start: u32) -> Result<u16> {
        self.db_read_value(db, start)
    }
    /// Write one S7 UINT value to a DB byte offset.
    pub fn db_write_uint(&self, db: u16, start: u32, value: u16) -> Result<()> {
        self.db_write_value(db, start, value)
    }
}

impl Client {
    /// Read one S7 WORD value from a DB byte offset.
    pub fn db_read_word(&self, db: u16, start: u32) -> Result<u16> {
        self.db_read_value(db, start)
    }
    /// Write one S7 WORD value to a DB byte offset.
    pub fn db_write_word(&self, db: u16, start: u32, value: u16) -> Result<()> {
        self.db_write_value(db, start, value)
    }
}

impl Client {
    /// Read one S7 DINT value from a DB byte offset.
    pub fn db_read_dint(&self, db: u16, start: u32) -> Result<i32> {
        self.db_read_value(db, start)
    }
    /// Write one S7 DINT value to a DB byte offset.
    pub fn db_write_dint(&self, db: u16, start: u32, value: i32) -> Result<()> {
        self.db_write_value(db, start, value)
    }
}

impl Client {
    /// Read one S7 UDINT value from a DB byte offset.
    pub fn db_read_udint(&self, db: u16, start: u32) -> Result<u32> {
        self.db_read_value(db, start)
    }
    /// Write one S7 UDINT value to a DB byte offset.
    pub fn db_write_udint(&self, db: u16, start: u32, value: u32) -> Result<()> {
        self.db_write_value(db, start, value)
    }
}

impl Client {
    /// Read one S7 DWORD value from a DB byte offset.
    pub fn db_read_dword(&self, db: u16, start: u32) -> Result<u32> {
        self.db_read_value(db, start)
    }
    /// Write one S7 DWORD value to a DB byte offset.
    pub fn db_write_dword(&self, db: u16, start: u32, value: u32) -> Result<()> {
        self.db_write_value(db, start, value)
    }
}

impl Client {
    /// Read one S7 REAL value from a DB byte offset.
    pub fn db_read_real(&self, db: u16, start: u32) -> Result<f32> {
        self.db_read_value(db, start)
    }
    /// Write one S7 REAL value to a DB byte offset.
    pub fn db_write_real(&self, db: u16, start: u32, value: f32) -> Result<()> {
        self.db_write_value(db, start, value)
    }
}

impl Client {
    /// Read one S7 LREAL value from a DB byte offset.
    pub fn db_read_lreal(&self, db: u16, start: u32) -> Result<f64> {
        self.db_read_value(db, start)
    }
    /// Write one S7 LREAL value to a DB byte offset.
    pub fn db_write_lreal(&self, db: u16, start: u32, value: f64) -> Result<()> {
        self.db_write_value(db, start, value)
    }
}
