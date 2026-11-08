//! Bounded metadata import from text. File I/O belongs to the caller.
//!
//! Duplicate names, unknown types, and missing absolute XML offsets are errors;
//! these must not silently replace a tag or target byte zero.

use super::{Tag, parse::offset, type_count};
use crate::{Area, Error, Result};
use std::collections::BTreeMap;

/// Named tags in deterministic name order.
pub type TagMap = BTreeMap<String, Tag>;

/// Bound allocations for externally supplied documents.
fn bounded(text: &str) -> Result<()> {
    if text.len() > 8 * 1024 * 1024 {
        return Err(Error::InvalidInput("tag document exceeds 8 MiB"));
    }
    Ok(())
}
/// Insert without silently losing duplicate tags.
fn insert(tags: &mut TagMap, tag: Tag) -> Result<()> {
    if tag.name().is_empty() || tags.len() >= 100_000 || tags.contains_key(tag.name()) {
        return Err(Error::InvalidInput(
            "empty/duplicate tag name or too many tags",
        ));
    }
    tags.insert(tag.name().to_owned(), tag);
    Ok(())
}
/// Construct a DB tag, retaining decimal bit suffixes exactly.
fn make(name: &str, db: u16, position: &str, dtype: &str, bit: Option<u8>) -> Result<Tag> {
    let (byte, parsed_bit) = offset(position)?;
    if bit.is_some_and(|bit| parsed_bit != 0 && parsed_bit != bit) {
        return Err(Error::InvalidInput("conflicting tag bit suffix"));
    }
    let (datatype, count) = type_count(&dtype.trim().to_ascii_uppercase())?;
    Ok(Tag::new(
        Area::Db,
        db,
        byte,
        bit.unwrap_or(parsed_bit),
        datatype,
        count,
    )?
    .named(name))
}

/// Read CSV columns tag, db, offset, type and optional bit, including quoted fields.
pub fn load_csv(text: &str) -> Result<TagMap> {
    bounded(text)?;
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|_| Error::InvalidInput("invalid CSV header"))?
        .clone();
    let column = |name| {
        headers
            .iter()
            .position(|s| s == name)
            .ok_or(Error::InvalidInput("missing CSV column"))
    };
    let (name, db, position, dtype) = (
        column("tag")?,
        column("db")?,
        column("offset")?,
        column("type")?,
    );
    let bit = headers.iter().position(|s| s == "bit");
    let mut tags = TagMap::new();
    for row in reader.records() {
        let row = row.map_err(|_| Error::InvalidInput("invalid CSV record"))?;
        let field = |index| {
            row.get(index)
                .ok_or(Error::InvalidInput("short CSV record"))
        };
        let bit = bit
            .and_then(|index| row.get(index))
            .filter(|s| !s.is_empty())
            .map(super::parse::number)
            .transpose()?;
        insert(
            &mut tags,
            make(
                field(name)?,
                super::parse::number(field(db)?)?,
                field(position)?,
                field(dtype)?,
                bit,
            )?,
        )?;
    }
    Ok(tags)
}

/// Read {"name":{"db":1,"offset":"2.3","type":"BOOL","bit":3}}.
/// Numeric integer offsets are accepted; fractional offsets must be strings.
pub fn load_json(text: &str) -> Result<TagMap> {
    bounded(text)?;
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| Error::InvalidInput("invalid tag JSON"))?;
    let object = value
        .as_object()
        .ok_or(Error::InvalidInput("tag JSON must be an object"))?;
    let mut tags = TagMap::new();
    for (name, value) in object {
        let db = value
            .get("db")
            .and_then(|v| v.as_u64())
            .and_then(|v| u16::try_from(v).ok())
            .ok_or(Error::InvalidInput("invalid JSON DB"))?;
        let position = match value.get("offset") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Number(n)) if n.is_u64() => n.to_string(),
            _ => return Err(Error::InvalidInput("invalid JSON offset")),
        };
        let dtype = value
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or(Error::InvalidInput("missing JSON type"))?;
        let bit = value
            .get("bit")
            .map(|v| {
                v.as_u64()
                    .and_then(|v| u8::try_from(v).ok())
                    .ok_or(Error::InvalidInput("invalid JSON bit"))
            })
            .transpose()?;
        insert(&mut tags, make(name, db, &position, dtype, bit)?)?;
    }
    Ok(tags)
}

/// Import TIA Member metadata with explicit absolute Offset attributes.
/// Namespace prefixes are ignored. Struct parents qualify leaf names with dots.
/// No automatic memory-layout inference or optimized-block access is provided.
pub fn load_tia_xml(text: &str) -> Result<TagMap> {
    bounded(text)?;
    let document =
        roxmltree::Document::parse(text).map_err(|_| Error::InvalidInput("invalid TIA XML"))?;
    let db = document
        .descendants()
        .find(|node| node.has_tag_name("Number"))
        .and_then(|node| node.text())
        .ok_or(Error::InvalidInput("missing TIA DB number"))?;
    let db = super::parse::number(db.trim())?;
    let mut tags = TagMap::new();
    for member in document
        .descendants()
        .filter(|node| node.has_tag_name("Member"))
    {
        if member
            .children()
            .any(|node| node.is_element() && node.has_tag_name("Member"))
        {
            continue;
        }
        let Some(dtype) = member.attribute("Datatype") else {
            continue;
        };
        if dtype.eq_ignore_ascii_case("Struct") {
            continue;
        }
        let mut names: Vec<_> = member
            .ancestors()
            .filter(|node| node.has_tag_name("Member"))
            .filter_map(|node| node.attribute("Name"))
            .collect();
        names.reverse();
        let name = names.join(".");
        let position = member.attribute("Offset").ok_or(Error::Unsupported(
            "TIA member without explicit absolute Offset",
        ))?;
        let normalized = match dtype.to_ascii_uppercase().as_str() {
            "TIME_OF_DAY" => "TOD".to_owned(),
            "DATE_AND_TIME" => "DT".to_owned(),
            _ => dtype.to_ascii_uppercase(),
        };
        insert(&mut tags, make(&name, db, position, &normalized, None)?)?;
    }
    Ok(tags)
}
