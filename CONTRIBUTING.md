# Contributing to Muckrake

## Building

Requires a stable Rust toolchain.

```sh
cargo build
```

## Testing

```sh
cargo test
```

All tests must pass before committing. Run clippy as well:

```sh
cargo clippy
```

Clippy pedantic and nursery lints are enabled. Zero warnings required.

## Git

- Pre-stable: commits go directly to `main`
- Use conventional commits: `feat:`, `fix:`, `chore:`, `refactor:`, `test:`, `docs:`
- Run tests, clippy, and formatting before committing
- Prefer many small commits over large ones

## Releases

Releases are done through git tags. Do not use `gh release create`.

```sh
# Bump version in Cargo.toml, then:
git tag -a v0.X.0 -m "v0.X.0"
git push origin main --tags
```

## License

BSD 2-Clause. Copyright Bailey "monokrome" Stoner.
