use async_std::net::TcpListener;
use eyre::Result;
use soketto::handshake::Server;
use soketto::handshake::server::Response;

fn main() -> Result<()> {
    async_std::task::block_on(async move {
        let ip_addr = format!("127.0.0.1:{}", std::env::var("MONOPOLY_WS_PORT")?);
        let server = TcpListener::bind(ip_addr).await?;

        while let Ok((stream, _)) = server.accept().await {
            let mut server = Server::new(stream);

            let websocket_key = {
                let req = server.receive_request().await?;
                req.key()
            };

            let accept = Response::Accept { key: websocket_key, protocol: None };
            server.send_response(&accept).await?;

            let (mut sender, mut receiver) = server.into_builder().finish();

            let mut data = Vec::new();
            let data_type = receiver.receive_data(&mut data).await?;

            if data_type.is_text() {
                sender.send_text(std::str::from_utf8(&data)?).await?;
            }

            sender.close().await?;
        }

        Ok(())
    })
}
