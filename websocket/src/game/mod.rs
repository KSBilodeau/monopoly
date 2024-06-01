use std::sync::Arc;

use async_std::os::unix::net::UnixStream;
use eyre::{bail, Result};
use parking_lot::Mutex;
use soketto::Sender;

#[derive(Debug, Clone)]
pub struct Player {
    id: usize,
    username: String,
    sock: Option<Arc<Mutex<Sender<UnixStream>>>>,
}

#[derive(Debug, Clone)]
struct Message {
    user_id: usize,
    msg_id: usize,
    content: String,
}

#[derive(Debug)]
pub struct Session {
    host: Option<String>,
    players: Vec<Player>,
    host_key: String,
    chat: Vec<Message>,
}

impl Session {
    pub fn new() -> Session {
        Session {
            host: None,
            players: vec![],
            host_key: std::env::var("MONOPOLY_HOST_KEY").unwrap(),
            chat: vec![],
        }
    }

    pub fn add_player(&mut self, username: &String, host_key: &Option<String>) -> Result<usize> {
        for player in &self.players {
            if &player.username == username {
                bail!("2");
            }
        }

        if let Some(key) = host_key {
            if self.host.is_some() {
                bail!("7");
            }

            if &*self.host_key == key {
                self.host = Some(username.clone())
            }
        }

        self.players.push(Player {
            id: self.players.len(),
            username: username.clone(),
            sock: None,
        });

        Ok(self.players.len() - 1)
    }

    pub fn assoc_sock(&mut self, id: usize, send: Arc<Mutex<Sender<UnixStream>>>) {
        self.players[id].sock = Some(send);
    }

    pub fn players(&mut self) -> &Vec<Player> {
        &mut self.players
    }

    pub fn player_id_by_username(&self, username: &str) -> Option<usize> {
        for player in &self.players {
            if player.username == username {
                return Some(player.id);
            }
        }

        None
    }

    pub fn player_username_by_id(&self, id: usize) -> Option<String> {
        for player in &self.players {
            if player.id == id {
                return Some(player.username.clone());
            }
        }

        None
    }

    pub fn add_message(&mut self, username: &str, content: &str) -> usize {
        let id = self.player_id_by_username(username).unwrap();

        self.chat.push(Message {
            user_id: id,
            msg_id: self.chat.len(),
            content: content.to_string(),
        });

        self.chat.len() - 1
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
