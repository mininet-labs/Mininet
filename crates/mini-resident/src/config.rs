//! Local model selection contains no API key. Saving never connects or starts a model.
use crate::{validate_provider_config, ProviderConfig, ResidentError, Result};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

const MAX_CONFIG: usize = 4096;

pub fn load_config(home: &Path) -> Result<Option<ProviderConfig>> {
    let path = home.join("resident.json");
    match fs::symlink_metadata(&path) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            return Err(ResidentError(
                "Resident AI configuration must be a regular owner-controlled file".into(),
            ))
        }
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_CONFIG as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_CONFIG {
        return Err(ResidentError(
            "Resident AI configuration exceeds limit".into(),
        ));
    }
    let config = serde_json::from_slice(&bytes)?;
    validate_provider_config(&config)?;
    Ok(Some(config))
}

pub fn save_config(home: &Path, config: &ProviderConfig) -> Result<()> {
    validate_provider_config(config)?;
    if home.is_symlink() {
        return Err(ResidentError("Resident AI home cannot be a symlink".into()));
    }
    let bytes = serde_json::to_vec(config)?;
    if bytes.len() > MAX_CONFIG {
        return Err(ResidentError(
            "Resident AI configuration exceeds limit".into(),
        ));
    }
    mini_durable::create_dir_all(home)?;
    mini_durable::atomic_replace(&home.join("resident.json"), &bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_configuration_roundtrips_and_corruption_fails_closed() {
        let temp = tempfile::tempdir().unwrap();
        assert!(load_config(temp.path()).unwrap().is_none());
        let config = ProviderConfig {
            endpoint: "http://127.0.0.1:8080/v1/chat/completions".into(),
            model: "owner-selected-model".into(),
        };
        save_config(temp.path(), &config).unwrap();
        assert_eq!(load_config(temp.path()).unwrap(), Some(config));
        fs::write(temp.path().join("resident.json"), b"{}").unwrap();
        assert!(load_config(temp.path()).is_err());
    }
    #[test]
    fn configuration_cannot_silently_enable_a_remote_provider() {
        let temp = tempfile::tempdir().unwrap();
        let config = ProviderConfig {
            endpoint: "http://203.0.113.1:8080/v1/chat/completions".into(),
            model: "model".into(),
        };
        assert!(save_config(temp.path(), &config).is_err());
        assert!(!temp.path().join("resident.json").exists());
    }
}
