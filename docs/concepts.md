# Concepts

## Projects

A project is a directory containing a `.mkrk` SQLite database. All file
tracking, categories, tags, and audit logs are stored in this database.

```sh
mkrk init
```

Creates `.mkrk` in the current directory and optionally sets up default
categories (`evidence/**` as immutable, `notes/**` as editable).

## Workspaces

A workspace groups multiple projects under a single root. The workspace
database (`.mksp`) tracks registered projects and shared configuration like
default categories and tool configs.

```sh
mkrk init --workspace projects/
```

Commands run inside a workspace-registered project can access cross-project
references and fall back to workspace-level tool configs.

## Categories

Categories are glob patterns that classify files by path:

```
evidence/**     -> immutable
notes/**        -> editable
evidence/financial/** -> protected
```

When multiple categories match a file, all matching protection levels are
collected and the strictest one wins. Categories cannot loosen protection
inherited from a parent pattern.

### Category matching

Categories use standard glob syntax. A file at `evidence/financial/receipt.pdf`
matches both `evidence/**` and `evidence/financial/**`. The category with the
most specific (longest) pattern is used for display, but protection is resolved
by taking the strictest level across all matches.

## Protection levels

Three levels, from least to most restrictive:

- **editable** — No restrictions. Default for uncategorized files.
- **protected** — Warns before editing but allows it.
- **immutable** — Blocks edits entirely. Sets the filesystem immutable flag
  (`chattr +i` on Linux) so the file cannot be modified even outside `mkrk`.

Protection only tightens through the hierarchy. If a parent category is
immutable, a child category cannot downgrade it to editable.

## Files

Files are ingested into a project with `mkrk ingest`. Ingestion:

1. Copies the file into the project directory
2. Computes a SHA-256 hash
3. Records metadata (name, path, size, MIME type, timestamp)
4. Stores provenance (source path, method, timestamp)
5. Applies the protection level from matching categories
6. Logs the operation in the audit trail

Files are identified by their relative path within the project.

## Tags

Tags are arbitrary string labels attached to files. They serve two purposes:

- **Filtering** — `mkrk list --tag classified` shows only tagged files.
  Reference syntax supports tag filters: `:evidence!classified` returns
  evidence files tagged "classified".
- **Tool resolution** — Tool configs can be scoped to tags, so tagged files
  use different tools than untagged ones.

Tags are orthogonal to categories. A file can have any number of tags
regardless of which category it belongs to.

## Audit log

Every significant operation (ingest, verify, categorize, edit, tool execution)
is recorded in the audit log with a timestamp, operation type, affected file,
user, and optional detail JSON. This provides an evidence trail for
chain-of-custody documentation.

## Integrity verification

`mkrk verify` checks every tracked file against its stored SHA-256 hash and
reports:

- **ok** — Hash matches.
- **modified** — Hash differs. Shows expected and actual hashes.
- **missing** — File not found on disk.
- **skipped** — No hash stored (shouldn't happen for ingested files).

For immutable files, verification also checks whether the filesystem immutable
flag is still set.
