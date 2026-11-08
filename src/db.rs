//! Explicit DB layouts, owned table snapshots, and selective row/field I/O.
//!
//! Layout offsets are byte/bit strings, never floating-point numbers. Tables own
//! both editable data and an acknowledged baseline; failed writes remain dirty.

use crate::{
    Area, Client, Error, Result,
    tags::{DataType, Tag, Value},
};
use std::{collections::BTreeMap, ops::Range};

/// A field layout whose tags use row-relative byte offsets.
#[derive(Debug, Clone)]
pub struct Layout {
    fields: BTreeMap<String, Tag>,
    width: usize,
}
impl Layout {
    /// Parse lines "byte[.bit] name TYPE"; '#' starts a comment.
    /// layout_offset is subtracted from source offsets. Reject overlaps except
    /// distinct BOOL bits in the same byte.
    pub fn parse(specification: &str, layout_offset: u32) -> Result<Self> {
        let mut fields = BTreeMap::new();
        let mut occupied = BTreeMap::<usize, u8>::new();
        let mut width = 0;
        for line in specification.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<_> = line.split_whitespace().collect();
            if parts.len() != 3 {
                return Err(Error::InvalidInput("layout line needs offset name type"));
            }
            let (offset, bit) = crate::tags::parse_offset(parts[0])?;
            let offset = offset
                .checked_sub(layout_offset)
                .ok_or(Error::InvalidInput("field precedes layout offset"))?;
            let normalized = match parts[2].to_ascii_uppercase().as_str() {
                "TIME_OF_DAY" => "TOD".to_owned(),
                "DATE_AND_TIME" => "DT".to_owned(),
                _ => parts[2].to_ascii_uppercase(),
            };
            let (datatype, count) = crate::tags::type_count(&normalized)?;
            let tag = Tag::new(Area::Db, 0, offset, bit, datatype, count)?.named(parts[1]);
            let mask = if datatype == DataType::Bool {
                1 << bit
            } else {
                0xff
            };
            for index in offset as usize..offset as usize + tag.size() {
                let old = occupied.entry(index).or_default();
                if *old & mask != 0 {
                    return Err(Error::InvalidInput("overlapping layout fields"));
                }
                *old |= mask;
            }
            width = width.max(offset as usize + tag.size());
            if fields.insert(parts[1].to_owned(), tag).is_some() {
                return Err(Error::InvalidInput("duplicate layout field"));
            }
        }
        if fields.is_empty() {
            return Err(Error::InvalidInput("empty layout"));
        }
        Ok(Self { fields, width })
    }
    /// Minimum row width covering every field.
    pub fn width(&self) -> usize {
        self.width
    }
    /// Fields sorted by name; address offsets are row-relative.
    pub fn fields(&self) -> &BTreeMap<String, Tag> {
        &self.fields
    }
}

/// Physical row placement and its writable suffix.
#[derive(Debug, Clone)]
pub struct DbConfig {
    /// Memory area; DB, inputs, outputs or markers.
    pub area: Area,
    /// DB number, zero for non-DB areas.
    pub db_number: u16,
    /// Absolute byte offset of row zero.
    pub start: u32,
    /// Row storage width, excluding the gap.
    pub row_size: usize,
    /// Bytes between consecutive rows, preserved by row writes.
    pub row_gap: usize,
    /// Number of rows.
    pub rows: usize,
    /// Read-only prefix within each row.
    pub writable_start: usize,
}

/// An editable table snapshot. Methods require exclusive access for local changes.
#[derive(Debug, Clone)]
pub struct Db {
    config: DbConfig,
    layout: Layout,
    data: Vec<u8>,
    baseline: Vec<u8>,
}
impl Db {
    /// Validate layout placement and exact buffer width before accepting a snapshot.
    /// The buffer starts at config.start and includes inter-row gaps.
    pub fn new(config: DbConfig, layout: Layout, data: Vec<u8>) -> Result<Self> {
        let stride = config
            .row_size
            .checked_add(config.row_gap)
            .ok_or(Error::InvalidInput("DB stride overflow"))?;
        let size = stride
            .checked_mul(config.rows)
            .and_then(|n| n.checked_sub(if config.rows > 0 { config.row_gap } else { 0 }))
            .ok_or(Error::InvalidInput("DB size overflow"))?;
        if config.rows == 0
            || layout.width > config.row_size
            || config.writable_start > config.row_size
            || data.len() != size
        {
            return Err(Error::InvalidInput("invalid DB row size or buffer"));
        }
        Tag::new(
            config.area,
            config.db_number,
            config.start,
            0,
            DataType::Byte,
            size,
        )?;
        Ok(Self {
            config,
            layout,
            baseline: data.clone(),
            data,
        })
    }
    /// Validated physical placement.
    pub fn config(&self) -> &DbConfig {
        &self.config
    }
    /// Field layout shared by all rows.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }
    /// Editable snapshot bytes, exposed read-only to preserve validation.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    /// Replace the entire snapshot and baseline; reject width changes.
    pub fn replace(&mut self, data: Vec<u8>) -> Result<()> {
        if data.len() != self.data.len() {
            return Err(Error::InvalidInput("replacement DB width mismatch"));
        }
        self.baseline = data.clone();
        self.data = data;
        Ok(())
    }
    /// Number of physical rows.
    pub fn len(&self) -> usize {
        self.config.rows
    }
    /// Whether the table has no rows (validated tables are nonempty).
    pub fn is_empty(&self) -> bool {
        self.config.rows == 0
    }
    /// Locate a row without unchecked multiplication.
    fn row_range(&self, row: usize) -> Result<Range<usize>> {
        if row >= self.len() {
            return Err(Error::InvalidInput("DB row index out of range"));
        }
        let start = row * (self.config.row_size + self.config.row_gap);
        Ok(start..start + self.config.row_size)
    }
    /// Locate a named field within a row.
    fn field(&self, row: usize, name: &str) -> Result<(Tag, Range<usize>)> {
        let row = self.row_range(row)?;
        let field = self
            .layout
            .fields
            .get(name)
            .ok_or(Error::InvalidInput("unknown DB field"))?;
        let start = row.start + field.offset() as usize;
        Ok((field.clone(), start..start + field.size()))
    }
    /// Decode a field from the local snapshot.
    pub fn get(&self, row: usize, name: &str) -> Result<Value> {
        let (field, range) = self.field(row, name)?;
        field.decode(&self.data[range])
    }
    /// Validate a complete value, then update locally while preserving BOOL neighbors.
    pub fn set(&mut self, row: usize, name: &str, value: &Value) -> Result<()> {
        let (field, range) = self.field(row, name)?;
        let mut bytes = field.encode(value)?;
        if field.datatype() == DataType::Bool {
            field.merge_bits(&mut bytes, &self.data[range.clone()]);
        }
        self.data[range].copy_from_slice(&bytes);
        Ok(())
    }
    /// Export one row into a deterministic name/value snapshot.
    pub fn export_row(&self, row: usize) -> Result<BTreeMap<String, Value>> {
        self.layout
            .fields
            .keys()
            .map(|name| Ok((name.clone(), self.get(row, name)?)))
            .collect()
    }
    /// Export every physical row in order.
    pub fn export(&self) -> Result<Vec<BTreeMap<String, Value>>> {
        (0..self.len()).map(|row| self.export_row(row)).collect()
    }
    /// Find the first matching field value, resolving identifiers from current data.
    pub fn find_row(&self, field: &str, value: &Value) -> Result<Option<usize>> {
        for row in 0..self.len() {
            if self.get(row, field)? == *value {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }
    /// Whether all bytes in the row match the last acknowledged snapshot.
    pub fn unchanged(&self, row: usize) -> Result<bool> {
        let range = self.row_range(row)?;
        Ok(self.data[range.clone()] == self.baseline[range])
    }
    /// Coalesce changed writable bytes; gaps and the read-only prefix are excluded.
    pub fn changed_ranges(&self, row: usize) -> Result<Vec<Range<usize>>> {
        let mut range = self.row_range(row)?;
        range.start += self.config.writable_start;
        let mut ranges = Vec::new();
        let mut cursor = range.start;
        while cursor < range.end {
            if self.data[cursor] == self.baseline[cursor] {
                cursor += 1;
                continue;
            }
            let start = cursor;
            while cursor < range.end && self.data[cursor] != self.baseline[cursor] {
                cursor += 1;
            }
            ranges.push(start..cursor);
        }
        Ok(ranges)
    }
    /// Translate local range positions to the absolute PLC byte address.
    fn address(&self, local: usize) -> u32 {
        self.config.start + local as u32
    }
    /// Map failures across multiple writes while retaining cumulative progress.
    fn write_error(error: Error, confirmed: usize) -> Error {
        match error {
            Error::AmbiguousWrite {
                confirmed: n,
                uncertain,
                source,
            } => Error::AmbiguousWrite {
                confirmed: confirmed + n,
                uncertain,
                source,
            },
            Error::PartialWrite {
                confirmed: n,
                source,
            } => Error::PartialWrite {
                confirmed: confirmed + n,
                source,
            },
            error if confirmed > 0 => Error::PartialWrite {
                confirmed,
                source: Box::new(error),
            },
            error => error,
        }
    }

    /// Refresh an entire row. Pending local changes in that row are discarded.
    pub fn read_row(&mut self, client: &Client, row: usize) -> Result<()> {
        let range = self.row_range(row)?;
        let data = client.read_area(
            self.config.area,
            self.config.db_number,
            self.address(range.start),
            range.len(),
        )?;
        self.data[range.clone()].copy_from_slice(&data);
        self.baseline[range].copy_from_slice(&data);
        Ok(())
    }
    /// Write the writable row suffix, or only changed byte runs.
    /// Only acknowledged ranges update the baseline; gaps are never written.
    pub fn write_row(&mut self, client: &Client, row: usize, changed_only: bool) -> Result<()> {
        let ranges = if changed_only {
            self.changed_ranges(row)?
        } else {
            let mut range = self.row_range(row)?;
            range.start += self.config.writable_start;
            vec![range]
        };
        let mut confirmed = 0;
        for range in ranges {
            client
                .write_area(
                    self.config.area,
                    self.config.db_number,
                    self.address(range.start),
                    &self.data[range.clone()],
                )
                .map_err(|error| Self::write_error(error, confirmed))?;
            confirmed += range.len();
            self.baseline[range.clone()].copy_from_slice(&self.data[range]);
        }
        Ok(())
    }
    /// Refresh selected fields. A BOOL read refreshes its entire storage byte.
    pub fn read_fields(&mut self, client: &Client, row: usize, names: &[&str]) -> Result<()> {
        let fields = names
            .iter()
            .map(|name| self.field(row, name))
            .collect::<Result<Vec<_>>>()?;
        for (field, range) in fields {
            let bytes = client.read_area(
                self.config.area,
                self.config.db_number,
                self.address(range.start),
                range.len(),
            )?;
            field.decode(&bytes)?;
            self.data[range.clone()].copy_from_slice(&bytes);
            self.baseline[range].copy_from_slice(&bytes);
        }
        Ok(())
    }
    /// Write selected fields through checked tags, preserving remote BOOL neighbors.
    /// Read-only fields are rejected before any request; acknowledged fields become clean.
    pub fn write_fields(&mut self, client: &Client, row: usize, names: &[&str]) -> Result<()> {
        let fields = self.write_fields_plan(row, names)?;
        let mut confirmed = 0;
        for (tag, range, value) in fields {
            client
                .write_tag(&tag, &value)
                .map_err(|error| Self::write_error(error, confirmed))?;
            confirmed += range.len();
            self.acknowledge_field(&tag, range);
        }
        Ok(())
    }
    /// Validate every requested write before touching the network.
    fn write_fields_plan(
        &self,
        row: usize,
        names: &[&str],
    ) -> Result<Vec<(Tag, Range<usize>, Value)>> {
        let row_start = self.row_range(row)?.start;
        names
            .iter()
            .map(|name| {
                let (field, range) = self.field(row, name)?;
                if range.start < row_start + self.config.writable_start {
                    return Err(Error::InvalidInput("field is read-only"));
                }
                let tag = Tag::new(
                    self.config.area,
                    self.config.db_number,
                    self.address(range.start),
                    field.bit(),
                    field.datatype(),
                    field.count(),
                )?;
                let value = field.decode(&self.data[range.clone()])?;
                tag.encode(&value)?;
                Ok((tag, range, value))
            })
            .collect()
    }
    /// A BOOL field acknowledges only its bit; other local bits can remain dirty.
    fn acknowledge_field(&mut self, tag: &Tag, range: Range<usize>) {
        if tag.datatype() == DataType::Bool {
            let mask = 1 << tag.bit();
            for index in range {
                self.baseline[index] = (self.baseline[index] & !mask) | (self.data[index] & mask);
            }
        } else {
            self.baseline[range.clone()].copy_from_slice(&self.data[range]);
        }
    }
}

#[cfg(feature = "async")]
impl Db {
    /// Refresh an entire row. Pending local changes in that row are discarded.
    pub async fn read_row_async(&mut self, client: &crate::AsyncClient, row: usize) -> Result<()> {
        let range = self.row_range(row)?;
        let data = client
            .read_area(
                self.config.area,
                self.config.db_number,
                self.address(range.start),
                range.len(),
            )
            .await?;
        self.data[range.clone()].copy_from_slice(&data);
        self.baseline[range].copy_from_slice(&data);
        Ok(())
    }
    /// Write the writable row suffix, or only changed byte runs.
    /// Only acknowledged ranges update the baseline; gaps are never written.
    pub async fn write_row_async(
        &mut self,
        client: &crate::AsyncClient,
        row: usize,
        changed_only: bool,
    ) -> Result<()> {
        let ranges = if changed_only {
            self.changed_ranges(row)?
        } else {
            let mut range = self.row_range(row)?;
            range.start += self.config.writable_start;
            vec![range]
        };
        let mut confirmed = 0;
        for range in ranges {
            client
                .write_area(
                    self.config.area,
                    self.config.db_number,
                    self.address(range.start),
                    &self.data[range.clone()],
                )
                .await
                .map_err(|error| Self::write_error(error, confirmed))?;
            confirmed += range.len();
            self.baseline[range.clone()].copy_from_slice(&self.data[range]);
        }
        Ok(())
    }
    /// Refresh selected fields. A BOOL read refreshes its entire storage byte.
    pub async fn read_fields_async(
        &mut self,
        client: &crate::AsyncClient,
        row: usize,
        names: &[&str],
    ) -> Result<()> {
        let fields = names
            .iter()
            .map(|name| self.field(row, name))
            .collect::<Result<Vec<_>>>()?;
        for (field, range) in fields {
            let bytes = client
                .read_area(
                    self.config.area,
                    self.config.db_number,
                    self.address(range.start),
                    range.len(),
                )
                .await?;
            field.decode(&bytes)?;
            self.data[range.clone()].copy_from_slice(&bytes);
            self.baseline[range].copy_from_slice(&bytes);
        }
        Ok(())
    }
    /// Write selected fields through checked tags, preserving remote BOOL neighbors.
    /// Read-only fields are rejected before any request; acknowledged fields become clean.
    pub async fn write_fields_async(
        &mut self,
        client: &crate::AsyncClient,
        row: usize,
        names: &[&str],
    ) -> Result<()> {
        let fields = self.write_fields_plan(row, names)?;
        let mut confirmed = 0;
        for (tag, range, value) in fields {
            client
                .write_tag(&tag, &value)
                .await
                .map_err(|error| Self::write_error(error, confirmed))?;
            confirmed += range.len();
            self.acknowledge_field(&tag, range);
        }
        Ok(())
    }
}
