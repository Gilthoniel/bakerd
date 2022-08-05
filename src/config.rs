use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    jobs: HashMap<String, String>,
}

impl Config {
    pub fn get_jobs(&self) -> &HashMap<String, String> {
        &self.jobs
    }
}
