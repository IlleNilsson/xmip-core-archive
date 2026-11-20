//! A file created whole at its name, or not at all, and never over another
//! (ADR-0040, amendments 2026-11-19 and 2026-11-20): what an archive on a
//! file system writes with — the file archive its bytes and its metadata,
//! the Parquet archive its file.
//!
//! The file is written beside its name first, under a name no item takes —
//! the name, the mark `.xmip-archiving.`, the process's id and a count of
//! its writes — held by this process from the moment it is made until it
//! is closed (`file::held`), and synced. It is then hard-linked to its
//! name, still held: a link is refused where the name is taken (`link(2)`,
//! `CreateHardLinkW`), so what is there is never replaced, as a rename
//! would replace it. The written name is removed and the directory synced
//! (`file::synced::directory`), so the name is on disk with the bytes
//! before the archive answers for them; a directory made for the file is
//! synced into the one it is in. Where anything fails the written file is
//! removed.
//!
//! A process that dies between the write and the removal leaves the file
//! under its mark, never at the name — unlinked, or a second name of a
//! file linked. A process's first write into a directory starts a sweep of
//! it, on a thread of its own beside the write, which removes every file
//! there under the mark that no process holds (`file::held::
//! remove_unheld`), as a node starting removes what its sends left: an
//! archive has no start of its own, so its start in a directory is its
//! first write there. The write waits for no listing of the directory,
//! which holds a file for each item. The operating system says which file
//! is left: it has closed everything a dead process held, and holds what a
//! live one is writing, in this process or another, which is never
//! removed. No process id is read to decide it, so a process given a dead
//! one's id finds the name free.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use file::{held, synced};
use xcore::lock::Locked;
use xcore::spent::{on_io, on_sync};
use xcore::wake::Wake;

use crate::named::Created;

/// What a file written beside its name carries between that name and the
/// process's id.
const MARK: &str = ".xmip-archiving";

/// The count that keeps two writes of one process apart.
static WRITES: AtomicU64 = AtomicU64::new(0);

/// The directories this process has written in, each swept from its first
/// write there, and whether that sweep has ended.
static SWEPT: Mutex<BTreeMap<PathBuf, bool>> = Mutex::new(BTreeMap::new());

/// Told as each sweep ends.
static ENDED: Wake = Wake::new();

/// A file at `path` holding what `write` writes: [`Created::Created`] once
/// it is on disk under its name, [`Created::Exists`] where `path` was
/// taken, which is left as it was.
///
/// # Errors
/// `path` names no file, or the file system refused the write, a sync or
/// the link; nothing is left at `path`.
pub fn create(path: &Path, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<Created> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let directory = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let first = match SWEPT.locked().entry(directory.to_path_buf()) {
        Entry::Vacant(first) => {
            first.insert(false);
            true
        }
        Entry::Occupied(_) => false,
    };
    if first {
        sweeping(directory);
    }
    let writing = path.with_file_name(format!(
        "{name}{MARK}.{}.{}",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    // The file made, linked, closed and its written name removed are the
    // file system's time, as its syncs are the disk's: each runs as an I/O
    // (`xcore::spent::on_io`), never Xmip's own share.
    let linked = on_io(|| opened(&writing)).and_then(|mut file| {
        write(&mut file)?;
        on_sync(|| file.sync_all())?;
        // Linked while it is held, so no sweep takes it first; closed after.
        on_io(|| {
            let linked = match fs::hard_link(&writing, path) {
                Ok(()) => Ok(Created::Created),
                Err(taken) if taken.kind() == io::ErrorKind::AlreadyExists => Ok(Created::Exists),
                Err(cause) => Err(cause),
            };
            drop(file);
            linked
        })
    });
    // Closed by now: a file held on Windows is removed by nobody, its
    // writer included. The name the file was written under goes whatever
    // the link did; where its removal fails, it is left under its mark.
    let _ = on_io(|| fs::remove_file(&writing));
    if linked? == Created::Exists {
        return Ok(Created::Exists);
    }
    on_sync(|| synced::directory(path))?;
    Ok(Created::Created)
}

/// Of the files in `directory`, every one under the mark that no process
/// holds removed: what a process that died left there. What cannot be
/// listed or removed is left as it is, for the next process's first write
/// there; the write that sweeps needs none of it.
fn sweep(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let marked = entry.file_name().to_str().is_some_and(is_marked);
        if marked && entry.file_type().is_ok_and(|kind| kind.is_file()) {
            let _ = held::remove_unheld(&entry.path());
        }
    }
}

/// `directory` swept on a thread of its own, beside the write that starts
/// it: a directory holds a file for each item, and the write waits for no
/// listing of them. The sweep is safe beside every write, this one
/// included: a held file is never removed. Where no thread can be started,
/// the directory is swept from this process's next write there.
fn sweeping(directory: &Path) {
    let swept = directory.to_path_buf();
    let started = std::thread::Builder::new()
        .name("xmip-archive-sweep".to_string())
        .spawn(move || {
            sweep(&swept);
            ended(&swept);
        });
    if started.is_err() {
        SWEPT.locked().remove(directory);
        ENDED.notify_all();
    }
}

/// `directory`'s sweep ended, and whoever waits for it told.
fn ended(directory: &Path) {
    if let Some(done) = SWEPT.locked().get_mut(directory) {
        *done = true;
    }
    ENDED.notify_all();
}

/// Waits until this process's sweep of `directory` has ended; at once
/// where none was started. For tests, which assert what a sweep removed.
#[cfg(any(test, feature = "test-support"))]
pub fn swept(directory: &Path) {
    let waiting = SWEPT.locked();
    drop(ENDED.wait_while(&SWEPT, waiting, |swept| {
        swept.get(directory) == Some(&false)
    }));
}

/// Whether `name` is a file written beside its name: the name, the mark,
/// the process's id and the count.
fn is_marked(name: &str) -> bool {
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let mut parts = name.rsplitn(3, '.');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(count), Some(id), Some(rest)) => {
            digits(count)
                && digits(id)
                && rest.strip_suffix(MARK).is_some_and(|name| !name.is_empty())
        }
        _ => false,
    }
}

/// `path` created new and held, and its directory with it where that is
/// not there yet, each directory made synced into the one it is in.
fn opened(path: &Path) -> io::Result<File> {
    match held::create(path) {
        Err(missing) if missing.kind() == io::ErrorKind::NotFound => {
            let Some(directory) = path.parent() else {
                return Err(missing);
            };
            let made: Vec<&Path> = directory
                .ancestors()
                .take_while(|ancestor| !ancestor.as_os_str().is_empty() && !ancestor.is_dir())
                .collect();
            fs::create_dir_all(directory)?;
            for directory in made.iter().rev() {
                on_sync(|| synced::directory(directory))?;
            }
            held::create(path)
        }
        opened => opened,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use file::temporary::Folder;
    use std::io::Write;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("listing")
            .map(|entry| {
                entry
                    .expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_file_is_created_whole_in_a_directory_made_for_it_and_never_written_over() {
        let dir = Folder::new("archive-whole-never-over");
        let path = dir.join("json").join("order");
        let created = create(&path, |file| file.write_all(b"first")).expect("created");
        assert_eq!(created, Created::Created);
        let again = create(&path, |file| file.write_all(b"second")).expect("taken");
        assert_eq!(again, Created::Exists);
        assert_eq!(fs::read(&path).expect("the file"), b"first");
        assert_eq!(names(&dir.join("json")), ["order"], "nothing beside it");
    }

    #[test]
    fn a_file_left_under_its_mark_is_removed_by_the_next_create_in_its_directory() {
        let dir = Folder::new("archive-whole-left");
        // Left by a process that died between its write and its link, held
        // by no process: one never linked, and one linked to its name.
        fs::write(dir.join("order.xmip-archiving.4294967295.0"), b"half").expect("left");
        fs::write(dir.join("kept"), b"whole").expect("an item");
        fs::hard_link(
            dir.join("kept"),
            dir.join("kept.xmip-archiving.4294967295.1"),
        )
        .expect("left linked");
        // Names a sweep takes for no file under the mark.
        for name in [
            "order.xmip-archiving.1",
            "order.xmip-archiving.x.1",
            ".xmip-archiving.1.1",
        ] {
            fs::write(dir.join(name), b"not ours").expect("another file");
        }
        let created = create(&dir.join("order"), |file| file.write_all(b"first"));
        assert_eq!(created.expect("created"), Created::Created);
        swept(&dir);
        assert_eq!(
            names(&dir),
            [
                ".xmip-archiving.1.1",
                "kept",
                "order",
                "order.xmip-archiving.1",
                "order.xmip-archiving.x.1"
            ]
        );
        assert_eq!(fs::read(dir.join("kept")).expect("the item"), b"whole");
    }

    #[test]
    fn a_file_a_live_write_holds_under_its_mark_is_never_removed() {
        let dir = Folder::new("archive-whole-live");
        // Another write, in this process or another, still writing.
        let writing = dir.join("order.xmip-archiving.4294967295.0");
        let mut live = held::create(&writing).expect("held");
        live.write_all(b"half").expect("written");
        let created = create(&dir.join("other"), |file| file.write_all(b"first"));
        assert_eq!(created.expect("created"), Created::Created);
        swept(&dir);
        assert_eq!(names(&dir), ["order.xmip-archiving.4294967295.0", "other"]);
        drop(live);
        sweep(&dir);
        assert_eq!(names(&dir), ["other"], "closed, it is left and removed");
    }

    #[test]
    fn a_write_failing_half_way_leaves_nothing_at_the_name() {
        let dir = Folder::new("archive-whole-half-way");
        let path = dir.join("order");
        let failed = create(&path, |file| {
            file.write_all(b"fir")?;
            Err(io::Error::other("the disk is full"))
        });
        assert!(failed.is_err());
        assert!(names(&dir).is_empty(), "{:?}", names(&dir));
    }
}
