//! The media catalog: every media post this device holds, indexed for a
//! YouTube-style browse — title, description, author, kind, size, likes,
//! comments, completeness — with local search, filters and sorting. Built
//! off the render thread from the object store; bounded so a large store
//! still produces a page quickly.

use crate::library;
use crate::player::{playback_for, Playback};
use did_mini::Did;
use mini_media::{missing_chunks, read_manifest};
use mini_objects::{ObjectId, ObjectType};
use mini_social::{comments, reaction_counts, resolve_post, resolve_profile, PostKind};
use mini_store::{Backend, FsBackend, Store};
use std::collections::HashMap;
use std::path::Path;

/// Posts scanned per build. Newer posts win; older ones stay on disk.
pub const MAX_ENTRIES: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Video,
    Music,
    Image,
    Animation,
    File,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Video => "Video",
            Kind::Music => "Music",
            Kind::Image => "Image",
            Kind::Animation => "GIF",
            Kind::File => "File",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Kind::Video => "🎬",
            Kind::Music => "♫",
            Kind::Image => "🖼",
            Kind::Animation => "🎬",
            Kind::File => "📋",
        }
    }

    pub fn from_content_type(content_type: &str) -> Self {
        match playback_for(content_type) {
            Playback::Video | Playback::VideoUnsupported => Kind::Video,
            Playback::Audio => Kind::Music,
            Playback::Image => Kind::Image,
            Playback::Animation => Kind::Animation,
            Playback::Other => Kind::File,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub post: ObjectId,
    pub media: ObjectId,
    pub title: String,
    pub description: String,
    pub file_name: String,
    pub content_type: String,
    pub kind: Kind,
    pub bytes: u64,
    pub author_name: String,
    pub author_did: String,
    pub author_avatar: Option<ObjectId>,
    pub timestamp_ms: u64,
    pub likes: usize,
    pub comments: usize,
    pub complete: bool,
    pub own: bool,
}

impl Entry {
    /// Case-insensitive match over the fields a person would search by.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        query.split_whitespace().all(|word| {
            self.title.to_lowercase().contains(word)
                || self.description.to_lowercase().contains(word)
                || self.file_name.to_lowercase().contains(word)
                || self.author_name.to_lowercase().contains(word)
                || self.author_did.to_lowercase().contains(word)
                || self.content_type.to_lowercase().contains(word)
                || self.kind.label().to_lowercase() == word
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Newest,
    MostLiked,
    MostDiscussed,
}

impl Sort {
    pub fn label(self) -> &'static str {
        match self {
            Sort::Newest => "Newest",
            Sort::MostLiked => "Most liked",
            Sort::MostDiscussed => "Most discussed",
        }
    }
}

pub fn snapshot(root: &Path, viewer: &Did) -> Result<Vec<Entry>, String> {
    let store = Store::new(FsBackend::open(root).map_err(|error| error.to_string())?);
    build(&store, viewer)
}

/// Every media post, newest first.
pub fn build<B: Backend>(store: &Store<B>, viewer: &Did) -> Result<Vec<Entry>, String> {
    let mut posts: Vec<(ObjectId, u64)> = Vec::new();
    for id in store
        .by_type(&ObjectType::POST)
        .map_err(|error| error.to_string())?
    {
        let Ok(object) = store.get(&id) else { continue };
        posts.push((id, object.timestamp_ms));
    }
    posts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.as_str().cmp(a.0.as_str())));
    let mut names: HashMap<String, (String, Option<ObjectId>)> = HashMap::new();
    let mut entries = Vec::new();
    for (id, _) in posts {
        if entries.len() >= MAX_ENTRIES {
            break;
        }
        let Ok(post) = resolve_post(store, &id) else {
            continue;
        };
        let PostKind::Media { media } = post.kind else {
            continue;
        };
        let (file_name, content_type, bytes, complete) = match store.get(&media) {
            Ok(object) => match read_manifest(&object) {
                Ok(manifest) => {
                    let complete = missing_chunks(store, &manifest)
                        .map(|missing| missing.is_empty())
                        .unwrap_or(false);
                    (
                        String::new(),
                        manifest.content_type,
                        manifest.total_len,
                        complete,
                    )
                }
                Err(_) => match library::read_collection(&object) {
                    Ok(collection) => {
                        let complete = collection.parts.iter().all(|part| {
                            store
                                .get(part)
                                .ok()
                                .and_then(|o| read_manifest(&o).ok())
                                .is_some_and(|manifest| {
                                    missing_chunks(store, &manifest)
                                        .map(|missing| missing.is_empty())
                                        .unwrap_or(false)
                                })
                        });
                        (
                            collection.name,
                            collection.content_type,
                            collection.total_len,
                            complete,
                        )
                    }
                    Err(_) => continue,
                },
            },
            // The manifest has not arrived yet: list it as pending.
            Err(_) => (String::new(), "application/octet-stream".into(), 0, false),
        };
        let did = post.author.as_str().to_owned();
        let (author_name, author_avatar) = match names.get(&did) {
            Some(entry) => entry.clone(),
            None => {
                let entry = resolve_profile(store, &post.author)
                    .ok()
                    .flatten()
                    .map(|profile| (profile.display_name, profile.avatar))
                    .unwrap_or_else(|| ("Mininet participant".into(), None));
                names.insert(did.clone(), entry.clone());
                entry
            }
        };
        let (title, description) = split_title(&post.text, &file_name);
        entries.push(Entry {
            kind: Kind::from_content_type(&content_type),
            likes: reaction_counts(store, &id)
                .map(|counts| counts.into_iter().map(|(_, n)| n).sum())
                .unwrap_or(0),
            comments: comments(store, &id).map(|c| c.len()).unwrap_or(0),
            own: &post.author == viewer,
            post: id,
            media,
            title,
            description,
            file_name,
            content_type,
            bytes,
            author_name,
            author_did: did,
            author_avatar,
            timestamp_ms: post.timestamp_ms,
            complete,
        });
    }
    Ok(entries)
}

/// Title = first line of the caption, else the file name, else "Untitled".
pub fn split_title(caption: &str, file_name: &str) -> (String, String) {
    let caption = caption.trim();
    let (title, rest) = match caption.split_once('\n') {
        Some((title, rest)) => (title.trim(), rest.trim()),
        None => (caption, ""),
    };
    let title = if title.is_empty() {
        if file_name.is_empty() {
            "Untitled".to_string()
        } else {
            file_name.to_string()
        }
    } else {
        title.to_string()
    };
    (title, rest.to_string())
}

pub fn sort(entries: &mut [Entry], sort: Sort) {
    match sort {
        Sort::Newest => entries.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms)),
        Sort::MostLiked => entries.sort_by(|a, b| {
            b.likes
                .cmp(&a.likes)
                .then_with(|| b.timestamp_ms.cmp(&a.timestamp_ms))
        }),
        Sort::MostDiscussed => entries.sort_by(|a, b| {
            b.comments
                .cmp(&a.comments)
                .then_with(|| b.timestamp_ms.cmp(&a.timestamp_ms))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use did_mini::{Capabilities, Controller};
    use mini_media::publish_media;
    use mini_social::{
        publish_media_post, publish_post, publish_profile, set_reaction, ReactionKind,
    };
    use mini_store::MemoryBackend;

    fn person(seed: u8) -> (Controller, Controller) {
        let mut root = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
        let device = Controller::incept_device_single_from_seeds(
            &root.did(),
            &[seed + 2; 32],
            &[seed + 3; 32],
        )
        .unwrap();
        root.delegate_device(&device.did(), Capabilities::primary())
            .unwrap();
        (root, device)
    }

    #[test]
    fn catalog_indexes_media_posts_with_titles_authors_and_search() {
        let mut store = Store::new(MemoryBackend::new());
        let (alice, alice_dev) = person(10);
        let (bob, bob_dev) = person(20);
        publish_profile(
            &mut store,
            &alice.did(),
            &alice_dev,
            "Alice",
            "",
            None,
            1,
            1,
        )
        .unwrap();
        let clip = publish_media(
            &mut store,
            &alice.did(),
            &alice_dev,
            "video/mp4",
            &[1; 10],
            2,
            2,
        )
        .unwrap();
        let song = publish_media(
            &mut store,
            &bob.did(),
            &bob_dev,
            "audio/mpeg",
            &[2; 10],
            3,
            1,
        )
        .unwrap();
        let clip_post = publish_media_post(
            &mut store,
            &alice.did(),
            &alice_dev,
            clip.id.clone(),
            "Sunset timelapse\nShot on the roof",
            10,
            4,
        )
        .unwrap();
        publish_media_post(&mut store, &bob.did(), &bob_dev, song.id.clone(), "", 20, 3).unwrap();
        publish_post(&mut store, &bob.did(), &bob_dev, "text only", 30, 4).unwrap();
        set_reaction(
            &mut store,
            &bob.did(),
            &bob_dev,
            clip_post.id(),
            ReactionKind::Like,
            true,
            40,
            5,
        )
        .unwrap();

        let mut entries = build(&store, &alice.did()).unwrap();
        assert_eq!(entries.len(), 2, "text-only posts are not media");
        // Newest first: Bob's song, then Alice's clip.
        assert_eq!(entries[0].kind, Kind::Music);
        assert_eq!(entries[0].title, "Untitled");
        assert_eq!(entries[0].author_name, "Mininet participant");
        assert_eq!(entries[1].title, "Sunset timelapse");
        assert_eq!(entries[1].description, "Shot on the roof");
        assert_eq!(entries[1].author_name, "Alice");
        assert_eq!(entries[1].kind, Kind::Video);
        assert_eq!(entries[1].likes, 1);
        assert!(entries[1].own);
        assert!(entries[1].complete);

        assert!(entries[1].matches("sunset roof"));
        assert!(entries[1].matches("ALICE"));
        assert!(entries[1].matches("video"));
        assert!(!entries[1].matches("bob"));
        assert!(entries[0].matches(""));

        sort(&mut entries, Sort::MostLiked);
        assert_eq!(entries[0].title, "Sunset timelapse");
        sort(&mut entries, Sort::Newest);
        assert_eq!(entries[0].kind, Kind::Music);
        assert_eq!(
            split_title("", "movie.mp4"),
            ("movie.mp4".into(), String::new())
        );
    }
}
