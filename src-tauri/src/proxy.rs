use crate::models::ProxyConfig;
use std::fs;
use std::path::{Path, PathBuf};

const PROXY_FILE: &str = "proxy.json";

pub fn load_proxy_from_dir(config_dir: &Path) -> Result<ProxyConfig, String> {
    let path = config_dir.join(PROXY_FILE);
    if !path.exists() {
        return Ok(ProxyConfig::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("读取代理配置失败: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("解析代理配置失败: {e}"))
}

pub fn save_proxy_to_dir(config_dir: &Path, config: &ProxyConfig) -> Result<(), String> {
    fs::create_dir_all(config_dir).map_err(|e| format!("创建配置目录失败: {e}"))?;
    let path = config_dir.join(PROXY_FILE);
    let content =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化代理配置失败: {e}"))?;
    fs::write(path, content).map_err(|e| format!("写入代理配置失败: {e}"))
}

pub fn proxy_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join(PROXY_FILE)
}
