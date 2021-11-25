use super::config;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

pub struct AppContext {
    pub(crate) config: Value,
}
impl AppContext {
    pub fn new() -> Arc<Self> {
        let path = config::get_config_file();
        Self::from(path)
    }
    pub fn from(path: PathBuf) -> Arc<Self> {
        let config = config::from(path);
        Arc::new(Self { config })
    }
}
