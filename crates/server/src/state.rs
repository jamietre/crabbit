use crate::config::Config;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub config: Arc<Config>,
    pub pending_oauth: Arc<Mutex<std::collections::HashMap<String, i64>>>,
}

impl AppState {
    pub fn new(conn: Connection, config: Config) -> Self {
        Self {
            db: Arc::new(Mutex::new(conn)),
            config: Arc::new(config),
            pending_oauth: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub fn with_db<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self.db.lock().map_err(|_| anyhow::anyhow!("db mutex poisoned"))?;
        f(&conn)
    }
}
