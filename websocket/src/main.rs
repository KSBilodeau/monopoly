#![feature(lazy_cell)]
#![feature(concat_bytes)]
#![warn(clippy::pedantic)]
#![allow(dead_code)]
#![allow(unused_mut)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![deny(rust_2018_idioms)]

use std::io::Read;
use std::os::unix::net::SocketAddr;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_std::os::unix::net::{UnixListener, UnixStream};
use eyre::Result;
use log::*;
use parking_lot::Mutex;
use soketto::handshake::Server;
use soketto::handshake::server::Response;

use crate::api::back::EventHandler;
use crate::api::front::CommandHandler;

mod api;
mod game;
mod util;

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

    // let (sender, receiver) = {
    //     let (send, recv) = server.into_builder().finish();
    //     (Arc::new(Mutex::new(send)), Arc::new(Mutex::new(recv)))
    // };

    let (mut sender, _) = server.into_builder().finish();

    util::sync!(sender.send_text("CONNECTED")).unwrap();

    // let (send, recv) = std::sync::mpsc::channel();
    //
    // let mut comm_handler = CommandHandler::new(ws_id, send, GAME.clone());
    // let mut event_handler = EventHandler::new(ws_id, recv, GAME.clone());
    //
    // std::thread::scope(|s| {
    //     let send1 = sender.clone();
    //     let recv1 = receiver.clone();
    //     s.spawn(move || {
    //         let sender = send1.clone();
    //         let receiver = recv1.clone();
    //
    //         loop {
    //             // let command = comm_handler.pump_command(receiver.clone());
    //             //
    //             // if let Some(command) = command {
    //             //     comm_handler.execute_command(&command, sender.clone());
    //             // }
    //             //
    //             // if comm_handler.is_kill() {
    //             //     break;
    //             // }
    //         }
    //     });
    //
    //     let send2 = sender.clone();
    //     s.spawn(move || {
    //         let sender = send2.clone();
    //         loop {
    //             // let event = event_handler.pump_event();
    //             //
    //             // if let Some(event) = event {
    //             //     event_handler.execute_event(event, sender.clone());
    //             // }
    //             //
    //             // if event_handler.is_kill() {
    //             //     break;
    //             // }
    //         }
    //     });
    // });

    Ok(())
}

async fn _test_serve_websocket(stream: UnixStream, addr: SocketAddr) {
    let mut rand_file = std::fs::File::open("/dev/random").unwrap();
    let mut buf = [0u8; 4];
    rand_file.read_exact(&mut buf).unwrap();

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
            return;
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
        return;
    };

    let (mut send, _) = server.into_builder().finish();

    loop {
        send.send_text(format!("{}", ws_id)).await.unwrap();
        async_std::task::sleep(Duration::new(10, 0)).await;
    }
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
