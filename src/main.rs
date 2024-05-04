use async_std::net;
use eyre::Result;

async fn _serve_websocket() -> Result<()> {
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
    let ip_addr = format!("127.0.0.1:{}", std::env::var("MONOPOLY_SERVER_PORT")?);

    let listener = std::net::TcpListener::bind(&ip_addr)?;
    println!("Listening on {}", &ip_addr);

    let server = listener.try_clone()?;

    async_std::task::spawn(async move {
       for _ in dbg!(server.incoming()) {
       }
    });

    let async_server = net::TcpListener::from(listener.try_clone()?);

    async_std::task::spawn(async move {
        println!("TASK HAS STARTED AND IS WAITING");
        while let Ok((stream, _)) = dbg!(async_server.accept().await) {
            let peer = stream.peer_addr().unwrap();
            println!("Peer address: {}", peer);
        }
    });

    let mut http_client = tide::new();

    http_client.at("/api/test").post(test);

    http_client.listen(listener.try_clone()?).await?;
    Ok(())
}