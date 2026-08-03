use did_mini::Controller;
use mini_objects::{Object, ObjectBuilder, ObjectType, Payload};
use mini_store::{FsBackend, MemoryBackend, Store, StoreError, TimeCursor, MAX_TIME_PAGE_SIZE};

fn temp_root(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "mini-store-time-pages-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn objects() -> Vec<Object> {
    let controller = Controller::incept_single_from_seeds(&[41; 32], &[42; 32]).unwrap();
    [30u64, 10, 20, 20, 40, 5, 30, 25, 50, 20]
        .into_iter()
        .enumerate()
        .map(|(sequence, timestamp_ms)| {
            ObjectBuilder::new(ObjectType::POST)
                .timestamp_ms(timestamp_ms)
                .sequence(sequence as u64 + 1)
                .payload(Payload::Public(format!("post-{sequence}").into_bytes()))
                .sign(&controller.did(), &controller)
                .unwrap()
        })
        .collect()
}

fn collect_pages<B: mini_store::Backend>(
    store: &Store<B>,
    start_ms: u64,
    page_size: usize,
) -> Vec<String> {
    let mut cursor: Option<TimeCursor> = None;
    let mut ids = Vec::new();
    loop {
        let page = store
            .since_page(start_ms, cursor.as_ref(), page_size)
            .unwrap();
        ids.extend(page.ids.iter().map(|id| id.as_str().to_string()));
        let Some(next) = page.next else {
            break;
        };
        cursor = Some(next);
    }
    ids
}

#[test]
fn memory_and_filesystem_pages_match_with_equal_and_out_of_order_timestamps() {
    let root = temp_root("equivalence");
    let mut memory = Store::new(MemoryBackend::new());
    let mut filesystem = Store::new(FsBackend::open(&root).unwrap());
    let objects = objects();
    for object in &objects {
        memory.insert(object).unwrap();
        filesystem.insert(object).unwrap();
    }

    let memory_ids = collect_pages(&memory, 0, 2);
    let filesystem_ids = collect_pages(&filesystem, 0, 2);
    assert_eq!(filesystem_ids, memory_ids);
    assert_eq!(filesystem_ids.len(), objects.len());

    let mut expected: Vec<(u64, String)> = objects
        .iter()
        .map(|object| (object.timestamp_ms, object.id().as_str().to_string()))
        .collect();
    expected.sort();
    assert_eq!(
        filesystem_ids,
        expected.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
    );

    let memory_recent: Vec<String> = memory
        .recent(4)
        .unwrap()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect();
    let filesystem_recent: Vec<String> = filesystem
        .recent(4)
        .unwrap()
        .into_iter()
        .map(|id| id.as_str().to_string())
        .collect();
    assert_eq!(filesystem_recent, memory_recent);

    drop(filesystem);
    let reopened = Store::new(FsBackend::open(&root).unwrap());
    assert_eq!(
        collect_pages(&reopened, 20, 3),
        collect_pages(&memory, 20, 3)
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn deleting_the_acceleration_index_triggers_a_deterministic_legacy_rebuild() {
    let root = temp_root("rebuild");
    let mut store = Store::new(FsBackend::open(&root).unwrap());
    for object in objects() {
        store.insert(&object).unwrap();
    }
    let before = collect_pages(&store, 0, 3);
    drop(store);

    std::fs::remove_dir_all(root.join("ordered")).unwrap();
    let backend = FsBackend::open(&root).unwrap();
    let rebuilt_count = backend.rebuild_time_index().unwrap();
    assert_eq!(rebuilt_count, before.len());
    let rebuilt = Store::new(backend);
    assert_eq!(collect_pages(&rebuilt, 0, 3), before);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn page_cursor_is_exclusive_and_start_bound_is_enforced() {
    let mut store = Store::new(MemoryBackend::new());
    for object in objects() {
        store.insert(&object).unwrap();
    }
    let first = store.since_page(20, None, 3).unwrap();
    assert_eq!(first.ids.len(), 3);
    let cursor = first.next.clone().unwrap();
    let second = store.since_page(20, Some(&cursor), 3).unwrap();
    assert!(first.ids.iter().all(|id| !second.ids.contains(id)));

    let invalid = TimeCursor::new(10, first.ids[0].clone());
    assert_eq!(
        store.since_page(20, Some(&invalid), 3),
        Err(StoreError::InvalidCursor)
    );
}

#[test]
fn page_limits_fail_closed() {
    let store = Store::new(MemoryBackend::new());
    let empty = store.since_page(0, None, 0).unwrap();
    assert!(empty.ids.is_empty());
    assert!(empty.next.is_none());
    assert_eq!(
        store.since_page(0, None, MAX_TIME_PAGE_SIZE + 1),
        Err(StoreError::LimitExceeded)
    );
    assert_eq!(
        store.recent(MAX_TIME_PAGE_SIZE + 1),
        Err(StoreError::LimitExceeded)
    );
}

#[test]
fn a_partial_delta_tail_rebuilds_from_authoritative_metadata() {
    use std::io::Write as _;

    let root = temp_root("partial-delta");
    let mut store = Store::new(FsBackend::open(&root).unwrap());
    for object in objects().into_iter().take(4) {
        store.insert(&object).unwrap();
    }
    let expected = collect_pages(&store, 0, 2);
    drop(store);

    let mut delta = std::fs::OpenOptions::new()
        .append(true)
        .open(root.join("ordered/time-v1/delta"))
        .unwrap();
    delta.write_all(&[0xff]).unwrap();
    delta.sync_all().unwrap();
    drop(delta);

    let reopened = Store::new(FsBackend::open(&root).unwrap());
    assert_eq!(collect_pages(&reopened, 0, 2), expected);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn independent_filesystem_writers_preserve_every_time_row() {
    let root = temp_root("concurrent-writers");
    let objects = objects();
    std::thread::scope(|scope| {
        for object in objects.iter() {
            let root = &root;
            scope.spawn(move || {
                let mut store = Store::new(FsBackend::open(root).unwrap());
                store.insert(object).unwrap();
            });
        }
    });

    let store = Store::new(FsBackend::open(&root).unwrap());
    let actual = collect_pages(&store, 0, 2);
    let mut expected: Vec<(u64, String)> = objects
        .iter()
        .map(|object| (object.timestamp_ms, object.id().as_str().to_string()))
        .collect();
    expected.sort();
    assert_eq!(
        actual,
        expected.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(root);
}

const PROCESS_WRITER_ROOT: &str = "MININET_TIME_PAGE_PROCESS_ROOT";
const PROCESS_WRITER_SEED: &str = "MININET_TIME_PAGE_PROCESS_SEED";
const PROCESS_WRITER_TIMESTAMP: &str = "MININET_TIME_PAGE_PROCESS_TIMESTAMP";
const PROCESS_WRITER_SEQUENCE: &str = "MININET_TIME_PAGE_PROCESS_SEQUENCE";

fn process_object(seed: u8, timestamp_ms: u64, sequence: u64) -> Object {
    let controller = Controller::incept_single_from_seeds(&[seed; 32], &[seed + 1; 32]).unwrap();
    ObjectBuilder::new(ObjectType::POST)
        .timestamp_ms(timestamp_ms)
        .sequence(sequence)
        .payload(Payload::Public(
            format!("process-post-{seed}-{sequence}").into_bytes(),
        ))
        .sign(&controller.did(), &controller)
        .unwrap()
}

#[test]
fn process_writer_child() {
    let Some(root) = std::env::var_os(PROCESS_WRITER_ROOT) else {
        return;
    };
    let seed: u8 = std::env::var(PROCESS_WRITER_SEED).unwrap().parse().unwrap();
    let timestamp_ms: u64 = std::env::var(PROCESS_WRITER_TIMESTAMP)
        .unwrap()
        .parse()
        .unwrap();
    let sequence: u64 = std::env::var(PROCESS_WRITER_SEQUENCE)
        .unwrap()
        .parse()
        .unwrap();

    let object = process_object(seed, timestamp_ms, sequence);
    let mut store = Store::new(FsBackend::open(std::path::Path::new(&root)).unwrap());
    store.insert(&object).unwrap();
}

#[test]
fn independent_process_writers_preserve_every_time_row() {
    let root = temp_root("process-writers");
    let executable = std::env::current_exe().unwrap();
    let specs: Vec<(u8, u64, u64)> = (0u8..8)
        .map(|index| {
            (
                80 + index,
                [40u64, 10, 30, 20, 20, 50, 5, 30][index as usize],
                u64::from(index) + 1,
            )
        })
        .collect();

    let mut children = Vec::new();
    for (seed, timestamp_ms, sequence) in &specs {
        children.push(
            std::process::Command::new(&executable)
                .arg("--exact")
                .arg("process_writer_child")
                .arg("--nocapture")
                .env(PROCESS_WRITER_ROOT, &root)
                .env(PROCESS_WRITER_SEED, seed.to_string())
                .env(PROCESS_WRITER_TIMESTAMP, timestamp_ms.to_string())
                .env(PROCESS_WRITER_SEQUENCE, sequence.to_string())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }

    let store = Store::new(FsBackend::open(&root).unwrap());
    let actual = collect_pages(&store, 0, 2);
    let mut expected: Vec<(u64, String)> = specs
        .into_iter()
        .map(|(seed, timestamp_ms, sequence)| {
            (
                timestamp_ms,
                process_object(seed, timestamp_ms, sequence)
                    .id()
                    .as_str()
                    .to_string(),
            )
        })
        .collect();
    expected.sort();
    assert_eq!(
        actual,
        expected.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn a_symlinked_ordered_index_is_refused() {
    use std::os::unix::fs::symlink;

    let root = temp_root("symlink");
    let outside = temp_root("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let mut store = Store::new(FsBackend::open(&root).unwrap());
    let object = objects().remove(0);
    store.insert(&object).unwrap();
    drop(store);

    std::fs::remove_dir_all(root.join("ordered")).unwrap();
    symlink(&outside, root.join("ordered")).unwrap();
    let store = Store::new(FsBackend::open(&root).unwrap());
    assert!(store.recent(1).is_err());

    let _ = std::fs::remove_file(root.join("ordered"));
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}
