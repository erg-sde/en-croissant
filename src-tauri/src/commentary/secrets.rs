//! Provider keys. Written once from the webview, read only here when a request is built, and never
//! returned: the webview sees `SecretStatus`. This is isolation, not a keychain; `SecretStore` is a
//! trait so a keyring-backed store can replace the file.

use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::PathBuf,
};

use serde::Serialize;
use specta::Type;
use tauri::Manager;

use super::provider::ProviderKind;
use crate::error::Error;

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub struct SecretStatus {
    pub configured: bool,
    /// The key's last four characters.
    pub hint: Option<String>,
}

pub trait SecretStore {
    fn get(&self, provider: ProviderKind) -> Result<Option<String>, Error>;
    fn set(&self, provider: ProviderKind, secret: &str) -> Result<(), Error>;
    fn clear(&self, provider: ProviderKind) -> Result<(), Error>;
}

/// Keys in `<app_data_dir>/commentary/secrets.json`, readable only by the user on Unix.
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn for_app(app: &tauri::AppHandle) -> Result<Self, Error> {
        let dir = app.path().app_data_dir()?.join("commentary");
        Ok(Self::new(dir.join("secrets.json")))
    }

    fn read(&self) -> Result<BTreeMap<String, String>, Error> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_default()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn write(&self, keys: &BTreeMap<String, String>) -> Result<(), Error> {
        if keys.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
                _ => Ok(()),
            };
        }
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string(keys)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&self.path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(text.as_bytes())?;
        Ok(())
    }
}

impl SecretStore for FileStore {
    fn get(&self, provider: ProviderKind) -> Result<Option<String>, Error> {
        Ok(self.read()?.remove(provider.key()))
    }

    fn set(&self, provider: ProviderKind, secret: &str) -> Result<(), Error> {
        let mut keys = self.read()?;
        keys.insert(provider.key().to_string(), secret.trim().to_string());
        self.write(&keys)
    }

    fn clear(&self, provider: ProviderKind) -> Result<(), Error> {
        let mut keys = self.read()?;
        keys.remove(provider.key());
        self.write(&keys)
    }
}

pub fn status(store: &impl SecretStore, provider: ProviderKind) -> Result<SecretStatus, Error> {
    Ok(match store.get(provider)? {
        Some(secret) if !secret.is_empty() => {
            let chars: Vec<char> = secret.chars().collect();
            let hint = (chars.len() >= 8).then(|| chars[chars.len() - 4..].iter().collect());
            SecretStatus {
                configured: true,
                hint,
            }
        }
        _ => SecretStatus {
            configured: false,
            hint: None,
        },
    })
}

#[tauri::command]
#[specta::specta]
pub async fn set_commentary_secret(
    provider: ProviderKind,
    secret: String,
    app: tauri::AppHandle,
) -> Result<(), Error> {
    let store = FileStore::for_app(&app)?;
    if secret.trim().is_empty() {
        store.clear(provider)
    } else {
        store.set(provider, &secret)
    }
}

#[tauri::command]
#[specta::specta]
pub async fn clear_commentary_secret(
    provider: ProviderKind,
    app: tauri::AppHandle,
) -> Result<(), Error> {
    FileStore::for_app(&app)?.clear(provider)
}

#[tauri::command]
#[specta::specta]
pub async fn get_commentary_secret_status(
    provider: ProviderKind,
    app: tauri::AppHandle,
) -> Result<SecretStatus, Error> {
    status(&FileStore::for_app(&app)?, provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_clear_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().join("commentary").join("secrets.json"));
        assert_eq!(store.get(ProviderKind::Anthropic).unwrap(), None);
        store
            .set(ProviderKind::Anthropic, " sk-ant-test-1234abcd \n")
            .unwrap();
        store.set(ProviderKind::OpenaiCompatible, "local").unwrap();
        assert_eq!(
            store.get(ProviderKind::Anthropic).unwrap().as_deref(),
            Some("sk-ant-test-1234abcd")
        );
        assert_eq!(
            status(&store, ProviderKind::Anthropic).unwrap(),
            SecretStatus {
                configured: true,
                hint: Some("abcd".into())
            }
        );
        // Too short to reveal four characters of.
        assert_eq!(
            status(&store, ProviderKind::OpenaiCompatible).unwrap().hint,
            None
        );
        store.clear(ProviderKind::Anthropic).unwrap();
        assert!(!status(&store, ProviderKind::Anthropic).unwrap().configured);
        store.clear(ProviderKind::OpenaiCompatible).unwrap();
        assert!(!store.path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().join("secrets.json"));
        store.set(ProviderKind::Anthropic, "secret").unwrap();
        let mode = fs::metadata(&store.path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
