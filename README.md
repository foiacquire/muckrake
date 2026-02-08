# muckrake

Chain-of-custody research management CLI for investigative journalism.

`mkrk` tracks files, enforces integrity, and organizes evidence across projects
and workspaces — designed for journalists who need to prove their documents
haven't been tampered with.

## Build

```
cargo install --path .
```

Binary name is `mkrk`.

## Quick start

```sh
# Initialize a project
mkrk init

# Ingest a file (copies it, hashes it, tracks it)
mkrk ingest document.pdf --category evidence

# Verify integrity of all tracked files
mkrk verify

# Tag files for organization
mkrk tag document.pdf classified

# View or edit tracked files (respects protection levels)
mkrk view document.pdf
mkrk edit notes.md

# List files, optionally filtered by tag
mkrk list --tag classified
```

## Concepts

**Categories** — Glob patterns that match files and assign protection levels.
`evidence/**` can be immutable, `notes/**` can be editable. The most specific
matching category wins.

**Protection levels** — `editable` (default), `protected` (warns on edit),
`immutable` (blocks edits, sets filesystem immutable flag).

**Tags** — Arbitrary labels attached to files. Used for filtering and tool
resolution.

**Tool configs** — Register external tools (scripts, commands) scoped by
category, file type, or tag. `mkrk` sets proxy environment variables by default
to prevent accidental information leaks.

**Workspaces** — Group multiple projects under a single `.mksp` database.
Cross-project references use `:project.category` syntax.

**Integrity** — Every ingested file gets a SHA-256 hash. `mkrk verify` checks
all files against stored hashes and reports modifications or deletions.

See [docs/](docs/) for detailed documentation.

## License

BSD-2-Clause. Copyright Bailey "monokrome" Stoner.
