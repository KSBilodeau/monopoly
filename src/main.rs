use std::net::TcpListener;
use eyre::Result;
use tungstenite::accept;

fn main() -> Result<()> {
    let port_str = std::env::var("MONOPOLY_SERVER_PORT")?;

    let server = TcpListener::bind(format!("0.0.0.0:{}", port_str))?;
    for stream in server.incoming() {
        let mut websocket = accept(stream?)?;

        loop {
            let msg = websocket.read()?;

            if msg.is_binary() || msg.is_text() {
                websocket.send(msg)?;
            }
        }
    }


    Ok(())
}
