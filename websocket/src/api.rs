use crate::game::Session;
use async_std::io::{ReadExt, WriteExt};
use async_std::os::unix::net::UnixStream;
use log::{error, info};
use soketto::Sender;
use std::fmt::Debug;

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
            Command::from(Error::new(&command.nonce(), "8".into()))
                .execute(game)
                .respond(sender);

            return;
        }

        let command: Box<dyn CommandExt> = command.execute(game).respond(sender).into();

        if command.is_error() {
            error!(
                "Command {} completed with error code: {}",
                command.nonce(), command.error_code().unwrap(),
            );
        } else if command.is_kill() {
            info!("Killing connection for WS with ID: {}", ws_id);

            let _ = sender.close();
            self.state = CommandState::Kill;
        } else {
            info!(
                "Command {} completed successfully {:#?}",
                command.nonce(),
                command
            );

            if command.is_init() {
                self.state = CommandState::Initialized;
            }
        }
    }

    pub fn is_kill(&self) -> bool {
        self.state == CommandState::Kill
    }
}

trait CommandExt: Debug {
    fn execute(self: Box<Self>, game: &mut Session) -> Command;

    fn respond(self: Box<Self>, sender: &mut Sender<UnixStream>) -> Command;

    fn nonce(&self) -> &String;

    fn is_init(&self) -> bool {
        false
    }

    fn is_error(&self) -> bool {
        false
    }

    fn error_code(&self) -> Option<&String> {
        None
    }

    fn is_kill(&self) -> bool {
        false
    }
}

#[derive(Debug)]
enum Command {
    Init(Init),
    Echo(Echo),
    Error(Error),
    Kill(Kill),
}

impl Command {
    fn new(data: &str) -> Command {
        let mut request = data.lines();

        let Some(nonce) = request.next().map(str::to_string) else {
            return Kill::new();
        };

        let Some(command) = request.next() else {
            return Error::new(&nonce, "0".into()).into();
        };

        match command {
            "INIT" => Init::new(&nonce, &mut request),
            "ECHO" => Echo::new(&nonce, &mut request),
            _ => Error::new(&nonce, "0".into()).into(),
        }
    }

    fn execute(self, game: &mut Session) -> Self {
        Into::<Box<dyn CommandExt>>::into(self).execute(game)
    }

    fn respond(self, sender: &mut Sender<UnixStream>) -> Self {
        Into::<Box<dyn CommandExt>>::into(self).respond(sender)
    }

    fn nonce(&self) -> &String {
        Into::<Box<&dyn CommandExt>>::into(self).nonce()
    }

    fn is_init(&self) -> bool {
        Into::<Box<&dyn CommandExt>>::into(self).is_init()
    }

    fn is_error(&self) -> bool {
        Into::<Box<&dyn CommandExt>>::into(self).is_error()
    }
}

impl From<Command> for Box<dyn CommandExt> {
    fn from(command: Command) -> Self {
        match command {
            Command::Init(init) => Box::new(init),
            Command::Echo(echo) => Box::new(echo),
            Command::Error(error) => Box::new(error),
            Command::Kill(kill) => Box::new(kill),
        }
    }
}

impl<'a> From<&'a Command> for Box<&'a dyn CommandExt> {
    fn from(command: &'a Command) -> Self {
        match command {
            Command::Init(init) => Box::new(*&init),
            Command::Echo(echo) => Box::new(*&echo),
            Command::Error(error) => Box::new(*&error),
            Command::Kill(kill) => Box::new(*&kill),
        }
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
    fn new(nonce: &String, request: &mut std::str::Lines<'_>) -> Command {
        Init {
            nonce: nonce.clone(),
            username: {
                let Some(username) = request.next().map(str::to_string) else {
                    return Error::new(&nonce.clone(), "1".into()).into();
                };

                username
            },
            host_key: request.next().map(str::to_string),
            players: vec![],
        }
        .into()
    }
}

impl CommandExt for Init {
    fn execute(mut self: Box<Init>, game: &mut Session) -> Command {
        let command = match game.add_player(&self.username, &self.host_key) {
            Ok(_) => Command::Init({
                self.players = game.players().clone();

                *self
            }),
            Err(err) => Error::new(&self.nonce, err.to_string()).into(),
        };

        command
    }

    fn respond(self: Box<Init>, sender: &mut Sender<UnixStream>) -> Command {
        let future = sender.send_text(format!("{}\nSUCCESS", self.nonce));
        async_std::task::block_on(async move { future.await.unwrap() });

        Command::Init(*self)
    }

    fn nonce(&self) -> &String {
        &self.nonce
    }

    fn is_init(&self) -> bool {
        true
    }
}

impl From<Init> for Command {
    fn from(init: Init) -> Self {
        Command::Init(init)
    }
}

#[derive(Debug, Default)]
pub struct Echo {
    nonce: String,
    msg: String,
    resp: String,
}

impl Echo {
    fn new(nonce: &String, request: &mut std::str::Lines<'_>) -> Command {
        Echo {
            nonce: nonce.clone(),
            msg: {
                let Some(msg) = request.next().map(str::to_string) else {
                    return Error::new(&nonce.clone(), "3".into()).into();
                };

                msg
            },
            resp: String::new(),
        }
        .into()
    }
}

impl CommandExt for Echo {
    fn execute(mut self: Box<Echo>, _: &mut Session) -> Command {
        let future = UnixStream::connect("/monopoly_socks/host");
        let mut stream = match async_std::task::block_on(async move { future.await }) {
            Ok(stream) => stream,
            Err(_) => return Error::new(&self.nonce, "4".into()).into(),
        };

        let request = format!(
            "GET /api/internal/test HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
            self.msg.len(),
            self.msg
        );

        let future = stream.write_all(request.as_bytes());
        let Ok(_) = async_std::task::block_on(async move { future.await }) else {
            return Error::new(&self.nonce, "5".into()).into();
        };

        let future = stream.read_to_string(&mut self.resp);
        let Ok(_) = async_std::task::block_on(async move { future.await }) else {
            return Error::new(&self.nonce, "6".into()).into();
        };

        info!("RESULT OF ECHO OPERATION:\n{}", self.resp);

        Command::Echo(*self)
    }

    fn respond(self: Box<Echo>, sender: &mut Sender<UnixStream>) -> Command {
        let future = sender.send_text(&self.resp);
        async_std::task::block_on(async move { future.await.unwrap() });

        Command::Echo(*self)
    }

    fn nonce(&self) -> &String {
        &self.nonce
    }
}

impl From<Echo> for Command {
    fn from(echo: Echo) -> Self {
        Command::Echo(echo)
    }
}

#[derive(Debug, Default)]
pub struct Error {
    nonce: String,
    code: String,
}

impl Error {
    pub fn new(nonce: &String, code: String) -> Error {
        Error {
            nonce: nonce.clone(),
            code,
        }
    }
}

impl CommandExt for Error {
    fn execute(self: Box<Error>, _: &mut Session) -> Command {
        Command::Error(*self)
    }

    fn respond(self: Box<Error>, sender: &mut Sender<UnixStream>) -> Command {
        let future = sender.send_text(format!("-{}\n{}", self.nonce, self.code));

        async_std::task::block_on(async move { future.await.unwrap() });

        Command::Error(*self)
    }

    fn nonce(&self) -> &String {
        &self.nonce
    }

    fn is_error(&self) -> bool {
        true
    }

    fn error_code(&self) -> Option<&String> {
        Some(&self.code)
    }
}

impl From<Error> for Command {
    fn from(err: Error) -> Self {
        Command::Error(err)
    }
}

#[derive(Debug)]
struct Kill {
    nonce: String,
}

impl Kill {
    fn new() -> Command {
        Command::Kill(Kill {
            nonce: String::from("0"),
        })
    }
}

impl CommandExt for Kill {
    fn execute(self: Box<Self>, _: &mut Session) -> Command {
        Command::Kill(*self)
    }

    fn respond(self: Box<Self>, sender: &mut Sender<UnixStream>) -> Command {
        let future = sender.send_text(format!("-{}\nWS KILLED FOR UNSPECIFIED REASON", self.nonce));
        async_std::task::block_on(async move { future.await.unwrap() });

        Command::Kill(*self)
    }

    fn nonce(&self) -> &String {
        &self.nonce
    }

    fn is_kill(&self) -> bool {
        true
    }
}

impl From<Kill> for Command {
    fn from(kill: Kill) -> Self {
        Command::Kill(kill)
    }
}
