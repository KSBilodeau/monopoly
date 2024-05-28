#![feature(lazy_cell)]
#![feature(concat_bytes)]
#![warn(clippy::pedantic)]
#![deny(rust_2018_idioms)]

use std::io::Read;
use std::os::unix::net::SocketAddr;
use std::sync::LazyLock;

use crate::api::CommandHandler;
use async_std::os::unix::net::{UnixListener, UnixStream};
use async_std::sync::Mutex;
use eyre::Result;
use log::*;
use soketto::handshake::server::Response;
use soketto::handshake::Server;

mod api;
mod game;

static GAME: LazyLock<Mutex<game::Session>> = LazyLock::new(|| Mutex::new(game::Session::new()));

async fn serve_websocket(stream: UnixStream, addr: SocketAddr) -> Result<()> {
    let mut rand_file = std::fs::File::open("/dev/random")?;
    let mut buf = [0u8; 4];
    rand_file.read_exact(&mut buf)?;

    let ws_id = u32::from_be_bytes(buf);

    info!(
        "Serving WS connection (ID #: {}) on {:?}",
        ws_id,
        addr.as_pathname()
    );

    let mut server = Server::new(stream);

    let websocket_key = {
        let Ok(req) = server.receive_request().await else {
            error!("Failed to receive connection request for WS (#{})", ws_id);
            return Ok(());
        };
        info!("Received request for path: {}", req.path());
        req.key()
    };

    let accept = Response::Accept {
        key: websocket_key,
        protocol: None,
    };

    let Ok(_) = server.send_response(&accept).await else {
        error!("Failed to accept WS (#{}) connection", ws_id);
        return Ok(());
    };

    let (mut sender, mut receiver) = server.into_builder().finish();

    let mut data = Vec::new();
    let mut comm_handler = CommandHandler::new();

    loop {
        let Ok(data_type) = receiver.receive_data(&mut data).await else {
            error!("Receiver closed prematurely on WS (#{})", ws_id);
            break;
        };

        if data_type.is_text() {
            let Ok(data) = std::str::from_utf8(&data) else {
                error!("Received invalid UTF-8 bytes on WS (#{})", ws_id);
                continue;
            };

            info!("Received data frame: {:?} \"{}\"", data_type, data);

            {
                let game = &mut *GAME.lock().await;

                comm_handler
                    .execute_command(ws_id, data, game, &mut sender)
                    .await;
                if comm_handler.is_kill() {
                    break;
                }

                info!("Game state: {:#?}", game);
            }
        }

        data.clear();
    }

    Ok(())
}

fn main() -> Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(LevelFilter::Debug)
        .init()?;

    assert!(
        std::env::var("MONOPOLY_HOST_KEY").is_ok(),
        "MISSING MONOPOLY_HOST_KEY ENV VAR"
    );
    assert!(
        std::env::var("MONOPOLY_GAME_PATH").is_ok(),
        "MISSING MONOPOLY_GAME_PATH ENV VAR"
    );
    assert!(
        std::env::var("MONOPOLY_CHOWN_ID").is_ok(),
        "MISSING MONOPOLY_CHOWN_ID ENV VAR"
    );

    async_std::task::block_on(async move {
        let sock_addr = std::env::var("MONOPOLY_GAME_PATH")?;

        let server = UnixListener::bind(&sock_addr).await?;
        info!("Listening on {}", &sock_addr);

        std::os::unix::fs::chown(sock_addr, Some(33), Some(33))?;

        while let Ok((stream, addr)) = server.accept().await {
            async_std::task::spawn(serve_websocket(stream, addr));
        }

        Ok(())
    })
}
