# Preserved divergent lifecycle migration lineage

These three files are exact SQL bytes from committed lifecycle `3df4c91b`. They are not executed
by the canonical migrator. Runtime read-only admission uses their SHA-384 checksums to identify
the divergent SQLite49–51 history and refuse it without changing source tables or SQLx history.

Canonical attachment/storage migrations1–53 and PostgreSQL1–7 are unchanged. New canonical
SQLite54/PostgreSQL8 add the publication/history tables. No automatic in-place conversion of an
already migrated lifecycle database is supported. Preserve that database and its original backup;
do not edit its applied versions/checksums, rename rows, delete history or copy data into an empty
replacement as an implicit repair. A separately reviewed lineage conversion is required.
