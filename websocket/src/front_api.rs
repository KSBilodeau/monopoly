use std::fmt::Debug;
use std::sync::Arc;

use async_std::io::{ReadExt, WriteExt};
use async_std::os::unix::net::UnixStream;
use log::{error, info};
use parking_lot::Mutex;
use soketto::{Receiver, Sender};

use crate::back_api::Event;
use crate::game::{Player, Session};

#[derive(Eq, PartialEq)]
enum CommandState {
    ExpectInit,
    Initialized,
    Kill,
}

pub struct CommandHandler {
    ws_id: u32,
    state: CommandState,
    send: Arc<Mutex<Sender<UnixStream>>>,
    recv: Arc<Mutex<Receiver<UnixStream>>>,
    game: Arc<Mutex<Session>>,
    data: Vec<u8>,
    events: std::sync::mpsc::Sender<Event>,
}

impl CommandHandler {
    pub fn new(
        ws_id: u32,
        send: Arc<Mutex<Sender<UnixStream>>>,
        recv: Arc<Mutex<Receiver<UnixStream>>>,
        game: Arc<Mutex<Session>>,
        events: std::sync::mpsc::Sender<Event>,
    ) -> Self {
        CommandHandler {
            ws_id,
            state: CommandState::ExpectInit,
            send,
            recv,
            game,
            data: vec![],
            events
        }
    }

    pub fn pump_command<'a>(&mut self) -> Option<String> {
        let Ok(data_type) = crate::sync!(self.recv.lock().receive_data(&mut self.data)) else {
            error!("Receiver closed prematurely on WS (#{})", self.ws_id);
            self.state = CommandState::Kill;
            return None;
        };

        if data_type.is_text() {
            let Ok(data) = std::str::from_utf8(&self.data).map(str::to_string) else {
                error!("Received invalid UTF-8 bytes on WS (#{})", self.ws_id);
                return None;
            };

            Some(data)
        } else {
            None
        }
    }

    pub fn execute_command(&mut self, data: &str) {
        let command = Command::new(data);

        if ((self.state == CommandState::ExpectInit) != command.is_init()) && !command.is_error() {
            Error::new(&command.nonce(), "8".into())
                .execute(&mut *self.game.lock())
                .respond(&mut *self.send.lock());

            return;
        }

        let command = command
            .execute(&mut *self.game.lock())
            .respond(&mut *self.send.lock());

        if command.is_error() {
            error!(
                "Command {} completed with error code: {}",
                command.nonce(),
                command.error_code().unwrap(),
            );
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
    fn execute(self: Box<Self>, game: &mut Session) -> Box<dyn CommandExt>;

    fn respond(self: Box<Self>, sender: &mut Sender<UnixStream>) -> Box<dyn CommandExt>;

    fn nonce(&self) -> String;

    fn is_init(&self) -> bool {
        false
    }

    fn is_error(&self) -> bool {
        false
    }

    fn error_code(&self) -> Option<String> {
        None
    }
}

struct Command {}

impl Command {
    fn new(data: &str) -> Box<dyn CommandExt> {
        let mut request = data.lines();

        let Some(nonce) = request.next().map(str::to_string) else {
            return Error::new(&String::from("0"), "0".into());
        };

        let Some(command) = request.next() else {
            return Error::new(&nonce, "0".into());
        };

        match command {
            "INIT" => Init::new(&nonce, &mut request),
            "ECHO" => Echo::new(&nonce, &mut request),
            _ => Error::new(&nonce, "0".into()),
        }
    }
}

#[derive(Debug, Default)]
pub struct Init {
    nonce: String,
    username: String,
    host_key: Option<String>,
    players: Vec<Player>,
}

impl Init {
    fn new(nonce: &String, request: &mut std::str::Lines<'_>) -> Box<dyn CommandExt> {
        Box::new(Init {
            nonce: nonce.clone(),
            username: {
                let Some(username) = request.next().map(str::to_string) else {
                    return Error::new(&nonce, "1".into());
                };

                username
            },
            host_key: request.next().map(str::to_string),
            players: vec![],
        })
    }
}

impl CommandExt for Init {
    fn execute(mut self: Box<Init>, game: &mut Session) -> Box<dyn CommandExt> {
        match game.add_player(&self.username, &self.host_key) {
            Ok(_) => {
                self.players = game.players().clone();

                self as Box<dyn CommandExt>
            }
            Err(err) => Error::new(&self.nonce, err.to_string()),
        }
    }

    fn respond(self: Box<Self>, sender: &mut Sender<UnixStream>) -> Box<dyn CommandExt> {
        crate::sync!(sender.send_text(format!("{}\nSUCCESS", self.nonce))).unwrap();

        self
    }

    fn nonce(&self) -> String {
        self.nonce.clone()
    }

    fn is_init(&self) -> bool {
        true
    }
}

#[derive(Debug, Default)]
pub struct Echo {
    nonce: String,
    msg: String,
    resp: String,
}

impl Echo {
    fn new(nonce: &String, request: &mut std::str::Lines<'_>) -> Box<dyn CommandExt> {
        Box::new(Echo {
            nonce: nonce.clone(),
            msg: {
                let Some(msg) = request.next().map(str::to_string) else {
                    return Error::new(&nonce.clone(), "3".into());
                };

                msg
            },
            resp: String::new(),
        })
    }
}

impl CommandExt for Echo {
    fn execute(mut self: Box<Echo>, _: &mut Session) -> Box<dyn CommandExt> {
        let mut stream = match crate::sync!(UnixStream::connect("/monopoly_socks/host")) {
            Ok(stream) => stream,
            Err(_) => return Error::new(&self.nonce, "4".into()),
        };

        let request = format!(
            "GET /api/internal/test HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
            self.msg.len(),
            self.msg
        );

        let Ok(_) = crate::sync!(stream.write_all(request.as_bytes())) else {
            return Error::new(&self.nonce, "5".into());
        };

        let Ok(_) = crate::sync!(stream.read_to_string(&mut self.resp)) else {
            return Error::new(&self.nonce, "6".into());
        };

        info!("RESULT OF ECHO OPERATION:\n{}", self.resp);

        self
    }

    fn respond(self: Box<Self>, sender: &mut Sender<UnixStream>) -> Box<dyn CommandExt> {
        crate::sync!(sender.send_text(&self.resp)).unwrap();

        self
    }

    fn nonce(&self) -> String {
        self.nonce.clone()
    }
}

#[derive(Debug, Default)]
pub struct Error {
    nonce: String,
    code: String,
}

impl Error {
    fn new(nonce: &String, code: String) -> Box<dyn CommandExt> {
        Box::new(Error {
            nonce: nonce.clone(),
            code,
        })
    }
}

impl CommandExt for Error {
    fn execute(self: Box<Self>, _: &mut Session) -> Box<dyn CommandExt> {
        self
    }

    fn respond(self: Box<Self>, sender: &mut Sender<UnixStream>) -> Box<dyn CommandExt> {
        crate::sync!(sender.send_text(format!("-{}\n{}", self.nonce, self.code))).unwrap();

        self
    }

    fn nonce(&self) -> String {
        self.nonce.clone()
    }

    fn is_error(&self) -> bool {
        true
    }

    fn error_code(&self) -> Option<String> {
        Some(self.code.clone())
    }
}
