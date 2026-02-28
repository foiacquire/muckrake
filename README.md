# muckrake

**IMPORTANT:** This project is pre-stable. It is pre-alpha software. It is
designed as much as possible to respect and even reinforce the safety of your
data. With that said, bugs will exist. Do NOT use it on mission critical data
without backups. It is not stable enough to recommend without backups.

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

# Ingest files (hashes, fingerprints, tracks)
mkrk ingest

# Verify integrity of all tracked files
mkrk verify

# Tag files for organization
mkrk tag document.pdf classified

# Read file contents
mkrk read evidence/report.pdf

# List files, optionally filtered by reference
mkrk list
mkrk list :evidence
mkrk list :evidence!classified

# Track file state through pipelines
mkrk pipeline add --name editorial --states draft,review,published
mkrk pipeline attach --pipeline editorial --category evidence
mkrk sign evidence/report.pdf review --pipeline editorial
mkrk state evidence/report.pdf
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

Files are ingested into a project with `mkrk ingest`. Ingestion computes a
SHA-256 hash and a chunk-based fingerprint, records metadata (name, path, size,
MIME type, timestamp), stores provenance, applies the protection level from
matching categories, and logs the operation in the audit trail.

```sh
mkrk ingest
mkrk ingest document.pdf --as evidence
mkrk ingest recording.wav --as evidence/audio
```

## Integrity verification

`mkrk` uses a two-tier integrity model:

- **Fingerprint** (fast) — 64KB chunks hashed with BLAKE3. Used for bulk
  operations like tag queries and listings. Catches file swaps and most
  modifications at minimal I/O cost.
- **SHA-256** (full) — Cryptographic hash of entire file contents. Used for
  single-file operations where authenticity matters: verify, sign, view, edit.

`mkrk verify` checks every tracked file against its stored SHA-256 hash and
reports:

- **ok** — Hash matches.
- **modified** — Hash differs. Shows expected and actual hashes.
- **missing** — File not found on disk.

For immutable files, verification also checks whether the filesystem immutable
flag is still set.

Tag queries verify fingerprints by default. Use `--no-hash-check` to skip
verification for faster bulk operations.

## Reading files

`mkrk read` streams file contents to stdout:

```sh
mkrk read evidence/report.pdf
mkrk read :evidence/*.txt         # multiple files via reference
mkrk read report.txt --path       # show file path before content
mkrk read report.txt --raw        # no color or decoration
```

Binary files are detected automatically and display size instead of dumping
contents. Files that exist on disk but aren't tracked are auto-ingested.

## Tags

Tags are arbitrary string labels attached to files. They serve two purposes:

- **Filtering** — Reference syntax supports tag filters: `:evidence!classified`
  returns evidence files tagged "classified".
- **Tool resolution** — Tool configs can be scoped to tags, so tagged files
  use different tools than untagged ones.

Tagging always fingerprints the file, binding the tag to the file's content.
Querying by tag verifies the fingerprint against disk to detect tampering.

```sh
mkrk tag document.pdf classified
mkrk untag document.pdf classified
mkrk tags                          # list all tags
mkrk tags document.pdf             # list tags on a file
```

## Pipelines

Pipelines are named state machines that track file progression through stages.

```sh
mkrk pipeline add --name editorial --states draft,review,published
mkrk pipeline list
mkrk pipeline remove editorial
```

States form an ordered chain. The first state is the initial state (no
transition into it). By default, transitions are linear — each non-initial
state requires a sign with its own name. Custom transitions support multi-sign
gates:

```sh
mkrk pipeline add --name approval \
  --states draft,reviewed,published \
  --transitions '{"reviewed":["editor"],"published":["editor","legal"]}'
```

Here, reaching `published` requires both `editor` and `legal` signs.

### Attaching pipelines

Pipelines attach to categories or tags. A file inherits all pipelines from its
matching categories and tags:

```sh
mkrk pipeline attach --pipeline editorial --category evidence
mkrk pipeline attach --pipeline classification --tag classified
mkrk pipeline detach --pipeline editorial --category evidence
```

## Signs

Signs are hash-bound attestations that a file has reached a pipeline stage.
They bind to the file's content — if the file is modified after signing, the
sign goes stale and the file regresses in the pipeline.

```sh
mkrk sign evidence/report.pdf review --pipeline editorial
mkrk unsign evidence/report.pdf review --pipeline editorial
mkrk signs                            # list all signs
mkrk signs evidence/report.pdf        # signs on a specific file
```

Optional GPG signatures provide cryptographic proof of who signed:

```sh
mkrk sign evidence/report.pdf review --pipeline editorial --gpg
```

### File state

A file's state in a pipeline is derived from its valid signs each time it's
queried — state is computed, not stored. Revoking or staling a sign instantly
changes the file's computed state.

```sh
mkrk state evidence/report.pdf
mkrk state :evidence --pipeline editorial
```

## Rules

Rules are event-driven automations: when a trigger event occurs and filters
match, an action fires.

```sh
mkrk rule add --name auto-tag-pdfs \
  --on ingest --file-type pdf \
  --action add-tag --tag needs-ocr

mkrk rule add --name sign-on-review \
  --on state-change --trigger-pipeline editorial --trigger-state reviewed \
  --action run-tool --tool notify-editor

mkrk rule list
mkrk rule remove auto-tag-pdfs
mkrk rule enable auto-tag-pdfs
mkrk rule disable auto-tag-pdfs
```

### Trigger events

`ingest`, `tag`, `untag`, `categorize`, `sign`, `state-change`,
`project-enter`, `workspace-enter`

### Actions

`run-tool`, `add-tag`, `remove-tag`, `sign`, `unsign`, `attach-pipeline`,
`detach-pipeline`

### Filters

All optional. Missing filter matches everything. Multiple filters are ANDed.

- `--category` — File must be in this category
- `--mime-type` — Wildcard supported (e.g., `image/*`)
- `--file-type` — File extension filter
- `--trigger-tag` — Which tag triggered the event (for tag/untag events)
- `--trigger-pipeline`, `--trigger-sign`, `--trigger-state` — For sign and
  state-change events

Rules fire in priority order (lower first) and each rule fires at most once per
event to prevent loops.

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

Every significant operation (ingest, verify, categorize, edit, tool execution,
sign, rule fire) is recorded in the audit log with a timestamp, operation type,
affected file, user, and optional detail JSON. This provides an evidence trail
for chain-of-custody documentation.

## License

BSD-2-Clause. Copyright Bailey "monokrome" Stoner.
