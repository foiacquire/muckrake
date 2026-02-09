/// Extract a scope prefix (`:name` or `:`) from CLI args before clap parsing.
///
/// If `args[1]` starts with `:` and is not a known clap subcommand,
/// it is removed from the arg list and returned as the scope string
/// (without the leading `:`).
///
/// Returns `(scope, filtered_args)` where scope is:
/// - `None` — no scope prefix present
/// - `Some("")` — bare `:` (workspace scope)
/// - `Some("name")` — named project scope
pub fn extract_scope(args: Vec<String>) -> (Option<String>, Vec<String>) {
    if args.len() < 2 {
        return (None, args);
    }

    let candidate = &args[1];
    if !candidate.starts_with(':') {
        return (None, args);
    }

    let scope = candidate[1..].to_string();
    let mut filtered = Vec::with_capacity(args.len() - 1);
    filtered.push(args[0].clone());
    filtered.extend_from_slice(&args[2..]);
    (Some(scope), filtered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(strs: &[&str]) -> Vec<String> {
        strs.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn no_scope_prefix() {
        let (scope, filtered) = extract_scope(args(&["mkrk", "list"]));
        assert!(scope.is_none());
        assert_eq!(filtered, args(&["mkrk", "list"]));
    }

    #[test]
    fn project_scope() {
        let (scope, filtered) = extract_scope(args(&["mkrk", ":bailey", "list"]));
        assert_eq!(scope.as_deref(), Some("bailey"));
        assert_eq!(filtered, args(&["mkrk", "list"]));
    }

    #[test]
    fn workspace_scope() {
        let (scope, filtered) = extract_scope(args(&["mkrk", ":", "list"]));
        assert_eq!(scope.as_deref(), Some(""));
        assert_eq!(filtered, args(&["mkrk", "list"]));
    }

    #[test]
    fn scope_with_trailing_refs() {
        let (scope, filtered) =
            extract_scope(args(&["mkrk", ":bailey", "list", ":evidence"]));
        assert_eq!(scope.as_deref(), Some("bailey"));
        assert_eq!(filtered, args(&["mkrk", "list", ":evidence"]));
    }

    #[test]
    fn bare_scope_no_subcommand() {
        let (scope, filtered) = extract_scope(args(&["mkrk", ":bailey"]));
        assert_eq!(scope.as_deref(), Some("bailey"));
        assert_eq!(filtered, args(&["mkrk"]));
    }

    #[test]
    fn flag_not_treated_as_scope() {
        let (scope, filtered) = extract_scope(args(&["mkrk", "--help"]));
        assert!(scope.is_none());
        assert_eq!(filtered, args(&["mkrk", "--help"]));
    }

    #[test]
    fn no_args_at_all() {
        let (scope, filtered) = extract_scope(args(&["mkrk"]));
        assert!(scope.is_none());
        assert_eq!(filtered, args(&["mkrk"]));
    }

    #[test]
    fn empty_args() {
        let (scope, filtered) = extract_scope(vec![]);
        assert!(scope.is_none());
        assert!(filtered.is_empty());
    }
}
