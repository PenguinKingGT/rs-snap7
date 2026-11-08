//! Blocking read of a typed DB value; endpoint and address are supplied by the caller.
use rs_snap7::{ClientConfig, Result, blocking::Client, tags::Tag};
fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let endpoint = arguments.next().ok_or(rs_snap7::Error::InvalidInput(
        "usage: blocking_read HOST:PORT TAG",
    ))?;
    let tag: Tag = arguments
        .next()
        .ok_or(rs_snap7::Error::InvalidInput("missing typed tag"))?
        .parse()?;
    let endpoint = endpoint
        .parse()
        .map_err(|_| rs_snap7::Error::InvalidInput("invalid socket endpoint"))?;
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?)?;
    println!("{:?}", client.read_tag(&tag)?);
    client.disconnect()
}
