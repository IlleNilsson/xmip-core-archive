# xmip-core-archive

What an archive is to Xmip, and what every archive technology shares. An
`ArchiveStore` takes an `ArchiveItem` — a data type, an identifier, the bytes
and the metadata pairs — and answers an `ArchiveReceipt` it can restore the
item from. The metadata text a column holds, the sidecar a name-keyed store
keeps, the timestamp, the name-keyed layout and its rule, and the receipt
checksum live here rather than in ten copies (ADR-0044), and so does
the one archive in a SQL server's table: `sql::SqlArchive<D>`, the store, the
row, its INSERT and SELECT and the receipt, given each server's
`sql::Database` — the SQL dialect it speaks, from `xmip-core-library-sql`,
the one its session reads where that may differ (`Database::dialect`:
`MariaDB` under `NO_BACKSLASH_ESCAPES`), and its connection, which are all
the technologies keep.

An archive whose items sit under names — a directory, a bucket, a
container — keeps an item as two objects: its bytes at
`<prefix>/<data_type>/<identifier>` and its metadata, a TOML sidecar
(`sidecar`), at `<prefix>/.meta/<data_type>/<identifier>` (`layout`); a
data type beginning with `.` is refused. **An archived item is never
written over** (ADR-0040, amendment 2026-11-19): `named` is that rule once
for every such store. The store implements `Named` — create an object only
where none is, read one, a key's location and back — and `named::archive`
creates the metadata first, as the claim, then the bytes, and answers the
receipt only after both: an identifier archived again with the same bytes
answers the same receipt, with other bytes is refused, and an archive
stopped between the two is completed by its repeat. A restore reads the
metadata first and refuses a name without it. On a file system a file is
created whole and never over another by `whole`: written beside its name,
synced, hard-linked to the name and its directory synced
(`file::synced::directory`, from `xmip-core-library-file`).

Archiving is the last thing Xmip does with a piece of data: the store keeps
it, the archive owner decides what becomes of it, Xmip never deletes
(ADR-0040). Archive moves already-retained data on a schedule and no Message
waits for it, which is why it is an Operation and retention is not
(`repository-model.md` section 1).

Each store is a technology mounted under this repository, riding on the
transport technology of the same name for its wire client; `architecture.toml`
names them.
