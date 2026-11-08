//! Validated S7 addresses and protocol identifiers.

use crate::{Error, Result};

/// S7 memory area identifiers, matching the wire protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Area {
    /// Process inputs.
    Inputs = 0x81,
    /// Process outputs.
    Outputs = 0x82,
    /// Marker memory.
    Markers = 0x83,
    /// Data block memory.
    Db = 0x84,
    /// Counter elements.
    Counters = 0x1c,
    /// Timer elements.
    Timers = 0x1d,
}

impl TryFrom<u8> for Area {
    type Error = Error;

    /// Reject unknown identifiers rather than silently substituting a value.
    fn try_from(value: u8) -> Result<Self> {
        match value {
            0x81 => Ok(Self::Inputs),
            0x82 => Ok(Self::Outputs),
            0x83 => Ok(Self::Markers),
            0x84 => Ok(Self::Db),
            0x1c => Ok(Self::Counters),
            0x1d => Ok(Self::Timers),
            _ => Err(Error::InvalidInput("unknown Area identifier")),
        }
    }
}

/// S7-ANY transport widths; timer/counter offsets count elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum WordLen {
    /// Single bit.
    Bit = 0x01,
    /// Unsigned byte.
    Byte = 0x02,
    /// Character byte.
    Char = 0x03,
    /// Unsigned 16-bit word.
    Word = 0x04,
    /// Signed 16-bit integer.
    Int = 0x05,
    /// Unsigned 32-bit word.
    DWord = 0x06,
    /// Signed 32-bit integer.
    DInt = 0x07,
    /// IEEE 754 binary32.
    Real = 0x08,
    /// Classic counter.
    Counter = 0x1c,
    /// Classic timer.
    Timer = 0x1d,
    /// S7-200 counter.
    Counter200 = 0x1e,
    /// S7-200 timer.
    Timer200 = 0x1f,
}

impl TryFrom<u8> for WordLen {
    type Error = Error;

    /// Reject unknown identifiers rather than silently substituting a value.
    fn try_from(value: u8) -> Result<Self> {
        match value {
            0x01 => Ok(Self::Bit),
            0x02 => Ok(Self::Byte),
            0x03 => Ok(Self::Char),
            0x04 => Ok(Self::Word),
            0x05 => Ok(Self::Int),
            0x06 => Ok(Self::DWord),
            0x07 => Ok(Self::DInt),
            0x08 => Ok(Self::Real),
            0x1c => Ok(Self::Counter),
            0x1d => Ok(Self::Timer),
            0x1e => Ok(Self::Counter200),
            0x1f => Ok(Self::Timer200),
            _ => Err(Error::InvalidInput("unknown WordLen identifier")),
        }
    }
}

/// Parameter identifiers retained for upstream interoperability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Parameter {
    /// Local TCP port.
    LocalPort = 0x01,
    /// Remote TCP port.
    RemotePort = 0x02,
    /// Connection probe timeout.
    PingTimeout = 0x03,
    /// Send timeout.
    SendTimeout = 0x04,
    /// Receive timeout.
    RecvTimeout = 0x05,
    /// Worker interval.
    WorkInterval = 0x06,
    /// Local COTP reference.
    SrcRef = 0x07,
    /// Remote COTP reference.
    DstRef = 0x08,
    /// Calling TSAP.
    SrcTsap = 0x09,
    /// Requested PDU size.
    PduRequest = 0x0a,
    /// Server connection limit.
    MaxClients = 0x0b,
    /// Partner send timeout.
    BSendTimeout = 0x0c,
    /// Partner receive timeout.
    BRecvTimeout = 0x0d,
    /// Recovery interval.
    RecoveryTime = 0x0e,
    /// Keepalive interval.
    KeepAliveTime = 0x0f,
}

impl TryFrom<u8> for Parameter {
    type Error = Error;

    /// Reject unknown identifiers rather than silently substituting a value.
    fn try_from(value: u8) -> Result<Self> {
        match value {
            0x01 => Ok(Self::LocalPort),
            0x02 => Ok(Self::RemotePort),
            0x03 => Ok(Self::PingTimeout),
            0x04 => Ok(Self::SendTimeout),
            0x05 => Ok(Self::RecvTimeout),
            0x06 => Ok(Self::WorkInterval),
            0x07 => Ok(Self::SrcRef),
            0x08 => Ok(Self::DstRef),
            0x09 => Ok(Self::SrcTsap),
            0x0a => Ok(Self::PduRequest),
            0x0b => Ok(Self::MaxClients),
            0x0c => Ok(Self::BSendTimeout),
            0x0d => Ok(Self::BRecvTimeout),
            0x0e => Ok(Self::RecoveryTime),
            0x0f => Ok(Self::KeepAliveTime),
            _ => Err(Error::InvalidInput("unknown Parameter identifier")),
        }
    }
}

/// PLC block type identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum BlockType {
    /// Organization block.
    Ob = 0x38,
    /// Data block.
    Db = 0x41,
    /// System data block.
    Sdb = 0x42,
    /// Function.
    Fc = 0x43,
    /// System function.
    Sfc = 0x44,
    /// Function block.
    Fb = 0x45,
    /// System function block.
    Sfb = 0x46,
}

impl TryFrom<u8> for BlockType {
    type Error = Error;

    /// Reject unknown identifiers rather than silently substituting a value.
    fn try_from(value: u8) -> Result<Self> {
        match value {
            0x38 => Ok(Self::Ob),
            0x41 => Ok(Self::Db),
            0x42 => Ok(Self::Sdb),
            0x43 => Ok(Self::Fc),
            0x44 => Ok(Self::Sfc),
            0x45 => Ok(Self::Fb),
            0x46 => Ok(Self::Sfb),
            _ => Err(Error::InvalidInput("unknown BlockType identifier")),
        }
    }
}

/// Server registration identifiers, distinct from wire area identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ServerArea {
    /// Process inputs.
    Inputs = 0x00,
    /// Process outputs.
    Outputs = 0x01,
    /// Markers.
    Markers = 0x02,
    /// Counters.
    Counters = 0x03,
    /// Timers.
    Timers = 0x04,
    /// Data blocks.
    Db = 0x05,
}

impl TryFrom<u8> for ServerArea {
    type Error = Error;

    /// Reject unknown identifiers rather than silently substituting a value.
    fn try_from(value: u8) -> Result<Self> {
        match value {
            0x00 => Ok(Self::Inputs),
            0x01 => Ok(Self::Outputs),
            0x02 => Ok(Self::Markers),
            0x03 => Ok(Self::Counters),
            0x04 => Ok(Self::Timers),
            0x05 => Ok(Self::Db),
            _ => Err(Error::InvalidInput("unknown ServerArea identifier")),
        }
    }
}

/// PLC operating state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CpuState {
    /// Unknown state.
    Unknown = 0x00,
    /// Stopped CPU.
    Stop = 0x04,
    /// Running CPU.
    Run = 0x08,
}

impl TryFrom<u8> for CpuState {
    type Error = Error;

    /// Reject unknown identifiers rather than silently substituting a value.
    fn try_from(value: u8) -> Result<Self> {
        match value {
            0x00 => Ok(Self::Unknown),
            0x04 => Ok(Self::Stop),
            0x08 => Ok(Self::Run),
            _ => Err(Error::InvalidInput("unknown CpuState identifier")),
        }
    }
}

impl WordLen {
    /// Storage bytes per element; bit transfers use one byte per result bit.
    pub const fn size(self) -> usize {
        match self {
            Self::Bit | Self::Byte | Self::Char => 1,
            Self::Word
            | Self::Int
            | Self::Counter
            | Self::Timer
            | Self::Counter200
            | Self::Timer200 => 2,
            Self::DWord | Self::DInt | Self::Real => 4,
        }
    }

    /// Whether an address is an element index rather than a byte offset.
    pub const fn is_indexed(self) -> bool {
        matches!(
            self,
            Self::Counter | Self::Timer | Self::Counter200 | Self::Timer200
        )
    }
}

impl Area {
    /// Default transfer width for raw area operations.
    pub const fn word_len(self) -> WordLen {
        match self {
            Self::Counters => WordLen::Counter,
            Self::Timers => WordLen::Timer,
            _ => WordLen::Byte,
        }
    }
}

/// A validated S7-ANY address. Byte offsets, bit offsets, and element indexes
/// follow the selected word length; the wire address is limited to 24 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Address {
    area: Area,
    db: u16,
    start: u32,
    word_len: WordLen,
    count: u16,
}

impl Address {
    /// Validate the complete transfer range. Non-DB areas require DB number zero.
    pub fn new(area: Area, db: u16, start: u32, word_len: WordLen, count: u16) -> Result<Self> {
        if count == 0 || (area != Area::Db && db != 0) {
            return Err(Error::InvalidInput(
                "empty transfer or DB number on non-DB area",
            ));
        }
        let indexed_area = matches!(area, Area::Timers | Area::Counters);
        if indexed_area != word_len.is_indexed()
            || (area == Area::Timers && !matches!(word_len, WordLen::Timer | WordLen::Timer200))
            || (area == Area::Counters
                && !matches!(word_len, WordLen::Counter | WordLen::Counter200))
        {
            return Err(Error::InvalidInput("word length does not match area"));
        }
        let scale = if word_len == WordLen::Bit || word_len.is_indexed() {
            1
        } else {
            8
        };
        let width = if word_len.is_indexed() || word_len == WordLen::Bit {
            1
        } else {
            word_len.size() as u64 * 8
        };
        let end = u64::from(start) * scale + u64::from(count) * width;
        if end > 0x0100_0000 {
            return Err(Error::InvalidInput("transfer exceeds 24-bit address space"));
        }
        Ok(Self {
            area,
            db,
            start,
            word_len,
            count,
        })
    }

    /// Construct a raw byte range in a data block.
    pub fn db(db: u16, start: u32, count: u16) -> Result<Self> {
        Self::new(Area::Db, db, start, WordLen::Byte, count)
    }

    /// Memory area.
    pub const fn area(self) -> Area {
        self.area
    }
    /// Data block number, or zero for other areas.
    pub const fn db_number(self) -> u16 {
        self.db
    }
    /// Offset in the units described by the selected word length.
    pub const fn start(self) -> u32 {
        self.start
    }
    /// Selected transfer width.
    pub const fn word_len(self) -> WordLen {
        self.word_len
    }
    /// Number of transferred elements.
    pub const fn count(self) -> u16 {
        self.count
    }
    /// Decoded payload size in bytes.
    pub const fn byte_len(self) -> usize {
        self.count as usize * self.word_len.size()
    }

    /// Encode the 12-byte S7-ANY specification in network byte order.
    pub fn encode(self) -> [u8; 12] {
        let offset = if self.word_len == WordLen::Bit || self.word_len.is_indexed() {
            self.start
        } else {
            self.start * 8
        };
        let count = self.count.to_be_bytes();
        let db = self.db.to_be_bytes();
        let start = offset.to_be_bytes();
        [
            0x12,
            0x0a,
            0x10,
            self.word_len as u8,
            count[0],
            count[1],
            db[0],
            db[1],
            self.area as u8,
            start[1],
            start[2],
            start[3],
        ]
    }
}
