use eyre::Result;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};
use hyper_tungstenite::HyperWebsocket;
use hyper_util::rt::TokioIo;
use tungstenite::Message;

async fn handle_connection(mut request: Request<Incoming>) -> Result<Response<Full<Bytes>>> {
    if hyper_tungstenite::is_upgrade_request(&request) {
        let (resp, websocket) = hyper_tungstenite::upgrade(&mut request, None)?;

        tokio::spawn(async move {
            if let Err(e) = handle_websocket(websocket).await {
                eprintln!("Error in websocket connection: {}", e);
            }
        });

        Ok(resp)
    } else {
        handle_http_request(request).await
    }
}

async fn handle_websocket(websocket: HyperWebsocket) -> Result<()> {
    let mut websocket = websocket.await?;

    while let Some(msg) = websocket.next().await {
        match msg? {
            Message::Text(msg) => {
                println!("Received text message: {}", msg);
                websocket.send(Message::Text(msg)).await?;
            }
            _ => unreachable!(),
        }
    }

    Ok(())
}

async fn handle_http_request(_: Request<Incoming>) -> Result<Response<Full<Bytes>>> {
    Ok(Response::new(Full::<Bytes>::from("Hello HTTP!")))
}

#[tokio::main]
async fn main() -> Result<()> {
    let port_str = std::env::var("MONOPOLY_SERVER_PORT")?;

    let server = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port_str)).await?;
    println!("Listening on port {}", port_str);

    let mut http = hyper::server::conn::http1::Builder::new();
    http.keep_alive(true);

    loop {
        let (stream, _) = server.accept().await?;

        let connection = http.serve_connection(
            TokioIo::new(stream),
            hyper::service::service_fn(handle_connection),
        );

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Error serving http connection: {:?}", e);
            }
        });
    }
}
