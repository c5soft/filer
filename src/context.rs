use super::config;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppContext {
    pub(crate) config: Value,
}
impl AppContext {
    pub fn new() -> Arc<Self> {
        let path = config::get_config_file();
        Self::from(path)
    }
    pub fn from<P: AsRef<Path>>(path: P) -> Arc<Self> {
        let config = config::from(path);
        Arc::new(Self { config })
    }
}
