use std::net::SocketAddr;

use async_std::net::{TcpListener, TcpStream};
use eyre::{OptionExt, Result};
use log::*;
use soketto::handshake::Server;
use soketto::handshake::server::Response;

async fn serve_websocket(stream: TcpStream, addr: SocketAddr) -> Result<()> {
    info!("Serving WS connection on {}", addr);

    let mut server = Server::new(stream);

    let websocket_key = {
        let req = server.receive_request().await?;

        let headers = req.headers();
        info!("HOST: {}", std::str::from_utf8(&headers.host)?);
        info!("ORIGIN: {}", std::str::from_utf8(&headers.origin.ok_or_eyre("There was no origin")?)?);

        info!("Received request for path: {}", req.path());
        req.key()
    };

    let accept = Response::Accept { key: websocket_key, protocol: None };
    server.send_response(&accept).await?;

    let (mut sender, mut receiver) = server.into_builder().finish();

    let mut data = Vec::new();

    loop {
        let data_type = receiver.receive_data(&mut data).await?;

        if data_type.is_text() {
            let data = std::str::from_utf8(&data)?;

            info!("Received data frame: {:?} \"{}\"", data_type, data);

            let resp = handle_data(data).await?;
            sender.send_text(resp).await?;

            info!("Responded with: \"{}\"", resp);
        }

        data.clear();
    }
}

async fn handle_data(data: &str) -> Result<&str> {
    Ok(data)
}

fn main() -> Result<()> {
    simple_logger::SimpleLogger::new().with_level(LevelFilter::Debug).init()?;

    async_std::task::block_on(async move {
        let ip_addr = format!("127.0.0.1:{}", std::env::var("MONOPOLY_WS_PORT")?);

        let server = TcpListener::bind(&ip_addr).await?;
        info!("Listening on {}", &ip_addr);

        while let Ok((stream, addr)) = server.accept().await {
            async_std::task::spawn(serve_websocket(stream, addr));
        }

        Ok(())
    })
}
