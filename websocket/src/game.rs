use eyre::{bail, Result};

#[derive(Debug, Clone)]
pub struct Player {
    id: usize,
    username: String,
}

#[derive(Debug)]
pub struct Session {
    host: Option<String>,
    players: Vec<Player>,
    host_key: String,
    chat: Vec<String>,
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

    pub fn add_player(&mut self, username: &String, host_key: &Option<String>) -> Result<()> {
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
        });

        Ok(())
    }

    pub fn players(&self) -> &Vec<Player> {
        &self.players
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
