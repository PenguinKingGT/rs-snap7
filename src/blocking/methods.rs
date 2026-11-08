//! Blocking invocation adapters; all S7 workflows execute in the async client.

use super::{Client, runtime};
use crate::{
    Address, Area, BlockType, CpuState, Result,
    blocks::{BlockEntry, BlockInfo, BlocksList},
    codec::{Scalar, datetime::DateTime},
    szl::{CpInfo, CpuInfo, DiagnosticEntry, ForceEntry, OrderCode, Protection, Szl},
    tags::{Tag, Value},
};

impl Client {
    /// Read a complete system status list, preserving its unmodified list body.
    /// ID/index, fragment count, total payload (1 MiB), and operation deadline are checked.
    pub fn read_szl(&self, id: u16, index: u16) -> Result<Szl> {
        runtime::run(self.inner.read_szl(id, index))
    }

    /// Read the available SZL ID list as the upstream raw list body.
    pub fn read_szl_list(&self) -> Result<Vec<u8>> {
        runtime::run(self.inner.read_szl_list())
    }

    /// Read CPU component identification.
    pub fn get_cpu_info(&self) -> Result<CpuInfo> {
        runtime::run(self.inner.get_cpu_info())
    }

    /// Read actual CPU state through SZL 0x0424, unlike upstream's constant RUN stub.
    pub fn get_cpu_state(&self) -> Result<CpuState> {
        runtime::run(self.inner.get_cpu_state())
    }

    /// Read communication processor information using the upstream field layout.
    pub fn get_cp_info(&self) -> Result<CpInfo> {
        runtime::run(self.inner.get_cp_info())
    }

    /// Read module order code, preferring installed firmware over bootloader version.
    pub fn get_order_code(&self) -> Result<OrderCode> {
        runtime::run(self.inner.get_order_code())
    }

    /// Read the CPU protection fields.
    pub fn get_protection(&self) -> Result<Protection> {
        runtime::run(self.inner.get_protection())
    }

    /// Read diagnostic events in wire order (upstream returns newest first).
    pub fn read_diagnostic_buffer(&self) -> Result<Vec<DiagnosticEntry>> {
        runtime::run(self.inner.read_diagnostic_buffer())
    }

    /// Unlock this session using a one-to-eight-character ASCII password.
    /// The password is encoded transiently and is never retained for automatic replay.
    pub fn set_session_password(&self, password: &str) -> Result<()> {
        runtime::run(self.inner.set_session_password(password))
    }

    /// Clear the current session password, accepting real-PLC null acknowledgements.
    pub fn clear_session_password(&self) -> Result<()> {
        runtime::run(self.inner.clear_session_password())
    }

    /// Read up to 20 variables, splitting large items and packets as required.
    /// Results follow input order; PLC item failures do not discard other items.
    /// A transport or malformed-message error fails the outer operation.
    pub fn read_multi_vars(&self, addresses: &[Address]) -> Result<Vec<Result<Vec<u8>>>> {
        runtime::run(self.inner.read_multi_vars(addresses))
    }

    /// Write up to 20 variables. Every payload is checked before the first exchange.
    /// Results preserve PLC item errors. An outer communication error reports total
    /// acknowledged bytes and the uncertain packet size; positions may be noncontiguous.
    pub fn write_multi_vars(&self, items: &[(Address, &[u8])]) -> Result<Vec<Result<()>>> {
        runtime::run(self.inner.write_multi_vars(items))
    }

    /// Update an input/output process-image bit under one local session guard.
    /// This is not a persistent CPU force job and may be overwritten by a scan
    /// or external writer. It does not add an entry to the CPU force table.
    pub fn force_bit(&self, area: Area, byte_offset: u32, bit: u8, value: bool) -> Result<()> {
        runtime::run(self.inner.force_bit(area, byte_offset, bit, value))
    }

    /// Clear one process-image bit, matching upstream cancellation behavior.
    /// The PLC scan may subsequently restore its natural value.
    pub fn cancel_force(&self, area: Area, byte_offset: u32, bit: u8) -> Result<()> {
        runtime::run(self.inner.cancel_force(area, byte_offset, bit))
    }

    /// Read CPU force records from SZL 0x0025. Explicit unsupported-service codes
    /// yield an empty list; transport failures and malformed data remain errors.
    pub fn read_force_table(&self) -> Result<Vec<ForceEntry>> {
        runtime::run(self.inner.read_force_table())
    }

    /// Read raw elements from an area. Timer/counter offsets and counts use elements;
    /// other areas use bytes. Large transfers are split using negotiated PDU limits.
    pub fn read_area(
        &self,
        area: crate::Area,
        db: u16,
        start: u32,
        count: usize,
    ) -> Result<Vec<u8>> {
        runtime::run(self.inner.read_area(area, db, start, count))
    }

    /// Write raw area data. Timer/counter payloads require complete 16-bit elements.
    /// Communication failures report uncertain writes; writes are never replayed.
    pub fn write_area(&self, area: crate::Area, db: u16, start: u32, data: &[u8]) -> Result<()> {
        runtime::run(self.inner.write_area(area, db, start, data))
    }

    /// Read bytes from a data block.
    pub fn db_read(&self, db: u16, start: u32, size: usize) -> Result<Vec<u8>> {
        runtime::run(self.inner.db_read(db, start, size))
    }

    /// Write bytes to a data block; acknowledged/uncertain progress is retained in errors.
    pub fn db_write(&self, db: u16, start: u32, data: &[u8]) -> Result<()> {
        runtime::run(self.inner.db_write(db, start, data))
    }

    /// Read an entire DB. Supply its size for PLCs with unreliable block metadata;
    /// `None` queries MC7 size, falling back to 65536 only for a reported zero size.
    pub fn db_get(&self, db: u16, size: Option<usize>) -> Result<Vec<u8>> {
        runtime::run(self.inner.db_get(db, size))
    }

    /// Fill a whole DB with one byte; `None` determines its size from block metadata.
    pub fn db_fill(&self, db: u16, filler: u8, size: Option<usize>) -> Result<()> {
        runtime::run(self.inner.db_fill(db, filler, size))
    }

    /// Read outputs; counts use the area's natural element units.
    pub fn ab_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        runtime::run(self.inner.ab_read(start, count))
    }

    /// Write outputs; payload bytes must contain complete elements.
    pub fn ab_write(&self, start: u32, data: &[u8]) -> Result<()> {
        runtime::run(self.inner.ab_write(start, data))
    }

    /// Read inputs; counts use the area's natural element units.
    pub fn eb_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        runtime::run(self.inner.eb_read(start, count))
    }

    /// Write inputs; payload bytes must contain complete elements.
    pub fn eb_write(&self, start: u32, data: &[u8]) -> Result<()> {
        runtime::run(self.inner.eb_write(start, data))
    }

    /// Read markers; counts use the area's natural element units.
    pub fn mb_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        runtime::run(self.inner.mb_read(start, count))
    }

    /// Write markers; payload bytes must contain complete elements.
    pub fn mb_write(&self, start: u32, data: &[u8]) -> Result<()> {
        runtime::run(self.inner.mb_write(start, data))
    }

    /// Read timer elements; counts use the area's natural element units.
    pub fn tm_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        runtime::run(self.inner.tm_read(start, count))
    }

    /// Write timer elements; payload bytes must contain complete elements.
    pub fn tm_write(&self, start: u32, data: &[u8]) -> Result<()> {
        runtime::run(self.inner.tm_write(start, data))
    }

    /// Read counter elements; counts use the area's natural element units.
    pub fn ct_read(&self, start: u32, count: usize) -> Result<Vec<u8>> {
        runtime::run(self.inner.ct_read(start, count))
    }

    /// Write counter elements; payload bytes must contain complete elements.
    pub fn ct_write(&self, start: u32, data: &[u8]) -> Result<()> {
        runtime::run(self.inner.ct_write(start, data))
    }

    /// Read and decode one classic tag, splitting large transfers as necessary.
    pub fn read_tag(&self, tag: &Tag) -> Result<Value> {
        runtime::run(self.inner.read_tag(tag))
    }

    /// Encode before I/O; BOOL updates preserve neighbors under one session guard.
    /// Read/modify/write is not atomic against independent clients or PLC programs.
    pub fn write_tag(&self, tag: &Tag, value: &Value) -> Result<()> {
        runtime::run(self.inner.write_tag(tag, value))
    }

    /// Read tags in input order, retaining item/decode errors per tag.
    /// Invalid symbolic tags fail before I/O; transport failures fail the batch.
    /// Transfers are sequential and permit interleaving between complete tag reads.
    pub fn read_tags(&self, tags: &[Tag]) -> Result<Vec<Result<Value>>> {
        runtime::run(self.inner.read_tags(tags))
    }

    /// Read one fixed-width scalar; numeric values use big-endian encoding.
    pub fn db_read_value<T: Scalar>(&self, db: u16, start: u32) -> Result<T> {
        runtime::run(self.inner.db_read_value::<T>(db, start))
    }

    /// Encode a scalar before sending it; an encoding error performs no network I/O.
    pub fn db_write_value<T: Scalar>(&self, db: u16, start: u32, value: T) -> Result<()> {
        runtime::run(self.inner.db_write_value::<T>(db, start, value))
    }

    /// Read a contiguous scalar array, splitting wire transfers as necessary.
    pub fn db_read_array<T: Scalar>(&self, db: u16, start: u32, count: usize) -> Result<Vec<T>> {
        runtime::run(self.inner.db_read_array::<T>(db, start, count))
    }

    /// Write a contiguous array after validating and encoding every value.
    pub fn db_write_array<T: Scalar + Copy>(
        &self,
        db: u16,
        start: u32,
        values: &[T],
    ) -> Result<()> {
        runtime::run(self.inner.db_write_array::<T>(db, start, values))
    }

    /// Read one bit (0–7) from a data block byte.
    pub fn db_read_bool(&self, db: u16, start: u32, bit: u8) -> Result<bool> {
        runtime::run(self.inner.db_read_bool(db, start, bit))
    }

    /// Update one DB bit while preserving its neighbors under one session guard.
    /// This read/modify/write is serialized against clones, but is not atomic
    /// against external PLC programs or independent client connections.
    pub fn db_write_bool(&self, db: u16, start: u32, bit: u8, value: bool) -> Result<()> {
        runtime::run(self.inner.db_write_bool(db, start, bit, value))
    }

    /// Read an S7 STRING in Latin-1; malformed headers are rejected before reading content.
    pub fn db_read_string(&self, db: u16, start: u32) -> Result<String> {
        runtime::run(self.inner.db_read_string(db, start))
    }

    /// Write a Latin-1 S7 STRING, clearing unused capacity like the scalar codec.
    pub fn db_write_string(&self, db: u16, start: u32, value: &str, capacity: u8) -> Result<()> {
        runtime::run(self.inner.db_write_string(db, start, value, capacity))
    }

    /// Read a UTF-16BE S7 WSTRING after validating its capacity and current length.
    pub fn db_read_wstring(&self, db: u16, start: u32) -> Result<String> {
        runtime::run(self.inner.db_read_wstring(db, start))
    }

    /// Write a BMP-only WSTRING header and actual content; unused capacity is untouched.
    pub fn db_write_wstring(&self, db: u16, start: u32, value: &str, capacity: u16) -> Result<()> {
        runtime::run(self.inner.db_write_wstring(db, start, value, capacity))
    }

    /// Read one S7 BYTE value from a DB byte offset.
    pub fn db_read_byte(&self, db: u16, start: u32) -> Result<u8> {
        runtime::run(self.inner.db_read_byte(db, start))
    }

    /// Write one S7 BYTE value to a DB byte offset.
    pub fn db_write_byte(&self, db: u16, start: u32, value: u8) -> Result<()> {
        runtime::run(self.inner.db_write_byte(db, start, value))
    }

    /// Read one S7 INT value from a DB byte offset.
    pub fn db_read_int(&self, db: u16, start: u32) -> Result<i16> {
        runtime::run(self.inner.db_read_int(db, start))
    }

    /// Write one S7 INT value to a DB byte offset.
    pub fn db_write_int(&self, db: u16, start: u32, value: i16) -> Result<()> {
        runtime::run(self.inner.db_write_int(db, start, value))
    }

    /// Read one S7 UINT value from a DB byte offset.
    pub fn db_read_uint(&self, db: u16, start: u32) -> Result<u16> {
        runtime::run(self.inner.db_read_uint(db, start))
    }

    /// Write one S7 UINT value to a DB byte offset.
    pub fn db_write_uint(&self, db: u16, start: u32, value: u16) -> Result<()> {
        runtime::run(self.inner.db_write_uint(db, start, value))
    }

    /// Read one S7 WORD value from a DB byte offset.
    pub fn db_read_word(&self, db: u16, start: u32) -> Result<u16> {
        runtime::run(self.inner.db_read_word(db, start))
    }

    /// Write one S7 WORD value to a DB byte offset.
    pub fn db_write_word(&self, db: u16, start: u32, value: u16) -> Result<()> {
        runtime::run(self.inner.db_write_word(db, start, value))
    }

    /// Read one S7 DINT value from a DB byte offset.
    pub fn db_read_dint(&self, db: u16, start: u32) -> Result<i32> {
        runtime::run(self.inner.db_read_dint(db, start))
    }

    /// Write one S7 DINT value to a DB byte offset.
    pub fn db_write_dint(&self, db: u16, start: u32, value: i32) -> Result<()> {
        runtime::run(self.inner.db_write_dint(db, start, value))
    }

    /// Read one S7 UDINT value from a DB byte offset.
    pub fn db_read_udint(&self, db: u16, start: u32) -> Result<u32> {
        runtime::run(self.inner.db_read_udint(db, start))
    }

    /// Write one S7 UDINT value to a DB byte offset.
    pub fn db_write_udint(&self, db: u16, start: u32, value: u32) -> Result<()> {
        runtime::run(self.inner.db_write_udint(db, start, value))
    }

    /// Read one S7 DWORD value from a DB byte offset.
    pub fn db_read_dword(&self, db: u16, start: u32) -> Result<u32> {
        runtime::run(self.inner.db_read_dword(db, start))
    }

    /// Write one S7 DWORD value to a DB byte offset.
    pub fn db_write_dword(&self, db: u16, start: u32, value: u32) -> Result<()> {
        runtime::run(self.inner.db_write_dword(db, start, value))
    }

    /// Read one S7 REAL value from a DB byte offset.
    pub fn db_read_real(&self, db: u16, start: u32) -> Result<f32> {
        runtime::run(self.inner.db_read_real(db, start))
    }

    /// Write one S7 REAL value to a DB byte offset.
    pub fn db_write_real(&self, db: u16, start: u32, value: f32) -> Result<()> {
        runtime::run(self.inner.db_write_real(db, start, value))
    }

    /// Read one S7 LREAL value from a DB byte offset.
    pub fn db_read_lreal(&self, db: u16, start: u32) -> Result<f64> {
        runtime::run(self.inner.db_read_lreal(db, start))
    }

    /// Write one S7 LREAL value to a DB byte offset.
    pub fn db_write_lreal(&self, db: u16, start: u32, value: f64) -> Result<()> {
        runtime::run(self.inner.db_write_lreal(db, start, value))
    }

    /// Return negotiated PDU bytes or a disconnected-session error.
    pub fn pdu_length(&self) -> Result<u16> {
        runtime::run(self.inner.pdu_length())
    }

    /// Disconnect the shared socket; repeated calls are harmless.
    pub fn disconnect(&self) -> Result<()> {
        runtime::run(self.inner.disconnect())
    }

    /// Explicitly reconnect with fresh negotiation and no automatic operation replay.
    pub fn reconnect(&self) -> Result<()> {
        runtime::run(self.inner.reconnect())
    }

    /// Exchange a custom S7 request using a fresh session-owned reference.
    pub fn iso_exchange(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        runtime::run(self.inner.iso_exchange(bytes))
    }

    /// Read all seven catalog block counts, validating the complete table.
    pub fn list_blocks(&self) -> Result<BlocksList> {
        runtime::run(self.inner.list_blocks())
    }

    /// Read all entries of one type, including language/flags and fragmented responses.
    pub fn list_block_entries(&self, kind: BlockType) -> Result<Vec<BlockEntry>> {
        runtime::run(self.inner.list_block_entries(kind))
    }

    /// Return at most `limit` block numbers. A zero limit performs no wire request.
    pub fn list_blocks_of_type(&self, kind: BlockType, limit: usize) -> Result<Vec<u16>> {
        runtime::run(self.inner.list_blocks_of_type(kind, limit))
    }

    /// Read canonical remote metadata and verify its type and block number.
    pub fn get_block_info(&self, kind: BlockType, number: u16) -> Result<BlockInfo> {
        runtime::run(self.inner.get_block_info(kind, number))
    }

    /// Upload a DB's data body, omitting its initial 36-byte compact header.
    pub fn upload(&self, number: u16) -> Result<Vec<u8>> {
        runtime::run(self.inner.upload(number))
    }

    /// Upload any block type with its complete header/body/footer representation.
    /// The returned Vec's length replaces Python's redundant `(buffer, size)` tuple.
    pub fn full_upload(&self, kind: BlockType, number: u16) -> Result<Vec<u8>> {
        runtime::run(self.inner.full_upload(kind, number))
    }

    /// Download a DB representation using PLC-driven slice jobs. `None` extracts
    /// its number from bytes 6–7; short input requires an explicit number.
    /// Interrupted downloads have an uncertain remote outcome and never replay.
    pub fn download(&self, data: &[u8], number: Option<u16>) -> Result<()> {
        runtime::run(self.inner.download(data, number))
    }

    /// Delete a block using the upstream _DELE PI service, without automatic replay.
    pub fn delete(&self, kind: BlockType, number: u16) -> Result<()> {
        runtime::run(self.inner.delete(kind, number))
    }

    /// Stop the PLC CPU. The operation is never automatically replayed.
    pub fn plc_stop(&self) -> Result<()> {
        runtime::run(self.inner.plc_stop())
    }

    /// Hot-start the PLC CPU without automatically replaying the operation.
    pub fn plc_hot_start(&self) -> Result<()> {
        runtime::run(self.inner.plc_hot_start())
    }

    /// Cold-start the PLC CPU without automatically replaying the operation.
    pub fn plc_cold_start(&self) -> Result<()> {
        runtime::run(self.inner.plc_cold_start())
    }

    /// Compress PLC memory using the configured operation timeout.
    pub fn compress(&self) -> Result<()> {
        runtime::run(self.inner.compress())
    }

    /// Copy PLC RAM to ROM using the configured operation timeout.
    pub fn copy_ram_to_rom(&self) -> Result<()> {
        runtime::run(self.inner.copy_ram_to_rom())
    }

    /// Read the PLC's timezone-free clock value, including milliseconds.
    pub fn get_plc_datetime(&self) -> Result<DateTime> {
        runtime::run(self.inner.get_plc_datetime())
    }

    /// Set the PLC clock in its configured local calendar convention (1990–2089).
    pub fn set_plc_datetime(&self, value: DateTime) -> Result<()> {
        runtime::run(self.inner.set_plc_datetime(value))
    }

    /// Set the PLC clock to current UTC system time. Unlike Python's local-time
    /// helper this has no implicit host timezone; supply `DateTime` for local PLC time.
    pub fn set_plc_system_datetime(&self) -> Result<()> {
        runtime::run(self.inner.set_plc_system_datetime())
    }
}
