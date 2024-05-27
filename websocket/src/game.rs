use std::sync::LazyLock;
use eyre::{bail, Result};

#[derive(Debug)]
pub struct Session {
    host: Option<String>,
    players: Vec<String>,
    host_key: LazyLock<String>,
}

impl Session {
    pub const fn new() -> Session {
        Session {
            host: None,
            players: vec![],
            host_key: LazyLock::new(|| {
                std::env::var("MONOPOLY_HOST_KEY").expect("MONOPOLY_HOST_KEY MISSING")
            }),
        }
    }

    pub fn add_player(&mut self, username: &String, host_key: &Option<String>) -> Result<()> {
        for player in &self.players {
            if player == username {
                bail!("2");
            }
        }

        self.players.push(username.clone());

        if let Some(key) = host_key {
            if self.host.is_some() {
                bail!("7");
            }

            if &*self.host_key == key {
                self.host = Some(username.clone())
            }
        }

        Ok(())
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}