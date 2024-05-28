use crate::game::Session;
use async_std::io::{ReadExt, WriteExt};
use async_std::os::unix::net::UnixStream;
use eyre::{OptionExt, Result};
use log::{error, info};
use soketto::Sender;

#[derive(Eq, PartialEq)]
enum CommandState {
    ExpectInit,
    Initialized,
    Kill,
}

pub struct CommandHandler {
    state: CommandState,
}

impl CommandHandler {
    pub fn new() -> Self {
        CommandHandler {
            state: CommandState::ExpectInit,
        }
    }

    pub async fn execute_command(
        &mut self,
        ws_id: u32,
        data: &str,
        game: &mut Session,
        sender: &mut Sender<UnixStream>,
    ) {
        let command = Command::new(data);

        if ((self.state == CommandState::ExpectInit) != command.is_init()) && !command.is_error() {
            Command::error(command.nonce().unwrap(), "8".into())
                .execute(game)
                .await
                .respond(sender)
                .await;

            return;
        }

        let command = command.execute(game).await.respond(sender).await;

        match command {
            Command::Init(init) => {
                info!(
                    "Command {} completed successfully: {:#?}",
                    init.nonce(),
                    init
                );

                self.state = CommandState::Initialized;
            }
            Command::Echo(echo) => info!(
                "Command {} completed successfully {:#?}",
                echo.nonce(),
                echo
            ),
            Command::Error(error) => error!(
                "Command {} completed with error code: {}",
                error.nonce(),
                error.code()
            ),
            Command::Kill => {
                info!("Killing connection for WS with ID: {}", ws_id);

                let _ = sender.close();
                self.state = CommandState::Kill;
            }
        }
    }

    pub fn is_kill(&self) -> bool {
        self.state == CommandState::Kill
    }
}

#[derive(Debug)]
enum Command {
    Init(Init),
    Echo(Echo),
    Error(Error),
    Kill,
}

impl Command {
    fn new(request: &str) -> Self {
        let mut request = request.lines();

        let Some(nonce) = request.next().map(str::to_string) else {
            return Command::Kill;
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

    async fn execute(self, game: &mut Session) -> Self {
        match self {
            Self::Init(init) => init.execute(game),
            Self::Echo(echo) => echo.execute().await,
            _ => self,
        }
    }

    async fn respond(self, sender: &mut Sender<UnixStream>) -> Self {
        match self {
            Self::Init(init) => init.respond(sender).await,
            Self::Echo(echo) => echo.respond(sender).await,
            Self::Error(error) => error.respond(sender).await,
            _ => self,
        }
    }

    fn nonce(&self) -> Option<String> {
        match self {
            Command::Init(init) => Some(init.nonce.clone()),
            Command::Echo(echo) => Some(echo.nonce.clone()),
            _ => None,
        }
    }

    fn is_init(&self) -> bool {
        if let Command::Init(_) = self {
            return true;
        }

        false
    }

    fn is_error(&self) -> bool {
        if let Command::Error(_) = self {
            return true;
        }

        false
    }

    fn init(nonce: String, request: &mut std::str::Lines<'_>) -> Self {
        match Init::new(nonce.clone(), request) {
            Ok(init) => Self::Init(init),
            Err(err) => Self::error(nonce, err.to_string()),
        }
    }

    fn echo(nonce: String, request: &mut std::str::Lines<'_>) -> Self {
        match Echo::new(nonce.clone(), request) {
            Ok(echo) => Self::Echo(echo),
            Err(err) => Self::error(nonce, err.to_string()),
        }
    }

    fn error(nonce: String, code: String) -> Self {
        Self::Error(Error::new(nonce, code))
    }
}

#[derive(Debug, Default)]
pub struct Init {
    nonce: String,
    username: String,
    host_key: Option<String>,
    players: Vec<String>,
}

impl Init {
    pub fn new(nonce: String, request: &mut std::str::Lines<'_>) -> Result<Init> {
        Ok(Init {
            nonce,
            username: request.next().map(str::to_string).ok_or_eyre("1")?,
            host_key: request.next().map(str::to_string),
            players: vec![],
        })
    }

    pub fn nonce(&self) -> &String {
        &self.nonce
    }

    fn execute(mut self, game: &mut Session) -> Command {
        let command = match game.add_player(&self.username, &self.host_key) {
            Ok(_) => Command::Init({
                self.players = game.players().clone();

                self
            }),
            Err(err) => Command::error(self.nonce, err.to_string()),
        };

        command
    }

    async fn respond(self, sender: &mut Sender<UnixStream>) -> Command {
        sender
            .send_text(format!("{}\nSUCCESS", self.nonce))
            .await
            .unwrap();

        Command::Init(self)
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
            .write_all(dbg!(format!(
                "GET /api/internal/test HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
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

        Command::Echo(self)
    }

    async fn respond(self, sender: &mut Sender<UnixStream>) -> Command {
        sender.send_text(&self.resp).await.unwrap();

        Command::Echo(self)
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

        Command::Error(self)
    }
}