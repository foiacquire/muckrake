# muckrake

Chain-of-custody research management CLI for investigative journalism.

`mkrk` tracks files, enforces integrity, and organizes evidence across projects
and workspaces — designed for journalists who need to prove their documents
haven't been tampered with.

[Community chat on Matrix](https://matrix.to/#/!ryjJeLknBythWVwLkm:matrix.org)

## Build

```
cargo install --path .
```

Binary name is `mkrk`.

## Quick start

```sh
# Initialize a standalone project
mkrk init

# Or initialize a workspace with projects
mkrk init --workspace projects/
mkrk init myproject          # creates projects/myproject/.mkrk

# Ingest a file (copies it, hashes it, tracks it)
mkrk ingest document.pdf --as evidence

# Verify integrity of all tracked files
mkrk verify

# Tag files for organization
mkrk tag document.pdf classified

# View or edit tracked files (respects protection levels)
mkrk view document.pdf
mkrk edit notes.md

# List files, optionally filtered by reference
mkrk list
mkrk list :evidence
mkrk list :evidence!classified
```

## Projects

A project is a directory containing a `.mkrk` SQLite database. All file
tracking, categories, tags, and audit logs are stored in this database.

```sh
mkrk init                    # initialize current directory
mkrk init myproject          # create and initialize ./myproject
```

When inside a workspace, `mkrk init` requires a project name and creates the
project under the workspace's configured projects directory. Without a
workspace, a bare `mkrk init` initializes the current directory.

## Workspaces

A workspace groups multiple projects under a single root. The workspace
database (`.mksp`) tracks registered projects and shared configuration like
default categories and tool configs.

```sh
mkrk init --workspace projects/
mkrk init myproject          # creates projects/myproject/.mkrk, registers it
```

Commands run inside a workspace-registered project can access cross-project
references and fall back to workspace-level tool configs. The `--inbox` flag
creates an inbox directory for staging files before assigning them to projects.

### Scope prefix

The `:scope` prefix before a command overrides which project context a command
runs in, without requiring `cd`:

```sh
mkrk :bailey list            # list files in project "bailey"
mkrk :bailey list :evidence  # list evidence files in project "bailey"
mkrk : list                  # list across entire workspace
mkrk list :evidence          # current project, evidence filter (unchanged)
```

The scope resolves to the project's root directory via workspace DB lookup. All
command handlers work transparently since they discover context from the working
directory.

## Categories

Categories are glob patterns that classify files by path:

```
evidence/**              -> immutable
notes/**                 -> editable
evidence/financial/**    -> protected
```

When multiple categories match a file, all matching protection levels are
collected and the strictest one wins. Categories cannot loosen protection
inherited from a parent pattern.

Categories use standard glob syntax. A file at `evidence/financial/receipt.pdf`
matches both `evidence/**` and `evidence/financial/**`. The category with the
most specific (longest) pattern is used for display, but protection is resolved
by taking the strictest level across all matches.

Custom categories can be defined during init:

```sh
mkrk init --category "documents/**:immutable" --category "drafts/**:editable"
```

## Protection levels

Three levels, from least to most restrictive:

- **editable** — No restrictions. Default for uncategorized files.
- **protected** — Warns before editing but allows it.
- **immutable** — Blocks edits entirely. Sets the filesystem immutable flag
  (`chattr +i` on Linux) so the file cannot be modified even outside `mkrk`.

Protection only tightens through the hierarchy. If a parent category is
immutable, a child category cannot downgrade it to editable.

## Files

Files are ingested into a project with `mkrk ingest`. Ingestion copies the
file, computes a SHA-256 hash, records metadata (name, path, size, MIME type,
timestamp), stores provenance, applies the protection level from matching
categories, and logs the operation in the audit trail.

```sh
mkrk ingest document.pdf --as evidence
mkrk ingest recording.wav --as evidence/audio
```

## Integrity verification

`mkrk verify` checks every tracked file against its stored SHA-256 hash and
reports:

- **ok** — Hash matches.
- **modified** — Hash differs. Shows expected and actual hashes.
- **missing** — File not found on disk.

For immutable files, verification also checks whether the filesystem immutable
flag is still set.

## Tags

Tags are arbitrary string labels attached to files. They serve two purposes:

- **Filtering** — Reference syntax supports tag filters: `:evidence!classified`
  returns evidence files tagged "classified".
- **Tool resolution** — Tool configs can be scoped to tags, so tagged files
  use different tools than untagged ones.

```sh
mkrk tag document.pdf classified
mkrk untag document.pdf classified
mkrk tags                          # list all tags
mkrk tags document.pdf             # list tags on a file
```

## Reference syntax

References identify files across projects and categories. They're used as
arguments to commands like `mkrk list`, `mkrk view`, and `mkrk tool`.

### Bare paths

A plain path (no `:` prefix) resolves against the current project:

```
evidence/report.pdf        # by relative path
report.pdf                 # by file name (path match first, then name)
```

### Structured references

A `:` prefix enters structured resolution:

```
:evidence                  # all files in the evidence category
:evidence.emails           # subcategory: evidence/emails/
:bailey.evidence           # project "bailey", category "evidence"
:                          # all files across workspace
```

Scope levels are separated by `.`. In a project context, the first level is
checked as a category name; if no match and a workspace is available, it's
treated as a project name. In a workspace context, the first level is always a
project name.

### Brace expansion

`{a,b}` expands into multiple names at that level:

```
:{evidence,notes}              # both categories
:{bailey,george}.evidence      # same category in two projects
```

### Tag filters

`!` filters results by tag. Chained `!` groups are ANDed. Commas within a `!`
group are ORed.

```
:evidence!classified           # files tagged "classified"
:evidence!classified!priority  # tagged both (AND)
:evidence!classified,priority  # tagged either (OR)
```

### Glob filters

`/` starts a glob pattern that filters by filename:

```
:evidence/*.pdf                # only PDFs in evidence
:evidence!classified/*.pdf     # classified PDFs
```

### Resolution rules

1. References are unioned — multiple arguments combine results.
2. Duplicate files (same project + file ID) are deduplicated.
3. Project and category names must not contain: `: . / ! { } ,`

## Tools

Tools are external commands (scripts, binaries) that `mkrk` can invoke on
tracked files.

### Discovery

Tools are discovered in two ways:

1. **Database configs** — Registered via `mkrk tool add`, stored in the project
   or workspace database. Resolved by action name, file type, scope, and tags.
2. **Filesystem** — Files in directories matching categories with type `Tools`
   (e.g., `tools/**`). `mkrk tool ner` looks for a file named `ner` or `ner.*`
   in those directories.

Database configs take priority over filesystem discovery.

### Registering tools

```sh
mkrk tool add analyze transcribe.py --file-type wav
mkrk tool add analyze ocr.sh --file-type pdf
mkrk tool add analyze ner.py --tag classified --file-type "*"
```

### Resolution order

1. Check project DB for matching scope (most specific first), then fallback
2. Check workspace DB with the same logic
3. Check tag-based configs in project DB, then workspace DB
4. If multiple candidates match, prompt the user to choose
5. Fall back to filesystem discovery

For a file at `evidence/financial/receipt.pdf`, the scope chain checked is:
`evidence/financial` (most specific), `evidence`, then default/null.

### Listing tools

`mkrk tool list` shows all tools — database-registered configs and filesystem
tools — across the workspace and all projects.

### Environment variables

Every tool invocation gets these environment variables:

- `MKRK_PROJECT_ROOT` — Absolute path to the project directory
- `MKRK_PROJECT_DB` — Path to the `.mkrk` database file
- `MKRK_WORKSPACE_ROOT` — Path to the workspace root (if applicable)

`mkrk` sets proxy environment variables by default to route traffic through
Tor (`socks5h://127.0.0.1:9050`). Tool configs can include custom environment
variables as JSON. Removing proxy variables requires explicit confirmation.

## Audit log

Every significant operation (ingest, verify, categorize, edit, tool execution)
is recorded in the audit log with a timestamp, operation type, affected file,
user, and optional detail JSON. This provides an evidence trail for
chain-of-custody documentation.

## License

BSD-2-Clause. Copyright Bailey "monokrome" Stoner.
