# Tools

Tools are external commands (scripts, binaries) that `mkrk` can invoke on
tracked files. They provide extensible analysis, transformation, and processing
without baking specific tools into the core.

## Discovery

Tools are discovered in two ways:

1. **Database configs** — Registered via `mkrk tool add`, stored in the project
   or workspace database. Resolved by action name, file type, scope, and tags.
2. **Convention** — Scripts in the `tools/` directory at the project root.
   `mkrk tool ner` looks for `tools/ner` or `tools/ner.*`.

Database configs take priority. If no config matches, the `tools/` directory is
searched.

## Tool configs

A tool config binds an action name to a command, scoped by file type and
optionally by category scope or tag:

```sh
mkrk tool add analyze --command transcribe.py --file-type wav
mkrk tool add analyze --command ocr.sh --file-type pdf
mkrk tool add analyze --command ner.py --tag classified --file-type "*"
```

### Resolution order

When resolving which tool to run:

1. Check project DB for matching scope (most specific first), then fallback
2. Check workspace DB with the same logic
3. Check tag-based configs in project DB, then workspace DB
4. If multiple candidates match, prompt the user to choose
5. Fall back to `tools/` directory discovery

### Scope chain

For a file at `evidence/financial/receipt.pdf`, the scope chain is:

```
evidence/financial  (most specific)
evidence
(default/null)      (least specific)
```

Each level is checked in order. The first match wins.

## Environment variables

Every tool invocation gets these environment variables:

- `MKRK_PROJECT_ROOT` — Absolute path to the project directory
- `MKRK_PROJECT_DB` — Path to the `.mkrk` database file
- `MKRK_WORKSPACE_ROOT` — Path to the workspace root (if applicable)

### Privacy protection

`mkrk` sets proxy environment variables by default to route traffic through
Tor or similar privacy tools. When registering a tool, `mkrk` warns that it
cannot guarantee the tool respects these variables.

Tool configs can include custom environment variables as JSON:

```sh
mkrk tool add fetch --command aria2c --env '{"http_proxy": "socks5://127.0.0.1:9050"}'
```

If a custom env config removes proxy variables, `mkrk` warns about the privacy
implications and requires confirmation.

## Audit trail

Every tool invocation is logged in the audit trail with the tool name and
list of files passed to it.
