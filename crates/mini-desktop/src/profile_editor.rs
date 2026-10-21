//! Public profile intent, independent of signing keys and local file contents.
use mini_app_protocol::{
    ProfileField, PublicProfileDetails, MAX_PROFILE_BIO_BYTES, MAX_PROFILE_NAME_BYTES,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Snapshot {
    pub name: String,
    pub bio: String,
    pub photo_path: String,
    pub avatar: Option<String>,
    pub remove_photo: bool,
    pub location: String,
    pub share_location: bool,
    pub age: String,
    pub share_age: bool,
    pub fields: String,
}
impl Snapshot {
    pub fn details(&self, imported_avatar: Option<&str>) -> Result<PublicProfileDetails, String> {
        let name = self.name.trim();
        let bio = self.bio.trim();
        if name.is_empty()
            || name.len() > MAX_PROFILE_NAME_BYTES
            || bio.len() > MAX_PROFILE_BIO_BYTES
        {
            return Err("Choose a display name up to 64 bytes and a bio up to 1024 bytes.".into());
        }
        let location = if self.share_location {
            let location = self.location.trim();
            if location.is_empty() || location.len() > mini_social::MAX_LOCATION_BYTES {
                return Err(
                    "Enter a location up to 128 bytes or turn off location sharing.".into(),
                );
            }
            Some(location.into())
        } else {
            None
        };
        let age = if self.share_age {
            Some(
                self.age
                    .trim()
                    .parse::<u8>()
                    .ok()
                    .filter(|age| *age != 0)
                    .ok_or("Age must be a whole number from 1 to 255, or turn off age sharing.")?,
            )
        } else {
            None
        };
        let fields = crate::parse_profile_fields(&self.fields)?
            .into_iter()
            .map(|field| ProfileField {
                label: field.label,
                value: field.value,
            })
            .collect();
        let avatar = if !self.photo_path.trim().is_empty() {
            Some(imported_avatar.ok_or("Import the selected photo through the application service before publishing.")?.into())
        } else if self.remove_photo {
            None
        } else {
            self.avatar.clone()
        };
        Ok(PublicProfileDetails {
            display_name: name.into(),
            bio: bio.into(),
            avatar,
            location,
            age,
            fields,
        })
    }
}

pub(crate) fn request_id(
    pending: &mut Option<(PublicProfileDetails, String)>,
    draft: &PublicProfileDetails,
) -> String {
    match pending {
        Some((previous, id)) if previous == draft => id.clone(),
        _ => {
            let id = crate::app_service::operation_id("profile-details");
            *pending = Some((draft.clone(), id.clone()));
            id
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn editor() -> Snapshot {
        Snapshot {
            name: " Name ".into(),
            bio: " Bio ".into(),
            ..Snapshot::default()
        }
    }
    #[test]
    fn disabled_private_fields_never_enter_public_intent() {
        let mut editor = editor();
        editor.location = "sensitive".repeat(256);
        editor.age = "private, not a number".into();
        let draft = editor.details(None).unwrap();
        assert_eq!(draft.location, None);
        assert_eq!(draft.age, None);
        editor.share_location = true;
        assert!(editor.details(None).is_err());
        editor.location = " Manchester ".into();
        editor.share_age = true;
        assert!(editor.details(None).is_err());
        editor.age = " 31 ".into();
        editor.fields = "Website: https://example.org\nLanguages: English".into();
        let draft = editor.details(None).unwrap();
        assert_eq!(draft.location.as_deref(), Some("Manchester"));
        assert_eq!(draft.age, Some(31));
        assert_eq!(draft.fields[0].label, "Website");
        assert_eq!(draft.fields[1].label, "Languages");
    }
    #[test]
    fn photo_selection_cannot_silently_fall_back_to_previous_avatar() {
        let mut editor = editor();
        editor.avatar = Some("old-avatar".into());
        assert_eq!(
            editor.details(None).unwrap().avatar.as_deref(),
            Some("old-avatar")
        );
        editor.remove_photo = true;
        assert_eq!(editor.details(None).unwrap().avatar, None);
        editor.photo_path = "new.png".into();
        assert!(editor.details(None).is_err());
        assert_eq!(
            editor
                .details(Some("imported-avatar"))
                .unwrap()
                .avatar
                .as_deref(),
            Some("imported-avatar")
        );
    }
    #[test]
    fn retries_bind_every_public_field_and_snapshot_detects_late_edits() {
        let editor = editor();
        let draft = editor.details(None).unwrap();
        let mut pending = None;
        let original = request_id(&mut pending, &draft);
        assert_eq!(request_id(&mut pending, &draft), original);
        let mut changed = draft.clone();
        changed.age = Some(23);
        assert_ne!(request_id(&mut pending, &changed), original);
        let mut changed_editor = editor.clone();
        changed_editor.photo_path = "later.png".into();
        assert_ne!(editor, changed_editor);
        changed_editor = editor.clone();
        changed_editor.share_location = true;
        assert_ne!(editor, changed_editor);
        changed_editor = editor.clone();
        changed_editor.fields = "Website: https://example.org".into();
        assert_ne!(editor, changed_editor);
    }
}
