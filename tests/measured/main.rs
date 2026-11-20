//! The module's timing program (`doc/governance/rust-style.md` section
//! 5c): an archive's write held to a time. A plain `cargo test` leaves it
//! out; the landing runs it after every other test of the module, alone,
//! one test at a time (`cargo test --test measured --features test-support
//! -- --test-threads=1`), so what is measured is Xmip's work and not its
//! neighbors':
//!
//! - the first write into a full directory waits for no sweep of it
//!   (ADR-0040, amendment 2026-11-20).

use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};

use file::synced;
use file::temporary::Folder;
use xcore::spent::Spent;
use xmip_core_archive::named::Created;
use xmip_core_archive::whole;

/// The items already in the directory: a listing of them is clearly
/// measurable beside a write. Made empty and synced once, together: about
/// twenty seconds on Windows, a second or two on Linux; made on eight
/// threads, no faster on Windows and slower on Linux (2026-11-20).
const ITEMS: usize = 50_000;
/// The writes after the first, whose slowest is the load a write carries.
const OTHERS: usize = 20;
/// The millisecond rule: Xmip's own share beyond that load.
const BEYOND: Duration = Duration::from_millis(1);

#[test]
fn the_first_write_into_a_full_directory_waits_for_no_sweep() {
    let dir = Folder::new("archive-whole-full");
    let made = Instant::now();
    for item in 0..ITEMS {
        File::create_new(dir.join(format!("item-{item}"))).expect("an item");
    }
    // The items on disk before any write is measured, so the first write's
    // sync is not theirs.
    synced::directory(&dir.join("item-0")).expect("the directory synced");
    println!("{ITEMS} items made in {:?}", made.elapsed());
    // Each write's own share, the processor time it took outside its syncs
    // and its file system calls (`xcore::spent`, the millisecond rule's
    // measure), and its time by the clock.
    let timed = |name: String| {
        let spent = Spent::begin();
        let began = Instant::now();
        let created = {
            let _working = spent.entered();
            whole::create(&dir.join(name), |file| file.write_all(b"whole"))
        };
        assert_eq!(created.expect("created"), Created::Created);
        (spent.own(Duration::ZERO), began.elapsed())
    };
    let (first, first_clock) = timed("first".to_string());
    let others: Vec<(Duration, Duration)> = (0..OTHERS)
        .map(|other| timed(format!("other-{other}")))
        .collect();
    let slowest = others.iter().map(|(own, _)| *own).max().expect("others");
    let slowest_clock = others
        .iter()
        .map(|(_, clock)| *clock)
        .max()
        .expect("others");
    // The folder is removed once the sweep listing it has ended.
    whole::swept(&dir);
    println!(
        "the first write: own {first:?}, clock {first_clock:?}; the slowest of {OTHERS} \
         others: own {slowest:?}, clock {slowest_clock:?}"
    );
    assert!(
        first <= slowest + BEYOND,
        "the first write's own share {first:?}, the slowest of {OTHERS} others' {slowest:?}"
    );
}
