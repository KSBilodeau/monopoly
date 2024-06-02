use std::sync::Arc;

use crate::api::front::CommandHandler;
use async_std::os::unix::net::UnixStream;
use async_std::sync::Mutex;
use soketto::{Receiver, Sender};

pub mod back;
pub mod front;

struct Socket {
    send: Arc<Mutex<Sender<UnixStream>>>,
    recv: Arc<Mutex<Receiver<UnixStream>>>,
}

pub struct SocketHandler {
    ws_id: u32,
    sock: Socket,
}

impl SocketHandler {
    pub fn new(
        ws_id: u32,
        send: Arc<Mutex<Sender<UnixStream>>>,
        recv: Arc<Mutex<Receiver<UnixStream>>>,
    ) -> Self {
        Self {
            ws_id,
            sock: Socket { send, recv },
        }
    }

    pub fn comm_handler(&self) -> CommandHandler {
        CommandHandler::new(self.ws_id, self.sock.send.clone(), self.sock.recv.clone())
    }
}
