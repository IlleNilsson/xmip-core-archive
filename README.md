# xmip-core-archive

What an archive is to Xmip, and what every archive technology shares. An
`ArchiveStore` takes an `ArchiveItem` — a data type, an identifier, the bytes
and the metadata pairs — and answers an `ArchiveReceipt` it can restore the
item from. The metadata text, the timestamp, the name-keyed layout and the
receipt checksum live here rather than in ten copies (ADR-0044), and so does
the one archive in a SQL server's table: `sql::SqlArchive<D>`, the store, the
row, its INSERT and SELECT and the receipt, given each server's `sql::Dialect`
— SQL Server's, `MySQL`'s and `PostgreSQL`'s are all their technologies keep.

Archiving is the last thing Xmip does with a piece of data: the store keeps
it, the archive owner decides what becomes of it, Xmip never deletes
(ADR-0040). Archive moves already-retained data on a schedule and no Journey
waits for it, which is why it is an Operation and retention is not
(`repository-model.md` section 1).

Each store is a technology mounted under this repository, riding on the
transport technology of the same name for its wire client; `architecture.toml`
names them.
