//! Native asynchronous typed read with explicit endpoint arguments.
#[tokio::main(flavor = "current_thread")]
async fn main() -> rs_snap7::Result<()> {
    use rs_snap7::{Client, ClientConfig, Error};
    let endpoint = std::env::args()
        .nth(1)
        .ok_or(Error::InvalidInput("usage: async_read HOST:PORT"))?
        .parse()
        .map_err(|_| Error::InvalidInput("invalid endpoint"))?;
    let client = Client::connect(ClientConfig::new(endpoint, 0, 1)?).await?;
    println!("{}", client.db_read_int(1, 0).await?);
    client.disconnect().await
}
