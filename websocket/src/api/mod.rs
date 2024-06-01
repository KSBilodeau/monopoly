use std::sync::Arc;

use async_std::os::unix::net::UnixStream;
use parking_lot::Mutex;
use soketto::{Receiver, Sender};

use crate::api::front::CommandHandler;
use crate::util;

pub mod back;
pub mod front;

struct Socket {
    send: Arc<Mutex<Sender<UnixStream>>>,
    recv: Arc<Mutex<Receiver<UnixStream>>>,
}

impl Socket {
    fn send(&self, text: String) {
        util::sync!(self.send.lock().send_text(text)).unwrap();
    }

    fn recv(&self) -> String {
        let mut buf = vec![];
        util::sync!(self.recv.lock().receive_data(&mut buf)).unwrap();

        String::from_utf8(buf).unwrap()
    }
}

pub struct SocketHandler {
    sock: Socket,
    command_handler: CommandHandler,
}
