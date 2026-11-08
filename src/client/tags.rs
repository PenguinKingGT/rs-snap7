//! Typed tag I/O; all address/value semantics live in the shared tags module.

use super::Client;
use crate::{
    Result,
    tags::{DataType, Tag, Value},
};

impl Client {
    /// Read and decode one classic tag, splitting large transfers as necessary.
    pub fn read_tag(&self, tag: &Tag) -> Result<Value> {
        self.observe("read_tag", || {
            tag.classic()?;
            tag.decode(&self.read_area(tag.area(), tag.db_number(), tag.offset(), tag.size())?)
        })
    }

    /// Encode before I/O; BOOL updates preserve neighbors under one session guard.
    /// Read/modify/write is not atomic against independent clients or PLC programs.
    pub fn write_tag(&self, tag: &Tag, value: &Value) -> Result<()> {
        self.observe("write_tag", || {
            tag.classic()?;
            let mut bytes = tag.encode(value)?;
            let mut session = self.session()?;
            if tag.datatype() == DataType::Bool {
                let old =
                    session.read_area(tag.area(), tag.db_number(), tag.offset(), tag.size())?;
                tag.merge_bits(&mut bytes, &old);
            }
            session.write_area(tag.area(), tag.db_number(), tag.offset(), &bytes)
        })
    }

    /// Read tags in input order, retaining item/decode errors per tag.
    /// Invalid symbolic tags fail before I/O; transport failures fail the batch.
    /// Transfers are sequential and permit interleaving between complete tag reads.
    pub fn read_tags(&self, tags: &[Tag]) -> Result<Vec<Result<Value>>> {
        self.observe("read_tags", || {
            for tag in tags {
                tag.classic()?;
            }
            let mut values = Vec::with_capacity(tags.len());
            for tag in tags {
                match self.read_tag(tag) {
                    Ok(value) => values.push(Ok(value)),
                    Err(
                        error @ (crate::Error::Item(_)
                        | crate::Error::InvalidInput(_)
                        | crate::Error::Protocol(_)),
                    ) => values.push(Err(error)),
                    Err(error) => return Err(error),
                }
            }
            Ok(values)
        })
    }
}
