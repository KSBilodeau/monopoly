#![feature(lazy_cell)]
#![feature(concat_bytes)]
#![warn(clippy::pedantic)]
#![deny(rust_2018_idioms)]

use std::io::Read;
use std::os::unix::net::SocketAddr;
use std::sync::{Arc, LazyLock};

use async_std::os::unix::net::{UnixListener, UnixStream};
use async_std::sync::Mutex;
use eyre::Result;
use log::*;
use soketto::handshake::server::Response;
use soketto::handshake::Server;

use crate::api::CommandHandler;

mod api;
mod game;

macro_rules! sync {
    ($future: expr) => {
        async_std::task::block_on(async { $future.await })
    };
}

pub(crate) use sync;

static GAME: LazyLock<Arc<Mutex<game::Session>>> =
    LazyLock::new(|| Arc::new(Mutex::new(game::Session::new())));

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

    let (sender, receiver) = server.into_builder().finish();
    let (sender, receiver) = (Arc::new(Mutex::new(sender)), Arc::new(Mutex::new(receiver)));

    let mut data = Vec::new();
    let mut comm_handler = CommandHandler::new(ws_id);

    let (send1, recv) = (sender.clone(), receiver.clone());
    let game = GAME.clone();
    let first_handle = std::thread::spawn(move || {
        info!("ENTERING FIRST SCOPED THREAD");

        loop {
            let Ok(data_type) = sync!(recv.lock().await.receive_data(&mut data)) else {
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
                    let game = &mut *sync!(game.lock());

                    {
                        sync!(comm_handler.execute_command(
                                data,
                                game,
                                &mut *send1.lock().await
                            ));
                    }

                    info!("Game state: {:#?}", game);
                }
            }

            data.clear();
        }
    });

    let send2 = sender.clone();
    let second_handle = std::thread::spawn(move || {
        info!("ENTERING SECOND SCOPED THREAD");

        while !first_handle.is_finished() {
            info!("SECOND SCOPED THREAD HEARTBEAT");
            std::thread::sleep(std::time::Duration::new(10, 0));
            sync!(send2.lock().await.send_text("0\nTEST")).unwrap();
        }
    });

    second_handle.join().unwrap();

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
