//! Typed tag I/O; all address/value semantics live in the shared tags module.

use super::Client;
use crate::{
    Result,
    tags::{DataType, Tag, Value},
};

impl Client {
    /// Read and decode one classic tag, splitting large transfers as necessary.
    pub async fn read_tag(&self, tag: &Tag) -> Result<Value> {
        self.observe("read_tag", async {
            tag.classic()?;
            tag.decode(
                &self
                    .read_area(tag.area(), tag.db_number(), tag.offset(), tag.size())
                    .await?,
            )
        })
        .await
    }

    /// Encode before I/O; BOOL updates preserve neighbors under one session guard.
    /// Read/modify/write is not atomic against independent clients or PLC programs.
    pub async fn write_tag(&self, tag: &Tag, value: &Value) -> Result<()> {
        self.observe("write_tag", async {
            tag.classic()?;
            let mut bytes = tag.encode(value)?;
            let mut session = self.session().await;
            if tag.datatype() == DataType::Bool {
                let old = session
                    .read_area(tag.area(), tag.db_number(), tag.offset(), tag.size())
                    .await?;
                tag.merge_bits(&mut bytes, &old);
            }
            session
                .write_area(tag.area(), tag.db_number(), tag.offset(), &bytes)
                .await
        })
        .await
    }

    /// Read tags in input order, retaining item/decode errors per tag.
    /// Invalid symbolic tags fail before I/O; transport failures fail the batch.
    /// Transfers are sequential and permit interleaving between complete tag reads.
    pub async fn read_tags(&self, tags: &[Tag]) -> Result<Vec<Result<Value>>> {
        self.observe("read_tags", async {
            for tag in tags {
                tag.classic()?;
            }
            let mut values = Vec::with_capacity(tags.len());
            for tag in tags {
                match self.read_tag(tag).await {
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
        .await
    }
}
