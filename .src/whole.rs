//! A file created whole at its name, or not at all, and never over another
//! (ADR-0040, amendment 2026-11-19): what an archive on a file system
//! writes with — the file archive its bytes and its metadata, the Parquet
//! archive its file.
//!
//! The file is written beside its name first, under a name no item takes —
//! the name, the mark `.xmip-archiving.`, the process's id and a count of
//! its writes — and synced. It is then hard-linked to its name: a link is
//! refused where the name is taken (`link(2)`, `CreateHardLinkW`), so what
//! is there is never replaced, as a rename would replace it. The written
//! name is removed and the directory synced (`file::synced::directory`),
//! so the name is on disk with the bytes before the archive answers for
//! them; a directory made for the file is synced into the one it is in.
//! Where anything fails the written file is removed; a process that dies
//! between the write and the link leaves it under its mark, never at the
//! name.

use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use file::synced;
use xcore::spent::on_sync;

use crate::named::Created;

/// The count that keeps two writes of one process apart.
static WRITES: AtomicU64 = AtomicU64::new(0);

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
    let writing = path.with_file_name(format!(
        "{name}.xmip-archiving.{}.{}",
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed)
    ));
    let linked = opened(&writing)
        .and_then(|mut file| {
            write(&mut file)?;
            on_sync(|| file.sync_all())
        })
        .and_then(|()| match fs::hard_link(&writing, path) {
            Ok(()) => Ok(Created::Created),
            Err(taken) if taken.kind() == io::ErrorKind::AlreadyExists => Ok(Created::Exists),
            Err(cause) => Err(cause),
        });
    // Closed by now: Windows removes no file that is still open. The name
    // the file was written under goes whatever the link did; where its
    // removal fails, it is left under its mark.
    let _ = fs::remove_file(&writing);
    if linked? == Created::Exists {
        return Ok(Created::Exists);
    }
    on_sync(|| synced::directory(path))?;
    Ok(Created::Created)
}

/// `path` created new, and its directory with it where that is not there
/// yet, each directory made synced into the one it is in.
fn opened(path: &Path) -> io::Result<File> {
    match File::create_new(path) {
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
            File::create_new(path)
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
