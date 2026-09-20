//! Materialize timeline cards off the renderer thread. This removes repeated
//! disk scans from repaint; it does not claim the underlying feed is indexed.

use did_mini::Did;
use mini_objects::ObjectId;
use mini_social::{comments, feed, resolve_post, resolve_profile, FeedFilter};
use mini_store::{FsBackend, Store};
use std::path::Path;

#[derive(Clone)]
pub struct Card {
    pub id: ObjectId,
    pub author: String,
    pub did: String,
    pub body: String,
    pub timestamp_ms: u64,
    pub reason: &'static str,
    pub support_count: usize,
    pub comment_count: usize,
    pub media: bool,
}

pub fn snapshot(root: &Path, human: &Did, filter: FeedFilter) -> Result<Vec<Card>, String> {
    let store = Store::new(FsBackend::open(root).map_err(|error| error.to_string())?);
    feed(&store, human, filter, 50)
        .map_err(|error| error.to_string())?
        .iter()
        .map(|item| {
            let profile =
                resolve_profile(&store, &item.author).map_err(|error| error.to_string())?;
            let post = resolve_post(&store, &item.id).map_err(|error| error.to_string())?;
            Ok(Card {
                id: item.id.clone(),
                author: profile
                    .map(|profile| profile.display_name)
                    .unwrap_or_else(|| "Mininet participant".into()),
                did: item.author.as_str().to_owned(),
                body: post.text,
                timestamp_ms: item.timestamp_ms,
                reason: match item.reason {
                    mini_social::FeedReason::Own => "Your post",
                    mini_social::FeedReason::Followed => "You follow this author",
                },
                support_count: item.support_count,
                comment_count: comments(&store, &item.id)
                    .map_err(|error| error.to_string())?
                    .len(),
                media: matches!(post.kind, mini_social::PostKind::Media { .. }),
            })
        })
        .collect()
}

pub fn age(timestamp_ms: u64, now_ms: u64) -> String {
    if timestamp_ms > now_ms {
        return "future author time".into();
    }
    let seconds = (now_ms - timestamp_ms) / 1000;
    match seconds {
        0..60 => "now".into(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::age;

    #[test]
    fn author_time_is_real_and_future_time_is_not_underflowed() {
        assert_eq!(age(0, 120_000), "2m");
        assert_eq!(age(0, 7_200_000), "2h");
        assert_eq!(age(0, 172_800_000), "2d");
        assert_eq!(age(u64::MAX, 0), "future author time");
    }
}
