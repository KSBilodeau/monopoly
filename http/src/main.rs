#![warn(clippy::pedantic)]
#![deny(rust_2018_idioms)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

use async_std::prelude::FutureExt;
use eyre::Result;
use rand::Rng;
use tide::Request;

async fn create_game(_: Request<()>) -> tide::Result {
    let mut game_code = *b"/monopoly_socks/        ";

    loop {
        for i in 16..24 {
            game_code[i] = rand::thread_rng().gen_range(b'A'..=b'Z');
        }

        if !Path::new(OsStr::from_bytes(&game_code)).exists() {
            break;
        }
    }

    let mut host_key = String::new();
    for _ in 0..128 {
        host_key.push(rand::thread_rng().gen_range(b'A'..=b'Z') as char);
    }

    Command::new(std::env::var("MONOPOLY_GAME_BIN_PATH")?)
        .env("MONOPOLY_GAME_PATH", OsStr::from_bytes(&game_code))
        .env("MONOPOLY_HOST_KEY", host_key)
        .spawn()?;

    Ok("HELLO WORLD".into())
}

async fn test_sock(mut request: Request<()>) -> tide::Result {
    let _ =  dbg!(request.body_string().await);
    Ok(request.body_string().await?.into())
}

fn main() -> Result<()> {
    unsafe {
        libc::signal(libc::SIGCHLD, libc::SIG_IGN);
    }

    async_std::task::block_on(async move {
        let task_one = async_std::task::spawn(async move {
            let mut server = tide::new();

            server.at("/api/create_game").get(create_game);

            let ip_addr = format!("127.0.0.1:{}", std::env::var("MONOPOLY_HTTP_PORT")?);
            server.listen(ip_addr).await?;

            Ok::<(), eyre::Error>(())
        });

        let task_two = async_std::task::spawn(async move {
            let mut server = tide::new();

            server.at("/api/internal/test").post(test_sock);
            server.listen("http+unix://%2Fmonopoly_socks%2Fhost").await?;

            Ok::<(), eyre::Error>(())
        });

        match task_one.join(task_two).await {
            (Ok(_), Ok(_)) => (),
            (Err(e), Ok(_)) => panic!("{}", e),
            (Ok(_), Err(e)) => panic!("{}", e),
            (Err(e1), Err(e2)) => panic!("{}\n{}", e1, e2),
        }

        Ok(())
    })
}
