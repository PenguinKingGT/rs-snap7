//! Siemens LOGO! VM addressing over the standard blocking S7 client.

use crate::{Address, Client, ClientConfig, Error, Result, WordLen};
use std::{net::SocketAddr, str::FromStr};

/// A validated LOGO! VM address backed by DB1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmAddress {
    offset: u32,
    word: WordLen,
    bit: Option<u8>,
}

impl VmAddress {
    /// DB1 byte offset.
    pub const fn byte_offset(self) -> u32 {
        self.offset
    }
    /// Bit index, if this is a bit address.
    pub const fn bit(self) -> Option<u8> {
        self.bit
    }
    /// Width of the value, with bit addresses represented by `WordLen::Bit`.
    pub const fn word_len(self) -> WordLen {
        self.word
    }
}

impl FromStr for VmAddress {
    type Err = Error;

    /// Parse complete `V10`, `VW20`, `VD30`, or `V10.3` syntax, rejecting suffix junk.
    fn from_str(text: &str) -> Result<Self> {
        let tail = text
            .strip_prefix('V')
            .ok_or(Error::InvalidInput("LOGO address must begin with V"))?;
        let (word, number) = if let Some(n) = tail.strip_prefix('W') {
            (WordLen::Word, n)
        } else if let Some(n) = tail.strip_prefix('D') {
            (WordLen::DWord, n)
        } else {
            (WordLen::Byte, tail)
        };
        let (number, bit) = if let Some((n, bit)) = number.split_once('.') {
            if word != WordLen::Byte || bit.len() != 1 || !matches!(bit.as_bytes()[0], b'0'..=b'7')
            {
                return Err(Error::InvalidInput("invalid LOGO bit address"));
            }
            (n, Some(bit.as_bytes()[0] - b'0'))
        } else {
            (number, None)
        };
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::InvalidInput("invalid LOGO byte offset"));
        }
        let offset = number
            .parse::<u32>()
            .map_err(|_| Error::InvalidInput("LOGO offset overflow"))?;
        Address::db(1, offset, word.size() as u16)?;
        Ok(Self {
            offset,
            word: if bit.is_some() { WordLen::Bit } else { word },
            bit,
        })
    }
}

/// A thin address adapter over the deep standard client implementation.
#[derive(Debug, Clone)]
pub struct Logo {
    client: Client,
}

impl Logo {
    /// Connect using explicit LOGO TSAPs, preserving them during reconnect.
    pub fn connect(endpoint: SocketAddr, local_tsap: u16, remote_tsap: u16) -> Result<Self> {
        let mut config = ClientConfig::new(endpoint, 0, 0)?;
        config.transport.local_tsap = local_tsap;
        config.transport.remote_tsap = remote_tsap.to_be_bytes().to_vec();
        Self::connect_with_config(config)
    }

    /// Connect with caller-supplied timeouts, TSAPs, routing, and PDU settings.
    pub fn connect_with_config(config: ClientConfig) -> Result<Self> {
        Ok(Self::from_client(Client::connect(config)?))
    }
    /// Wrap an existing client; its shared session and settings are preserved.
    pub fn from_client(client: Client) -> Self {
        Self { client }
    }
    /// Access lifecycle and other supported standard-client capabilities.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Read a VM value. VW and VD use signed 16-/32-bit values as upstream does.
    pub fn read(&self, address: &str) -> Result<i64> {
        let address: VmAddress = address.parse()?;
        if let Some(bit) = address.bit {
            return Ok(i64::from(self.client.db_read_bool(
                1,
                address.offset,
                bit,
            )?));
        }
        match address.word {
            WordLen::Byte => Ok(i64::from(self.client.db_read_byte(1, address.offset)?)),
            WordLen::Word => Ok(i64::from(self.client.db_read_int(1, address.offset)?)),
            WordLen::DWord => Ok(i64::from(self.client.db_read_dint(1, address.offset)?)),
            _ => Err(Error::InvalidInput("invalid LOGO value width")),
        }
    }

    /// Write a VM value after checking its signed/unsigned width. Bit values use
    /// upstream's `value > 0` convention and preserve neighboring flags.
    pub fn write(&self, address: &str, value: i64) -> Result<()> {
        let address: VmAddress = address.parse()?;
        if let Some(bit) = address.bit {
            return self.client.db_write_bool(1, address.offset, bit, value > 0);
        }
        match address.word {
            WordLen::Byte => self.client.db_write_byte(
                1,
                address.offset,
                u8::try_from(value).map_err(|_| Error::InvalidInput("LOGO byte out of range"))?,
            ),
            WordLen::Word => self.client.db_write_int(
                1,
                address.offset,
                i16::try_from(value).map_err(|_| Error::InvalidInput("LOGO word out of range"))?,
            ),
            WordLen::DWord => self.client.db_write_dint(
                1,
                address.offset,
                i32::try_from(value).map_err(|_| Error::InvalidInput("LOGO dword out of range"))?,
            ),
            _ => Err(Error::InvalidInput("invalid LOGO value width")),
        }
    }
}
