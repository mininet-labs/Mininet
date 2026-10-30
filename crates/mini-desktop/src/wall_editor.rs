//! Public wall draft/intent. No identity creation, link fetching or advertisement.
use mini_app_protocol::{PublicWallDraft, WallVisibility};
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Snapshot {
    pub name: String,
    pub bio: String,
    pub links: String,
    pub unlisted: bool,
}
impl Snapshot {
    pub fn draft(&self) -> Result<PublicWallDraft, String> {
        let name = self.name.trim();
        let bio = self.bio.trim();
        let links: Vec<String> = self
            .links
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        if name.is_empty()
            || name.len() > mini_social::MAX_WALL_NAME_BYTES
            || bio.len() > mini_social::MAX_WALL_BIO_BYTES
            || links.len() > mini_social::MAX_WALL_LINKS
            || links
                .iter()
                .any(|link| link.len() > mini_social::MAX_WALL_LINK_BYTES)
        {
            return Err("Wall needs a name up to 64 bytes, bio up to 1024 bytes, and at most 16 links of 256 bytes each.".into());
        }
        Ok(PublicWallDraft {
            display_name: name.into(),
            bio: bio.into(),
            public_links: links,
            visibility: if self.unlisted {
                WallVisibility::Unlisted
            } else {
                WallVisibility::Public
            },
        })
    }
}
pub(crate) fn request_id(
    pending: &mut Option<(PublicWallDraft, String)>,
    draft: &PublicWallDraft,
) -> String {
    match pending {
        Some((old, id)) if old == draft => id.clone(),
        _ => {
            let id = crate::app_service::operation_id("wall");
            *pending = Some((draft.clone(), id.clone()));
            id
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            name: " Wall ".into(),
            bio: " Bio ".into(),
            links: " https://one.example\n\nhttps://two.example ".into(),
            unlisted: false,
        }
    }
    #[test]
    fn visibility_and_ordered_public_links_are_part_of_the_exact_retry_intent() {
        let mut editor = snapshot();
        let draft = editor.draft().unwrap();
        assert_eq!(
            draft.public_links,
            ["https://one.example", "https://two.example"]
        );
        let mut pending = None;
        let first = request_id(&mut pending, &draft);
        assert_eq!(request_id(&mut pending, &draft), first);
        editor.unlisted = true;
        let unlisted = request_id(&mut pending, &editor.draft().unwrap());
        assert_ne!(unlisted, first);
        editor.links = "https://two.example\nhttps://one.example".into();
        assert_ne!(request_id(&mut pending, &editor.draft().unwrap()), unlisted);
    }
    #[test]
    fn invalid_draft_is_refused_without_mutating_the_editor() {
        let mut editor = snapshot();
        editor.name = " ".into();
        let original = editor.clone();
        assert!(editor.draft().is_err());
        assert_eq!(editor, original);
        editor.name = "Wall".into();
        editor.links = "x".repeat(257);
        assert!(editor.draft().is_err());
        editor.links = "x\n".repeat(17);
        assert!(editor.draft().is_err());
    }
}
