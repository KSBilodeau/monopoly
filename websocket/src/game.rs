use eyre::{bail, Result};

#[derive(Debug)]
pub struct Session {
    host: Option<String>,
    players: Vec<String>,
    host_key: String,
}

impl Session {
    pub fn new() -> Session {
        Session {
            host: None,
            players: vec![],
            host_key: std::env::var("MONOPOLY_HOST_KEY").unwrap(),
        }
    }

    pub fn add_player(&mut self, username: &String, host_key: &Option<String>) -> Result<()> {
        for player in &self.players {
            if player == username {
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

        self.players.push(username.clone());

        Ok(())
    }

    pub fn players(&self) -> &Vec<String> {
        &self.players
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}