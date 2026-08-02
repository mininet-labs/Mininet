#!/usr/bin/env python3
"""Prepare the legacy backend fixture for the strict time-index key format."""

from pathlib import Path


root = Path(__file__).resolve().parents[1]
path = root / "crates/mini-store/src/backend.rs"
text = path.read_text(encoding="utf-8")
old = '''    #[test]
    fn fs_backend_default_last_impl_agrees_with_memory_backend() {
        let dir = std::env::temp_dir().join(format!(
            "mini-store-list-last-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut fs_backend = FsBackend::open(&dir).unwrap();
        let mut mem_backend = MemoryBackend::new();
        for (k, v) in [
            ("idx/time/00000000000000001000/a", "a"),
            ("idx/time/00000000000000002000/b", "b"),
            ("idx/time/00000000000000003000/c", "c"),
        ] {
            fs_backend.put_meta(k, v.as_bytes()).unwrap();
            mem_backend.put_meta(k, v.as_bytes()).unwrap();
        }

        assert_eq!(
            fs_backend.list_meta_prefix_last("idx/time/", 2).unwrap(),
            mem_backend.list_meta_prefix_last("idx/time/", 2).unwrap()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
'''
new = '''    #[test]
    fn fs_backend_ordered_last_agrees_with_memory_backend() {
        fn object_id(seed: u8) -> String {
            let digest = mini_crypto::Multihash::of(
                mini_crypto::HashAlgorithm::Blake3,
                &[seed],
            );
            mini_crypto::encoding::encode(
                mini_crypto::encoding::BASE58BTC,
                &digest.to_bytes(),
            )
            .unwrap()
        }

        let dir = std::env::temp_dir().join(format!(
            "mini-store-list-last-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut fs_backend = FsBackend::open(&dir).unwrap();
        let mut mem_backend = MemoryBackend::new();
        let rows = [
            (
                format!("idx/time/00000000000000001000/{}", object_id(1)),
                b"a".as_slice(),
            ),
            (
                format!("idx/time/00000000000000002000/{}", object_id(2)),
                b"b".as_slice(),
            ),
            (
                format!("idx/time/00000000000000003000/{}", object_id(3)),
                b"c".as_slice(),
            ),
        ];
        for (key, value) in &rows {
            fs_backend.put_meta(key, value).unwrap();
            mem_backend.put_meta(key, value).unwrap();
        }

        assert_eq!(
            fs_backend.list_meta_prefix_last("idx/time/", 2).unwrap(),
            mem_backend.list_meta_prefix_last("idx/time/", 2).unwrap()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
'''
if text.count(old) != 1:
    raise SystemExit("expected exactly one legacy FsBackend time-index fixture")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
Path(__file__).unlink()
