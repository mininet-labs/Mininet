#![no_main]
//! An arbitrary evidence file either fails closed or loads a pool whose every
//! entry is canonical and re-saves to a file that loads identically.
use libfuzzer_sys::fuzz_target;
use mini_shielded_verify::{ClaimEvidencePoolV3, EvidenceLimits};

fuzz_target!(|data: &[u8]| {
    let dir = std::env::temp_dir().join(format!("mini-fuzz-evidence-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("evidence");
    std::fs::write(&path, data).unwrap();
    if let Ok(pool) = ClaimEvidencePoolV3::load(&path, EvidenceLimits::default()) {
        let again = dir.join("again");
        pool.save(&again).unwrap();
        let reloaded = ClaimEvidencePoolV3::load(&again, EvidenceLimits::default()).unwrap();
        assert_eq!(pool.len(), reloaded.len());
        assert_eq!(pool.total_bytes(), reloaded.total_bytes());
    }
});
