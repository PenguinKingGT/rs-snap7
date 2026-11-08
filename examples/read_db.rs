//! Read a typed DB value; endpoint and address are supplied by the caller.
use rs_snap7::{Client, ClientConfig, Result, tags::Tag};
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let endpoint = arguments.next().ok_or(rs_snap7::Error::InvalidInput(
        "usage: read_db HOST:PORT TAG",
    ))?;
    let tag: Tag = arguments
        .next()
        .ok_or(rs_snap7::Error::InvalidInput("missing typed tag"))?
        .parse()?;
    let endpoint = endpoint
        .parse()
        .map_err(|_| rs_snap7::Error::InvalidInput("invalid socket endpoint"))?;
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?).await?;
    println!("{:?}", client.read_tag(&tag).await?);
    client.disconnect().await
}
