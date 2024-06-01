use std::fmt::Debug;
use std::sync::{mpsc, Arc};

use async_std::os::unix::net::UnixStream;
use parking_lot::Mutex;
use soketto::Sender;

use crate::game::Session;
use crate::util;

#[derive(Debug, Eq, PartialEq)]
enum EventState {
    Running,
    Killed,
}

pub struct EventHandler {
    ws_id: u32,
    state: EventState,
    recv: mpsc::Receiver<Event>,
    game: Arc<Mutex<Session>>,
}

impl EventHandler {
    pub fn new(ws_id: u32, recv: mpsc::Receiver<Event>, game: Arc<Mutex<Session>>) -> EventHandler {
        EventHandler {
            ws_id,
            state: EventState::Running,
            recv,
            game,
        }
    }

    pub fn pump_event(&mut self) -> Option<Box<dyn EventExt>> {
        let Ok(event) = self.recv.recv() else {
            self.state = EventState::Killed;
            return None;
        };

        Some(event.into())
    }

    pub fn execute_event(
        &mut self,
        event: Box<dyn EventExt>,
        send: Arc<Mutex<Sender<UnixStream>>>,
    ) -> Box<dyn EventExt> {
        event.execute(self.game.clone()).respond(send)
    }

    pub fn is_kill(&self) -> bool {
        self.state == EventState::Killed
    }
}

pub trait EventExt: Debug {
    fn execute(self: Box<Self>, game: Arc<Mutex<Session>>) -> Box<dyn EventExt>;

    fn respond(self: Box<Self>, send: Arc<Mutex<Sender<UnixStream>>>) -> Box<dyn EventExt>;

    fn is_error(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub enum Event {
    Msg(Message),
}

impl From<Event> for Box<dyn EventExt> {
    fn from(event: Event) -> Self {
        match event {
            Event::Msg(msg) => Box::new(msg),
        }
    }
}

#[derive(Debug)]
pub struct Message {
    username: String,
    msg: String,
}

impl Message {
    pub fn new(username: &String, msg: &String) -> Message {
        Message {
            username: username.clone(),
            msg: msg.clone(),
        }
    }
}

impl EventExt for Message {
    fn execute(mut self: Box<Self>, _: Arc<Mutex<Session>>) -> Box<dyn EventExt> {
        self
    }

    fn respond(self: Box<Self>, send: Arc<Mutex<Sender<UnixStream>>>) -> Box<dyn EventExt> {
        util::sync!(send.lock().send_text(format!(
            "0\nCHAT\n{}\n{}",
            self.username,
            self.msg
        )))
        .unwrap();

        self
    }
}
