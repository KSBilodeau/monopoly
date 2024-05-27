#![feature(lazy_cell)]
#![feature(concat_bytes)]
#![warn(clippy::pedantic)]
#![deny(rust_2018_idioms)]

use std::os::unix::net::SocketAddr;

use crate::api::Command;
use async_std::os::unix::net::{UnixListener, UnixStream};
use async_std::sync::Mutex;
use eyre::Result;
use log::*;
use soketto::handshake::server::Response;
use soketto::handshake::Server;

mod api;
mod game;

static GAME: Mutex<game::Session> = Mutex::new(game::Session::new());

async fn serve_websocket(stream: UnixStream, addr: SocketAddr) -> Result<()> {
    info!("Serving WS connection on {:?}", addr);

    let mut server = Server::new(stream);

    let websocket_key = {
        let req = server.receive_request().await?;
        info!("Received request for path: {}", req.path());
        req.key()
    };

    let accept = Response::Accept {
        key: websocket_key,
        protocol: None,
    };
    server.send_response(&accept).await?;

    let (mut sender, mut receiver) = server.into_builder().finish();

    let mut data = Vec::new();

    loop {
        let data_type = receiver.receive_data(&mut data).await?;

        if data_type.is_text() {
            let data = std::str::from_utf8(&data)?;

            info!("Received data frame: {:?} \"{}\"", data_type, data);

            {
                let game = &mut *GAME.lock().await;

                let command = Command::new(data)
                    .execute(game)
                    .await
                    .respond(&mut sender)
                    .await;

                match command {
                    Command::INIT(init) => info!(
                        "Command {} completed successfully: {:#?}",
                        init.nonce(),
                        init
                    ),
                    Command::ECHO(echo) => info!(
                        "Command {} completed successfully {:#?}",
                        echo.nonce(),
                        echo
                    ),
                    Command::ERROR(error) => error!(
                        "Command {} completed with error code: {}",
                        error.nonce(),
                        error.code()
                    ),
                }

                info!("Game state: {:#?}", game);
            }
        }

        data.clear();
    }
}

fn main() -> Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(LevelFilter::Debug)
        .init()?;

    async_std::task::block_on(async move {
        let ip_addr = std::env::var("MONOPOLY_GAME_PATH")?;

        let server = UnixListener::bind(&ip_addr).await?;
        info!("Listening on {}", &ip_addr);

        std::os::unix::fs::chown(ip_addr, Some(33), Some(33))?;

        while let Ok((stream, addr)) = server.accept().await {
            async_std::task::spawn(serve_websocket(stream, addr));
        }

        Ok(())
    })
}
