# Reference syntax

References identify files across projects and categories. They're used as
arguments to commands like `mkrk tool`, `mkrk view`, and `mkrk list`.

## Bare paths

A plain path (no `:` prefix) resolves against the current project:

```
evidence/report.pdf    # by relative path
report.pdf             # by file name
```

Bare paths try path match first, then name match.

## Structured references

A `:` prefix enters structured resolution:

```
:evidence              # all files in the evidence category
:evidence.emails       # subcategory: evidence/emails/
:bailey.evidence       # project "bailey", category "evidence"
```

### Scope levels

`.` separates scope levels. Resolution depends on context:

- **In a project**: first level is checked as a category name. If no matching
  category exists and a workspace is available, it's treated as a project name.
- **In a workspace**: first level is always a project name.

```
:evidence              # category in current project
:bailey                # project "bailey" (workspace context)
:bailey.evidence       # project "bailey", category "evidence"
:bailey.evidence.emails  # project, category, subcategory
```

### Brace expansion

`{a,b}` expands into multiple names at that level:

```
:{evidence,notes}           # both categories
:{bailey,george}.evidence   # same category in two projects
```

### Tag filters

`!` filters results by tag. Chained `!` groups are ANDed. Commas within a `!`
group are ORed.

```
:evidence!classified              # files tagged "classified"
:evidence!classified!priority     # tagged both (AND)
:evidence!classified,priority     # tagged either (OR)
```

### Glob filters

`/` starts a glob pattern that filters by filename:

```
:evidence/*.pdf          # only PDFs in evidence
:evidence!classified/*.pdf  # classified PDFs
```

## Resolution rules

1. References are unioned — multiple arguments combine results.
2. Duplicate files (same project + file ID) are deduplicated.
3. Single file = collection of one. All commands work with collections.
4. Complex boolean logic uses multiple arguments (implicit OR) rather than
   parenthesized expressions.

## Reserved characters

Project and category names must not contain: `: . / ! { } ,`
