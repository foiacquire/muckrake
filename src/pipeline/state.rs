use std::collections::HashSet;

use crate::models::{Pipeline, Sign};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub current_state: String,
    pub stale_signs: Vec<String>,
}

pub fn derive_file_state(pipeline: &Pipeline, all_signs: &[Sign], current_hash: &str) -> FileState {
    let valid_sign_names: HashSet<&str> = all_signs
        .iter()
        .filter(|s| s.is_valid(current_hash))
        .map(|s| s.sign_name.as_str())
        .collect();

    let stale_signs: Vec<String> = all_signs
        .iter()
        .filter(|s| !s.is_valid(current_hash) && s.revoked_at.is_none())
        .map(|s| s.sign_name.clone())
        .collect();

    let mut current_state = pipeline.states[0].clone();

    for state in pipeline.states.iter().skip(1) {
        let Some(required) = pipeline.transitions.get(state) else {
            break;
        };

        let all_satisfied = required
            .iter()
            .all(|sign_name| valid_sign_names.contains(sign_name.as_str()));

        if all_satisfied {
            current_state.clone_from(state);
        } else {
            break;
        }
    }

    FileState {
        current_state,
        stale_signs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Pipeline;
    use std::collections::HashMap;

    fn make_pipeline(states: &[&str], transitions: HashMap<String, Vec<String>>) -> Pipeline {
        Pipeline {
            id: Some(1),
            name: "test".to_string(),
            states: states.iter().map(|s| (*s).to_string()).collect(),
            transitions,
        }
    }

    fn make_sign(name: &str, hash: &str, revoked: bool) -> Sign {
        Sign {
            id: Some(1),
            pipeline_id: 1,
            file_id: 1,
            file_hash: hash.to_string(),
            sign_name: name.to_string(),
            signer: "alice".to_string(),
            signed_at: "2025-06-01T00:00:00Z".to_string(),
            signature: None,
            revoked_at: if revoked {
                Some("2025-06-02T00:00:00Z".to_string())
            } else {
                None
            },
        }
    }

    fn linear_pipeline() -> Pipeline {
        let states = vec!["draft", "review", "published"];
        let transitions = Pipeline::default_transitions(
            &states.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
        );
        make_pipeline(&states, transitions)
    }

    #[test]
    fn no_signs_returns_initial_state() {
        let pipeline = linear_pipeline();
        let state = derive_file_state(&pipeline, &[], "hash123");
        assert_eq!(state.current_state, "draft");
        assert!(state.stale_signs.is_empty());
    }

    #[test]
    fn all_signs_valid_returns_final_state() {
        let pipeline = linear_pipeline();
        let signs = vec![
            make_sign("review", "hash123", false),
            make_sign("published", "hash123", false),
        ];
        let state = derive_file_state(&pipeline, &signs, "hash123");
        assert_eq!(state.current_state, "published");
        assert!(state.stale_signs.is_empty());
    }

    #[test]
    fn partial_signs_returns_intermediate_state() {
        let pipeline = linear_pipeline();
        let signs = vec![make_sign("review", "hash123", false)];
        let state = derive_file_state(&pipeline, &signs, "hash123");
        assert_eq!(state.current_state, "review");
    }

    #[test]
    fn hash_mismatch_makes_signs_stale() {
        let pipeline = linear_pipeline();
        let signs = vec![
            make_sign("review", "old_hash", false),
            make_sign("published", "old_hash", false),
        ];
        let state = derive_file_state(&pipeline, &signs, "new_hash");
        assert_eq!(state.current_state, "draft");
        assert_eq!(state.stale_signs.len(), 2);
        assert!(state.stale_signs.contains(&"review".to_string()));
        assert!(state.stale_signs.contains(&"published".to_string()));
    }

    #[test]
    fn revoked_signs_ignored_not_stale() {
        let pipeline = linear_pipeline();
        let signs = vec![make_sign("review", "hash123", true)];
        let state = derive_file_state(&pipeline, &signs, "hash123");
        assert_eq!(state.current_state, "draft");
        assert!(state.stale_signs.is_empty());
    }

    #[test]
    fn custom_multi_sign_transitions() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "reviewed".to_string(),
            vec!["editor_ok".to_string(), "legal_ok".to_string()],
        );
        transitions.insert("published".to_string(), vec!["publish_ok".to_string()]);

        let pipeline = make_pipeline(&["draft", "reviewed", "published"], transitions);

        let signs = vec![make_sign("editor_ok", "h", false)];
        let state = derive_file_state(&pipeline, &signs, "h");
        assert_eq!(state.current_state, "draft");

        let signs = vec![
            make_sign("editor_ok", "h", false),
            make_sign("legal_ok", "h", false),
        ];
        let state = derive_file_state(&pipeline, &signs, "h");
        assert_eq!(state.current_state, "reviewed");

        let signs = vec![
            make_sign("editor_ok", "h", false),
            make_sign("legal_ok", "h", false),
            make_sign("publish_ok", "h", false),
        ];
        let state = derive_file_state(&pipeline, &signs, "h");
        assert_eq!(state.current_state, "published");
    }

    #[test]
    fn gap_in_transition_chain_stops_progression() {
        let pipeline = linear_pipeline();
        let signs = vec![make_sign("published", "h", false)];
        let state = derive_file_state(&pipeline, &signs, "h");
        assert_eq!(state.current_state, "draft");
    }

    #[test]
    fn mixed_valid_and_stale_signs() {
        let pipeline = linear_pipeline();
        let signs = vec![
            make_sign("review", "current", false),
            make_sign("published", "old_hash", false),
        ];
        let state = derive_file_state(&pipeline, &signs, "current");
        assert_eq!(state.current_state, "review");
        assert_eq!(state.stale_signs, vec!["published"]);
    }
}
