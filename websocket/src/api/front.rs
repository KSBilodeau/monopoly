use std::any::Any;
use std::fmt::Debug;

use async_std::io::{ReadExt, WriteExt};
use async_std::os::unix::net::UnixStream;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use eyre::Result;
use soketto::{Receiver, Sender};

type SafeSender<T> = Arc<Mutex<Sender<T>>>;

macro_rules! unlock_mut {
    ($lock: expr) => {
        &mut *$lock.lock().await
    };
}

#[derive(Debug, Eq, PartialEq)]
enum State {
    Uninit,
    _Init,
    Killed,
}

pub struct CommandHandler {
    ws_id: u32,
    player_id: Option<usize>,
    state: State,
    ws_send: SafeSender<UnixStream>,
    ws_recv: Receiver<UnixStream>,
}

impl CommandHandler {
    pub fn new(ws_id: u32, send: SafeSender<UnixStream>, recv: Receiver<UnixStream>) -> Self {
        Self {
            ws_id,
            player_id: None,
            state: State::Uninit,
            ws_send: send,
            ws_recv: recv,
        }
    }

    pub async fn pump(&mut self) -> Option<String> {
        let mut data = vec![];

        let Ok(data_type) = self.ws_recv.receive_data(&mut data).await else {
            self.state = State::Killed;
            return None;
        };

        if data_type.is_text() {
            let Ok(data) = std::str::from_utf8(&data).map(str::to_string) else {
                log::error!("Received invalid UTF-8 bytes on WS (#{})", self.ws_id);
                return None;
            };

            Some(data)
        } else {
            None
        }
    }

    pub async fn execute(&mut self, data: &str) {
        let command = Command::new(data, self.player_id);

        log::info!("PROCESSING: {:#?}", command);

        let Ok(command) = command.execute(unlock_mut!(self.ws_send)).await else {
            log::error!("Sender/Receiver closed prematurely");
            self.state = State::Killed;
            return;
        };

        if let Some(command) = command.as_any().downcast_ref::<Error>() {
            log::error!(
                "Command {} completed with error code: {}",
                command.nonce,
                command.code,
            );
        } else {
            log::info!(
                "Command {} completed successfully {:#?}",
                command.nonce(),
                command
            );
        }
    }

    pub fn is_killed(&self) -> bool {
        self.state == State::Killed
    }
}

#[async_trait]
pub(super) trait CommandExt: Debug + Send {
    async fn execute(
        self: Box<Self>,
        sender: &mut Sender<UnixStream>,
    ) -> Result<Box<dyn CommandExt>>;

    fn nonce(&self) -> &str;

    fn as_any(&self) -> &dyn Any;
}

pub(super) struct Command;

impl Command {
    pub(super) fn new(data: &str, _player_id: Option<usize>) -> Box<dyn CommandExt> {
        let mut data = data.lines();

        let Some(nonce) = data.next().map(str::to_string) else {
            return Error::new("0", 0);
        };

        let Some(command) = data.next() else {
            return Error::new(&nonce, 0);
        };

        match command {
            "ECHO" => Echo::new(&nonce, &mut data),
            _ => Error::new(&nonce, 0),
        }
    }
}

#[derive(Debug)]
pub(super) struct Echo {
    nonce: String,
    msg: String,
}

impl Echo {
    pub(super) fn new(nonce: &str, data: &mut std::str::Lines<'_>) -> Box<dyn CommandExt> {
        Box::new(Echo {
            nonce: nonce.into(),
            msg: {
                let Some(msg) = data.next() else {
                    return Error::new(nonce, 3);
                };

                msg.into()
            },
        })
    }
}

#[async_trait]
impl CommandExt for Echo {
    async fn execute(
        self: Box<Self>,
        sender: &mut Sender<UnixStream>,
    ) -> Result<Box<dyn CommandExt>> {
        let Ok(mut stream) = UnixStream::connect("/monopoly_socks/host").await else {
            return Ok(Error::new(&self.nonce, 4));
        };

        let request = format!(
            concat!(
                "GET /api/internal/test HTTP/1.1",
                "\r\nHost: 127.0.0.1",
                "\r\nConnection: close",
                "\r\nContent-Type: text/plain",
                "\r\nContent-Length: {}",
                "\r\n\r\n{}",
            ),
            self.msg.len(),
            self.msg
        );

        let Ok(_) = stream.write_all(request.as_bytes()).await else {
            return Ok(Error::new(&self.nonce, 5));
        };

        let mut resp = String::new();
        let Ok(_) = stream.read_to_string(&mut resp).await else {
            return Ok(Error::new(&self.nonce, 6));
        };

        log::info!("RESULT OF ECHO: {}", resp);

        sender.send_text(&resp).await?;

        Ok(self)
    }

    fn nonce(&self) -> &str {
        &self.nonce
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug)]
pub(super) struct Error {
    nonce: String,
    code: u32,
}

impl Error {
    pub(super) fn new(nonce: &str, code: u32) -> Box<dyn CommandExt> {
        Box::new(Error {
            nonce: nonce.to_string(),
            code,
        })
    }
}

#[async_trait]
impl CommandExt for Error {
    async fn execute(
        self: Box<Self>,
        sender: &mut Sender<UnixStream>,
    ) -> Result<Box<dyn CommandExt>> {
        if self.code == 0 {
            sender
                .send_text(format!("{}\nEMPTY\n{}", self.nonce, self.code))
                .await?;
        } else {
            sender
                .send_text(format!("-{}\n{}", self.nonce, self.code))
                .await?;
        }

        Ok(self)
    }

    fn nonce(&self) -> &str {
        &self.nonce
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
