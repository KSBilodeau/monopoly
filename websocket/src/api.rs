use crate::game::Session;
use async_std::io::{ReadExt, WriteExt};
use async_std::os::unix::net::UnixStream;
use eyre::{OptionExt, Result};
use log::info;
use soketto::Sender;

#[derive(Debug)]
pub enum Command {
    INIT(Init),
    ECHO(Echo),
    ERROR(Error),
    KILL,
}

impl Command {
    pub fn new(request: &str) -> Self {
        let mut request = request.lines();

        let Some(nonce) = request.next().map(str::to_string) else {
            return Command::KILL;
        };

        let Some(command) = request.next() else {
            return Self::error(nonce, "0".into());
        };

        match command {
            "INIT" => Self::init(nonce, &mut request),
            "ECHO" => Self::echo(nonce, &mut request),
            _ => return Self::error(nonce, "0".into()),
        }
    }

    pub async fn execute(self, game: &mut Session) -> Self {
        match self {
            Self::INIT(init) => init.execute(game),
            Self::ECHO(echo)  => echo.execute().await,
            _ => self,
        }
    }

    pub async fn respond(self, sender: &mut Sender<UnixStream>) -> Command {
        match self {
            Self::INIT(init) => init.respond(sender).await,
            Self::ECHO(echo) => echo.respond(sender).await,
            Self::ERROR(error) => error.respond(sender).await,
            _ => self,
        }
    }

    fn init(nonce: String, request: &mut std::str::Lines<'_>) -> Self {
        match Init::new(nonce.clone(), request) {
            Ok(init) => Self::INIT(init),
            Err(err) => Self::error(nonce, err.to_string()),
        }
    }

    fn echo(nonce: String, request: &mut std::str::Lines<'_>) -> Self {
        match Echo::new(nonce.clone(), request) {
            Ok(echo) => Self::ECHO(echo),
            Err(err) => Self::error(nonce, err.to_string()),
        }
    }

    fn error(nonce: String, code: String) -> Self {
        Self::ERROR(Error::new(nonce, code))
    }
}

#[derive(Debug, Default)]
pub struct Init {
    nonce: String,
    username: String,
    host_key: Option<String>,
}

impl Init {
    pub fn new(nonce: String, request: &mut std::str::Lines<'_>) -> Result<Init> {
        Ok(Init {
            nonce,
            username: request.next().map(str::to_string).ok_or_eyre("1")?,
            host_key: request.next().map(str::to_string),
        })
    }

    pub fn nonce(&self) -> &String {
        &self.nonce
    }

    fn execute(self, game: &mut Session) -> Command {
        match game.add_player(&self.username, &self.host_key) {
            Ok(_) => Command::INIT(self),
            Err(err) => Command::error(self.nonce, err.to_string()),
        }
    }

    async fn respond(self, sender: &mut Sender<UnixStream>) -> Command {
        sender
            .send_text(format!("{}\nSUCCESS", self.nonce))
            .await
            .unwrap();

        Command::INIT(self)
    }
}

#[derive(Debug, Default)]
pub struct Echo {
    nonce: String,
    msg: String,
    resp: String,
}

impl Echo {
    pub fn new(nonce: String, request: &mut std::str::Lines<'_>) -> Result<Echo> {
        Ok(Echo {
            nonce,
            msg: request.next().map(str::to_string).ok_or_eyre("3")?,
            resp: String::new(),
        })
    }

    pub fn nonce(&self) -> &String {
        &self.nonce
    }

    async fn execute(mut self) -> Command {
        let Ok(mut stream) = UnixStream::connect("/monopoly_socks/host").await else {
            return Command::error(self.nonce, "4".into());
        };

        let Ok(_) = stream
            .write_all(dbg!(format!("{}{}{}{}{}{}\r\n\r\n{}",
                "GET /api/internal/test HTTP/1.1",
                "\r\nHost: 127.0.0.1",
                "\r\nConnection: close",
                "\r\nContent-Type: text/plain",
                "\r\nContent-Length: ",
                self.msg.len(),
                self.msg
            )).as_bytes())
            .await
        else {
            return Command::error(self.nonce, "5".into());
        };

        let Ok(_) = stream.read_to_string(&mut self.resp).await else {
            return Command::error(self.nonce, "6".into());
        };

        info!("RESULT OF ECHO OPERATION:\n{}", self.resp);

        Command::ECHO(self)
    }

    async fn respond(self, sender: &mut Sender<UnixStream>) -> Command {
        sender.send_text(&self.resp).await.unwrap();

        Command::ECHO(self)
    }
}

#[derive(Debug, Default)]
pub struct Error {
    nonce: String,
    code: String,
}

impl Error {
    pub fn new(nonce: String, code: String) -> Error {
        Error { nonce, code }
    }

    pub fn nonce(&self) -> &String {
        &self.nonce
    }

    pub fn code(&self) -> &String {
        &self.code
    }

    async fn respond(self, sender: &mut Sender<UnixStream>) -> Command {
        sender
            .send_text(format!("-{}\n{}", self.nonce, self.code))
            .await
            .unwrap();

        Command::ERROR(self)
    }
}
