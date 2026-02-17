pub mod engine;

pub use engine::{evaluate_rules, RuleContext, RuleEvent};

use std::collections::HashSet;

use crate::context::Context;

/// Fire rules for a given event without requiring callers to build
/// [`RuleContext`] / `HashSet` boilerplate.
pub fn fire_rules(ctx: &Context, event: &RuleEvent<'_>) {
    let Ok((project_root, project_db)) = ctx.require_project() else {
        return;
    };
    let (workspace_root, workspace_db) = crate::cli::ingest::workspace_from_ctx(ctx);
    let rule_ctx = RuleContext {
        project_root,
        project_db,
        workspace_root,
        workspace_db,
    };
    let mut fired = HashSet::new();
    let _ = evaluate_rules(event, &rule_ctx, &mut fired);
}
