use std::path::PathBuf;

use tauri::Manager;
use thiserror::Error;
use zeroize::Zeroizing;

fn normalize_provider(provider: &str) -> String {
    let cleaned: String = provider
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    if cleaned.is_empty() {
        "gemini".to_string()
    } else {
        cleaned.to_ascii_lowercase()
    }
}

fn provider_key(provider: &str) -> Vec<u8> {
    format!("api_key::{}", normalize_provider(provider)).into_bytes()
}

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("stronghold error: {0}")]
    Stronghold(#[from] tauri_plugin_stronghold::stronghold::Error),
}

pub struct VaultPaths {
    pub snapshot_path: PathBuf,
    pub salt_path: PathBuf,
}

pub fn vault_paths<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<VaultPaths, VaultError> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    Ok(VaultPaths {
        snapshot_path: dir.join("gojira_vault.stronghold"),
        salt_path: dir.join("gojira_vault.salt"),
    })
}

pub fn load_api_key<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    passphrase: &str,
    provider: &str,
) -> Result<Option<String>, VaultError> {
    let paths = vault_paths(app)?;
    let key = tauri_plugin_stronghold::kdf::KeyDerivation::argon2(passphrase, &paths.salt_path);
    let stronghold = tauri_plugin_stronghold::stronghold::Stronghold::new(paths.snapshot_path, key)?;

    let client = stronghold
        .get_client("gojira".as_bytes().to_vec())
        .map_err(tauri_plugin_stronghold::stronghold::Error::from)?;
    let maybe = client
        .store()
        .get(&provider_key(provider))
        .map_err(tauri_plugin_stronghold::stronghold::Error::from)?;
    Ok(maybe.map(|bytes| String::from_utf8_lossy(&bytes).to_string()))
}

pub fn save_api_key<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    passphrase: &str,
    provider: &str,
    api_key: &str,
) -> Result<(), VaultError> {
    let paths = vault_paths(app)?;
    let key = tauri_plugin_stronghold::kdf::KeyDerivation::argon2(passphrase, &paths.salt_path);
    let stronghold = tauri_plugin_stronghold::stronghold::Stronghold::new(paths.snapshot_path, key)?;

    let client = stronghold
        .get_client("gojira".as_bytes().to_vec())
        .map_err(tauri_plugin_stronghold::stronghold::Error::from)?;
    let secret = Zeroizing::new(api_key.as_bytes().to_vec());
    let _ = client
        .store()
        .insert(
            provider_key(provider),
            secret.to_vec(),
            None,
        )
        .map_err(tauri_plugin_stronghold::stronghold::Error::from)?;
    stronghold.save()?;
    Ok(())
}

pub fn clear_api_key<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    passphrase: &str,
    provider: &str,
) -> Result<(), VaultError> {
    let paths = vault_paths(app)?;
    let key = tauri_plugin_stronghold::kdf::KeyDerivation::argon2(passphrase, &paths.salt_path);
    let stronghold = tauri_plugin_stronghold::stronghold::Stronghold::new(paths.snapshot_path, key)?;

    let client = stronghold
        .get_client("gojira".as_bytes().to_vec())
        .map_err(tauri_plugin_stronghold::stronghold::Error::from)?;
    let _ = client
        .store()
        .delete(&provider_key(provider))
        .map_err(tauri_plugin_stronghold::stronghold::Error::from)?;
    stronghold.save()?;
    Ok(())
}
