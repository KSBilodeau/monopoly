#![feature(async_closure)]
#![warn(clippy::pedantic)]
#![deny(rust_2018_idioms)]

use std::io::Read;
use std::os::unix::net::SocketAddr;
use std::sync::Arc;

use async_std::os::unix::net::{UnixListener, UnixStream};
use async_std::sync::Mutex;
use eyre::Result;
use log::*;
use soketto::handshake::server::Response;
use soketto::handshake::Server;

mod api;
mod game;
mod util;

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

    let (send, recv) = {
        let (send, recv) = server.into_builder().finish();
        (Arc::new(Mutex::new(send)), Arc::new(Mutex::new(recv)))
    };

    let sock_handler = api::SocketHandler::new(ws_id, send, recv);

    let mut comm_handler = sock_handler.comm_handler();
    async_std::task::spawn(async move {
        loop {
            let command = comm_handler.pump().await;

            if let Some(command) = command {
                comm_handler.execute(&command).await;
            }

            if comm_handler.is_killed() {
                break;
            }
        }
    });

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

    async_std::task::block_on(async {
        let sock_addr = std::env::var("MONOPOLY_GAME_PATH")?;

        let server = UnixListener::bind(&sock_addr).await?;
        info!("Listening on {}", &sock_addr);

        std::os::unix::fs::chown(sock_addr, Some(33), Some(33))?;

        while let Ok((stream, addr)) = server.accept().await {
            async_std::task::spawn(async { serve_websocket(stream, addr).await });
        }

        Ok(())
    })
}
