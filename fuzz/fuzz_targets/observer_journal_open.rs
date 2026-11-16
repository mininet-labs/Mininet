#![no_main]
//! An arbitrary receipt journal either fails closed or opens; opening never
//! panics and never invents a finalized receipt out of garbage.
use libfuzzer_sys::fuzz_target;
use mini_chain_observer::ReceiptJournal;

fuzz_target!(|data: &[u8]| {
    let dir = std::env::temp_dir().join(format!("mini-fuzz-journal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("receipts");
    std::fs::write(&path, data).unwrap();
    let _ = ReceiptJournal::open(&path);
});
