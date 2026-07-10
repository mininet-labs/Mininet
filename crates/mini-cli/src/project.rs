//! Local project aliases: a human-typable name mapped to a project's real
//! `ObjectId`, purely a local convenience. What's cryptographically real is
//! always the id — two people are free to use different local names for
//! the same project, or `track` a project someone else created under
//! whatever name they like.

use std::fs;
use std::path::{Path, PathBuf};

use mini_objects::ObjectId;

use crate::error::{CliError, Result};

fn projects_dir(home: &Path) -> PathBuf {
    home.join("projects")
}

fn alias_path(home: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(CliError::Usage(format!("invalid project alias: {name:?}")));
    }
    Ok(projects_dir(home).join(name))
}

/// Record a local alias `name` for `project_id`. Errors if the alias
/// already points somewhere.
pub fn track(home: &Path, name: &str, project_id: &ObjectId) -> Result<()> {
    let path = alias_path(home, name)?;
    if path.exists() {
        return Err(CliError::Usage(format!(
            "alias {name:?} is already tracked -- pick a different local name"
        )));
    }
    fs::create_dir_all(projects_dir(home)).map_err(|e| CliError::Io(e.to_string()))?;
    fs::write(&path, project_id.as_str()).map_err(|e| CliError::Io(e.to_string()))?;
    Ok(())
}

/// Resolve `reference` to a project id: if it parses as a raw object id,
/// use it directly; otherwise look it up as a local alias.
pub fn resolve(home: &Path, reference: &str) -> Result<ObjectId> {
    if let Ok(id) = ObjectId::parse(reference) {
        return Ok(id);
    }
    let path = alias_path(home, reference)?;
    let raw = fs::read_to_string(&path).map_err(|_| {
        CliError::Usage(format!(
            "{reference:?} is not a known project alias or a valid object id"
        ))
    })?;
    ObjectId::parse(raw.trim()).map_err(|e| CliError::Object(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mini_objects::{ObjectBuilder, ObjectType, Payload};

    fn any_id() -> ObjectId {
        // Any well-formed object id; content doesn't matter for alias tests.
        let root = did_mini::Controller::incept_single_from_seeds(&[1u8; 32], &[2u8; 32]).unwrap();
        let device = did_mini::Controller::incept_device_single_from_seeds(
            &root.did(),
            &[3u8; 32],
            &[4u8; 32],
        )
        .unwrap();
        let obj = ObjectBuilder::new(ObjectType::Custom("test".to_string()))
            .payload(Payload::Public(vec![1, 2, 3]))
            .sign(&root.did(), &device)
            .unwrap();
        obj.id().clone()
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "mini-cli-project-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        p
    }

    #[test]
    fn tracked_alias_resolves_to_the_right_id() {
        let dir = tempdir();
        let id = any_id();
        track(&dir, "myrepo", &id).unwrap();
        assert_eq!(resolve(&dir, "myrepo").unwrap(), id);
    }

    #[test]
    fn a_raw_object_id_resolves_without_being_tracked() {
        let dir = tempdir();
        let id = any_id();
        assert_eq!(resolve(&dir, id.as_str()).unwrap(), id);
    }

    #[test]
    fn tracking_the_same_alias_twice_is_rejected() {
        let dir = tempdir();
        let id = any_id();
        track(&dir, "myrepo", &id).unwrap();
        assert!(track(&dir, "myrepo", &id).is_err());
    }

    #[test]
    fn unknown_alias_is_rejected() {
        let dir = tempdir();
        assert!(resolve(&dir, "nope").is_err());
    }
}
