use async_std::net;
use async_std::prelude::FutureExt;
use async_tungstenite::accept_async;
use eyre::Result;
use futures::{SinkExt, StreamExt};

async fn serve_websocket(peer: net::SocketAddr, stream: net::TcpStream) -> Result<()> {
    let mut ws_stream = accept_async(stream).await?;
    println!("New WS connection at {}", peer);

    while let Some(msg) = ws_stream.next().await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => return Err(e.into()),
        };

        if msg.is_text() && msg.is_binary() {
            ws_stream.send(msg).await?;
        }
    }

    Ok(())
}

async fn _serve_request() -> Result<()> {
    Ok(())
}

async fn test(_: tide::Request<()>) -> tide::Result {
    Ok("Hello, World!".into())
}

#[async_std::main]
async fn main() -> Result<()> {
    let ws_addr = format!("127.0.0.1:{}", std::env::var("MONOPOLY_WS_PORT")?);
    let http_addr = format!("127.0.0.1:{}", std::env::var("MONOPOLY_HTTP_PORT")?);

    let websocket = async_std::task::spawn(async move {
        let Ok(ws_server) = net::TcpListener::bind(ws_addr.clone()).await else {
            panic!("Websocket server failed to establish a connection!");
        };
        println!("Listening on ws port {}", ws_addr);

        while let Ok((stream, _)) = dbg!(ws_server.accept().await) {
            let peer = stream.peer_addr().unwrap();
            println!("Peer address: {}", peer);

            async_std::task::spawn(async move {
                if let Err(e) = serve_websocket(peer, stream).await {
                    eprintln!("Websocket connection failed: {}", e);
                }
            });
        }
    });

    let http = async_std::task::spawn(async move {
        let mut http_client = tide::new();

        http_client.at("/api/test").get(test);

        println!("Listening on http port {}", http_addr);
        let Ok(_) = http_client.listen(http_addr.clone()).await else {
            panic!("HTTP server failed to establish a connection!");
        };
    });

    websocket.join(http).await;

    Ok(())
}