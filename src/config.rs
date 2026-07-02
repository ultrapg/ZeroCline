use serde::{Deserialize, Serialize};
use std::path::Path;
use anyhow::Result;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ZeroClineConfig {
    pub default_model: String,
    pub llama_port: u16,
    pub llama_host: String,
    pub backend: String,
    pub hide_second_terminal: bool,
}

impl Default for ZeroClineConfig {
    fn default() -> Self {
        Self {
            default_model: "nvidia-nemotron-3-nano-4b".to_string(),
            llama_port: 8080,
            llama_host: "127.0.0.1".to_string(),
            backend: "vulkan".to_string(),
            hide_second_terminal: true,
        }
    }
}

impl ZeroClineConfig {
    pub fn load_or_create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: ZeroClineConfig = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            let default_config = ZeroClineConfig::default();
            let content = serde_json::to_string_pretty(&default_config)?;
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, content)?;
            Ok(default_config)
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelConfig {
    pub name: String,
    pub filename: String,
    pub download_url: String,
    pub ctx_size: usize,
    pub n_gpu_layers: usize,
    pub temperature: f32,
    #[serde(default)]
    pub thinking: bool,
}

impl ModelConfig {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: ModelConfig = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
