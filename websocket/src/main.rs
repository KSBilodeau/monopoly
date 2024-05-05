use std::net::SocketAddr;

use async_std::net::{TcpListener, TcpStream};
use eyre::Result;
use log::*;
use soketto::handshake::Server;
use soketto::handshake::server::Response;

async fn serve_websocket(stream: TcpStream, addr: SocketAddr) -> Result<()> {
    info!("Serving WS connection on {}", addr);

    let mut server = Server::new(stream);

    let websocket_key = {
        let req = server.receive_request().await?;
        info!("Received request for path: {}", req.path());
        req.key()
    };

    let accept = Response::Accept { key: websocket_key, protocol: None };
    server.send_response(&accept).await?;

    let (mut sender, mut receiver) = server.into_builder().finish();

    let mut data = Vec::new();

    loop {
        let data_type = receiver.receive_data(&mut data).await?;
        info!("Received data frame: {:#?}", data_type);

        if data_type.is_text() {
            sender.send_text(std::str::from_utf8(&data)?).await?;
        }

        data.clear();
    }
}

fn main() -> Result<()> {
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
