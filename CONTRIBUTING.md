# Contributing to Muckrake

## Development Setup

### Prerequisites

- Rust (stable)
- Node.js 18+ and pnpm
- SQLite

### Building

```bash
# Build all Rust crates
cargo build

# Build the web frontend
cd crates/muckrake-web/frontend
pnpm install
pnpm run build
```

### Development

```bash
# Run the web server (serves API and static files)
cargo run -p muckrake-web

# Run frontend dev server with hot reload
cd crates/muckrake-web/frontend
pnpm run dev
```

## Code Style

### Rust

- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Follow standard Rust naming conventions

### TypeScript/React

- Use TypeScript strict mode
- Use vanilla-extract for styling (CSS-in-JS)
- Follow React hooks best practices
- Ensure ARIA accessibility attributes on interactive elements

### Commits

Use conventional commits:

- `feat:` - New features
- `fix:` - Bug fixes
- `refactor:` - Code refactoring
- `chore:` - Build, tooling, dependencies
- `docs:` - Documentation
- `test:` - Tests

## Internationalization

All user-facing strings should use react-i18next:

```typescript
const { t } = useTranslation();
// Use: t('namespace.key')
```

Add translations to `crates/muckrake-web/frontend/src/i18n/locales/`.

## Releases

### Versioning

This project uses [Semantic Versioning](https://semver.org/):

- **MAJOR**: Breaking API changes
- **MINOR**: New features, backwards compatible
- **PATCH**: Bug fixes, backwards compatible

### Release Process

1. **Update version numbers**
   ```bash
   # Update Cargo.toml versions for affected crates
   # Update package.json version for frontend
   ```

2. **Update changelog**
   - Document notable changes
   - Group by: Added, Changed, Fixed, Removed

3. **Create release commit**
   ```bash
   git add -A
   git commit -m "chore: release v0.x.x"
   ```

4. **Tag the release**
   ```bash
   git tag -a v0.x.x -m "Release v0.x.x"
   git push origin main --tags
   ```

5. **Build release artifacts**
   ```bash
   cargo build --release
   cd crates/muckrake-web/frontend && pnpm run build
   ```

### Crate Publishing

For publishing to crates.io (when ready):

```bash
# Publish in dependency order
cargo publish -p muckrake-core
cargo publish -p muckrake-web
```

## Project Structure

```
muckrake/
├── crates/
│   ├── muckrake-core/     # Core library (entities, relationships, storage)
│   └── muckrake-web/      # Web server and API
│       ├── src/           # Rust axum server
│       └── frontend/      # React + Vite frontend
│           └── src/
│               ├── components/
│               ├── styles/      # vanilla-extract CSS
│               ├── i18n/        # Translations
│               └── utils/
```

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.
