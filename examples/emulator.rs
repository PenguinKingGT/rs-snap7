//! Run a local emulator with DB1, ending when Enter is pressed.
#[cfg(feature = "server")]
fn main() -> rs_snap7::Result<()> {
    use rs_snap7::{
        Area,
        server::{Server, ServerConfig},
    };
    let server = Server::start(ServerConfig::new(
        "127.0.0.1:1102".parse().expect("constant endpoint"),
    ))?;
    server.register_area(Area::Db, 1, vec![0; 1024])?;
    println!("Listening on {}; press Enter to stop", server.local_addr());
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(())
}
#[cfg(not(feature = "server"))]
fn main() {
    eprintln!("Run with --features server");
}
