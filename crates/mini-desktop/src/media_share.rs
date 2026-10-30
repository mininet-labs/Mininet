//! Intent and draft acknowledgement for sharing existing public media headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Draft {
    pub media_id: String,
    pub caption: String,
}
pub(crate) fn request_id(pending: &mut Option<(Draft, String)>, draft: &Draft) -> String {
    match pending {
        Some((old, id)) if old == draft => id.clone(),
        _ => {
            let id = crate::app_service::operation_id("media-post");
            *pending = Some((draft.clone(), id.clone()));
            id
        }
    }
}
pub(crate) fn acknowledges_editor(
    draft: &Draft,
    raw_caption: &str,
    current_target: Option<&str>,
    current_caption: &str,
) -> bool {
    current_target == Some(draft.media_id.as_str()) && raw_caption == current_caption
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertain_media_post_binds_header_and_caption_and_preserves_edited_or_retargeted_draft() {
        let draft = Draft {
            media_id: "header-a".into(),
            caption: "caption".into(),
        };
        let mut pending = None;
        let first = request_id(&mut pending, &draft);
        assert_eq!(request_id(&mut pending, &draft), first);
        assert!(acknowledges_editor(
            &draft,
            "caption ",
            Some("header-a"),
            "caption "
        ));
        assert!(!acknowledges_editor(
            &draft,
            "caption ",
            Some("header-a"),
            "caption"
        ));
        assert!(!acknowledges_editor(
            &draft,
            "caption ",
            Some("header-b"),
            "caption "
        ));
        assert!(!acknowledges_editor(&draft, "caption ", None, "caption "));
        let other = Draft {
            media_id: "header-b".into(),
            ..draft.clone()
        };
        let other_id = request_id(&mut pending, &other);
        assert_ne!(other_id, first);
        let edited = Draft {
            caption: "edited".into(),
            ..other.clone()
        };
        assert_ne!(request_id(&mut pending, &edited), other_id);
    }
}
