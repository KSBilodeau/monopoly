use std::net::SocketAddr;

use async_std::net::{TcpListener, TcpStream};
use async_std::sync::Mutex;
use eyre::Result;
use log::*;
use soketto::handshake::server::Response;
use soketto::handshake::Server;

#[derive(Debug)]
struct Game {
    host: Option<String>,
    players: Vec<String>,
}

static GAME: Mutex<Game> = Mutex::new(Game {
    host: None,
    players: vec![],
});

async fn serve_websocket(stream: TcpStream, addr: SocketAddr) -> Result<()> {
    info!("Serving WS connection on {}", addr);

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

    {
        receiver.receive_data(&mut data).await?;

        let data = std::str::from_utf8(&data)?;
        let mut request = data.lines();

        match handle_init(&mut request).await {
            Ok(msg) => sender.send_text(msg).await?,
            Err(e) => {
                sender.send_text(format!("{}", e)).await?;
                sender.close().await?;
                info!("Received incorrect init procedure on {}", addr);
                info!("Closed connection on {}", addr);
                return Ok(());
            }
        }
    }

    loop {
        let data_type = receiver.receive_data(&mut data).await?;

        if data_type.is_text() {
            let data = std::str::from_utf8(&data)?;

            info!("Received data frame: {:?} \"{}\"", data_type, data);

            let resp = handle_data(data).await?;
            sender.send_text(&resp).await?;

            {
                let lock = GAME.lock().await;
                info!("GAME State: {:?}", *lock);
            }

            info!("Responded with: \"{}\"", resp);
        }

        data.clear();
    }
}

async fn handle_data(data: &str) -> Result<String> {
    let mut request = data.lines();

    let Some(req_type) = request.next() else {
        return Ok("INVALID COMMAND".into());
    };

    match req_type {
        _ => Ok("INVALID REQUEST TYPE".into()),
    }
}

async fn handle_init(request: &mut core::str::Lines<'_>) -> Result<String> {
    if request.next() != Some("INIT") {
        eyre::bail!("FIRST MSG SENT MUST BE INIT COMMAND");
    }

    let num_args = request.clone().count();

    if num_args < 1 {
        Ok("INVALID INIT REQUEST".into())
    } else if num_args == 1 {
        let username = request.next().unwrap();

        {
            let mut lock = GAME.lock().await;
            lock.players.push(username.into());
        }

        Ok(format!("{} ADDED", username))
    } else {
        let username = request.next().unwrap();
        let host_key = request.next().unwrap();

        if host_key != std::env::var("MONOPOLY_HOST_KEY")? {
            Ok("INVALID HOST KEY".into())
        } else {
            let mut lock = GAME.lock().await;

            if lock.host.is_none() {
                lock.players.push(username.into());
                lock.host = Some(username.into());

                Ok(format!("{} ADDED AS HOST", username))
            } else {
                Ok("HOST HAS ALREADY BEEN ADDED".into())
            }
        }
    }
}

fn main() -> Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(LevelFilter::Debug)
        .init()?;

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
