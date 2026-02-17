#[allow(dead_code)]
mod iden;
pub mod project;
mod schema;
pub mod workspace;

pub use project::{
    ProjectDb, TagToolConfigParams, TagToolConfigRow, ToolConfigParams, ToolConfigRow,
};
pub use workspace::{ProjectRow, WorkspaceDb};

/// Try WAL journal mode (best performance for
/// concurrent reads), fall back to DELETE mode on filesystems that don't
/// support the required shared-memory semantics (e.g. NFS on macOS).
/// Always enables foreign key enforcement.
pub(crate) fn configure_conn(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    if conn.execute_batch("PRAGMA journal_mode=WAL;").is_err() {
        conn.execute_batch("PRAGMA journal_mode=DELETE;")?;
    }
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(())
}
