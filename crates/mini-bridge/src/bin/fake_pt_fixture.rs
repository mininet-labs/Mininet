//! A fake Tor PT v1 client transport, used only by
//! `pt_process.rs`'s own tests to exercise `PtProcessManager::launch`
//! end to end without any real, external circumvention binary — per the
//! research report's explicit "no real PT dependency yet" scope for this
//! milestone (`docs/research/
//! BRIDGE_ADAPTER_INTEGRATION_RESEARCH_20260715.md` §18/§24 PR2).
//!
//! Prints a valid, minimal PT v1 startup handshake to stdout, then idles
//! until the parent terminates it. Never shipped or referenced outside
//! this crate's own test suite.

use std::thread;
use std::time::Duration;

fn main() {
    println!("VERSION 1");
    println!("CMETHOD obfs4 socks5 127.0.0.1:41213");
    println!("CMETHODS DONE");

    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}
