use std::any::Any;
use std::fmt::Debug;
use std::sync::{mpsc, Arc};

use async_std::io::{ReadExt, WriteExt};
use async_std::os::unix::net::UnixStream;
use log::{error, info};
use parking_lot::Mutex;
use soketto::{Receiver, Sender};

use crate::api::back::{Event, Message};
use crate::game::Session;
use crate::util;

#[derive(Eq, PartialEq)]
enum CommandState {
    AwaitingInit,
    Running,
    Killed,
}

pub struct CommandHandler {
    ws_id: u32,
    player_id: usize,
    state: CommandState,
    send: mpsc::Sender<Event>,
    game: Arc<Mutex<Session>>,
    data: Vec<u8>,
}

impl CommandHandler {
    pub fn new(ws_id: u32, send: mpsc::Sender<Event>, game: Arc<Mutex<Session>>) -> Self {
        CommandHandler {
            ws_id,
            player_id: 0,
            state: CommandState::AwaitingInit,
            send,
            game,
            data: vec![],
        }
    }

    pub fn pump_command(&mut self, recv: Arc<Mutex<Receiver<UnixStream>>>) -> Option<String> {
        let Ok(data_type) = util::sync!(recv.lock().receive_data(&mut self.data)) else {
            error!("Receiver closed prematurely on WS (#{})", self.ws_id);
            self.state = CommandState::Killed;
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

    pub fn execute_command(
        &mut self,
        data: &str,
        send: Arc<Mutex<Sender<UnixStream>>>,
    ) -> Box<dyn CommandExt> {
        let command = Command::new(data, self.player_id);

        if ((self.state == CommandState::AwaitingInit) != command.is_init()) && !command.is_error()
        {
            return Error::new(&command.nonce(), "8".into())
                .execute(self.game.clone())
                .respond(send);
        }

        let command = command.execute(self.game.clone()).respond(send);

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
                self.player_id = command.as_any().downcast_ref::<Init>().unwrap().player_id;
                self.state = CommandState::Running;
            } else if command.is_chat() {
                let chat = command.as_any().downcast_ref::<Chat>().unwrap();

                self.send
                    .send(Event::Msg(Message::new(
                        &self
                            .game
                            .lock()
                            .player_username_by_id(chat.player_id)
                            .unwrap(),
                        &chat.msg,
                    )))
                    .unwrap();
            }
        }

        command
    }

    pub fn is_kill(&self) -> bool {
        self.state == CommandState::Killed
    }
}

pub trait CommandExt: Debug {
    fn execute(self: Box<Self>, game: Arc<Mutex<Session>>) -> Box<dyn CommandExt>;

    fn respond(self: Box<Self>, sender: Arc<Mutex<Sender<UnixStream>>>) -> Box<dyn CommandExt>;

    fn nonce(&self) -> String;

    fn is_init(&self) -> bool {
        false
    }

    fn is_chat(&self) -> bool {
        false
    }

    fn is_error(&self) -> bool {
        false
    }

    fn error_code(&self) -> Option<String> {
        None
    }

    fn as_any(&self) -> &dyn Any;
}

struct Command {}

impl Command {
    fn new(data: &str, player_id: usize) -> Box<dyn CommandExt> {
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
            "CHAT" => Chat::new(&nonce, &mut request, player_id),
            _ => Error::new(&nonce, "0".into()),
        }
    }
}

#[derive(Debug, Default)]
struct Init {
    nonce: String,
    username: String,
    host_key: Option<String>,
    player_id: usize,
    game: Option<Arc<Mutex<Session>>>,
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
            player_id: 0,
            game: None,
        })
    }
}

impl CommandExt for Init {
    fn execute(mut self: Box<Init>, game: Arc<Mutex<Session>>) -> Box<dyn CommandExt> {
        self.game = Some(game.clone());
        let mut game = game.lock();

        match game.add_player(&self.username, &self.host_key) {
            Ok(id) => {
                self.player_id = id;

                self as Box<dyn CommandExt>
            }
            Err(err) => Error::new(&self.nonce, err.to_string()),
        }
    }

    fn respond(self: Box<Self>, sender: Arc<Mutex<Sender<UnixStream>>>) -> Box<dyn CommandExt> {
        let binding = self.game.as_ref().unwrap().clone();
        let mut game = binding.lock();
        game.assoc_sock(self.player_id, sender.clone());

        util::sync!(sender.lock().send_text(format!("{}\nSUCCESS", self.nonce))).unwrap();

        self
    }

    fn nonce(&self) -> String {
        self.nonce.clone()
    }

    fn is_init(&self) -> bool {
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Default)]
struct Echo {
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
    fn execute(mut self: Box<Echo>, _: Arc<Mutex<Session>>) -> Box<dyn CommandExt> {
        let mut stream = match util::sync!(UnixStream::connect("/monopoly_socks/host")) {
            Ok(stream) => stream,
            Err(_) => return Error::new(&self.nonce, "4".into()),
        };

        let request = format!(
            "GET /api/internal/test HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
            self.msg.len(),
            self.msg
        );

        let Ok(_) = util::sync!(stream.write_all(request.as_bytes())) else {
            return Error::new(&self.nonce, "5".into());
        };

        let Ok(_) = util::sync!(stream.read_to_string(&mut self.resp)) else {
            return Error::new(&self.nonce, "6".into());
        };

        info!("RESULT OF ECHO OPERATION:\n{}", self.resp);

        self
    }

    fn respond(self: Box<Self>, sender: Arc<Mutex<Sender<UnixStream>>>) -> Box<dyn CommandExt> {
        util::sync!(sender.lock().send_text(&self.resp)).unwrap();

        self
    }

    fn nonce(&self) -> String {
        self.nonce.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Default)]
struct Chat {
    nonce: String,
    msg: String,
    player_id: usize,
}

impl Chat {
    pub fn new(
        nonce: &String,
        request: &mut std::str::Lines<'_>,
        player_id: usize,
    ) -> Box<dyn CommandExt> {
        Box::new(Chat {
            nonce: nonce.clone(),
            msg: {
                let msg = request.collect::<String>();

                if msg.is_empty() {
                    return Error::new(&nonce.clone(), "10".into());
                };

                if msg.len() > 12 {
                    return Error::new(&nonce.clone(), "9".into());
                }

                msg.to_string()
            },
            player_id,
        })
    }
}

impl CommandExt for Chat {
    fn execute(self: Box<Self>, game: Arc<Mutex<Session>>) -> Box<dyn CommandExt> {
        let mut game = game.lock();
        let username = game.player_username_by_id(self.player_id).unwrap();

        game.add_message(&username, &self.msg);

        self
    }

    fn respond(self: Box<Self>, sender: Arc<Mutex<Sender<UnixStream>>>) -> Box<dyn CommandExt> {
        util::sync!(sender.lock().send_text(format!("{}", self.nonce))).unwrap();

        self
    }

    fn nonce(&self) -> String {
        self.nonce.clone()
    }

    fn is_chat(&self) -> bool {
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Default)]
struct Error {
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
    fn execute(self: Box<Self>, _: Arc<Mutex<Session>>) -> Box<dyn CommandExt> {
        self
    }

    fn respond(self: Box<Self>, sender: Arc<Mutex<Sender<UnixStream>>>) -> Box<dyn CommandExt> {
        util::sync!(sender
            .lock()
            .send_text(format!("-{}\n{}", self.nonce, self.code)))
        .unwrap();

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

    fn as_any(&self) -> &dyn Any {
        self
    }
}
