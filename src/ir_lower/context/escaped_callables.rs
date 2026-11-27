//! Purpose:
//! Finds tracked callable generations whose storage is reachable outside this activation.
//!
//! Called from:
//! - `LoweringContext::invalidate_escaped_callable_locals` at opaque PHP call boundaries.
//!
//! Key details:
//! - Reference cells and main-scope globals are the only invalidation candidates.
//! - Synthetic temporaries grow the generation map without growing the candidate set.

use std::collections::{HashMap, HashSet};

/// Intersects tracked generations with escaped names, returning each name exactly once.
pub(super) fn names(
    epochs: &HashMap<String, u64>, references: &HashSet<String>,
    globals: &HashSet<String>, in_main: bool,
) -> Vec<String> {
    let mut escaped: Vec<_> = references.iter()
        .filter(|name| epochs.contains_key(*name)).cloned().collect();
    if in_main {
        escaped.extend(globals.iter().filter(|name| {
            !references.contains(*name) && epochs.contains_key(*name)
        }).cloned());
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checks the semantic set formula for every tracked/reference/global combination.
    #[test]
    fn every_small_membership_combination_preserves_scope_rules() {
        for tracked in 0..8 {
            let epochs: HashMap<_, _> = (0..3).filter(|index| tracked & (1 << index) != 0)
                .map(|index| (format!("local{index}"), 7)).collect();
            for referenced in 0..8 {
                let references = (0..3).filter(|index| referenced & (1 << index) != 0)
                    .map(|index| format!("local{index}")).collect();
                for global in 0..8 {
                    let globals = (0..3).filter(|index| global & (1 << index) != 0)
                        .map(|index| format!("local{index}")).collect();
                    for in_main in [false, true] {
                        let mask = tracked & (referenced | if in_main { global } else { 0 });
                        let expected: HashSet<_> = (0..3).filter(|index| mask & (1 << index) != 0)
                            .map(|index| format!("local{index}")).collect();
                        let actual = names(&epochs, &references, &globals, in_main);
                        assert_eq!(actual.len(), expected.len());
                        assert_eq!(actual.into_iter().collect::<HashSet<_>>(), expected);
                    }
                }
            }
        }
    }

    /// Overlapping reference and global membership never advances one generation twice.
    #[test]
    fn overlapping_and_untracked_cells_remain_distinct() {
        let epochs = [("shared".to_string(), u64::MAX)].into_iter().collect();
        let references: HashSet<_> = ["shared".to_string(), "absent".to_string()].into_iter().collect();
        let globals = references.clone();
        assert_eq!(names(&epochs, &references, &globals, true), vec!["shared"]);
        assert_eq!(epochs["shared"], u64::MAX);
    }

    /// A large temporary inventory does not add escaped cells in either activation kind.
    #[test]
    fn many_temporaries_keep_only_the_sparse_escaped_set() {
        let epochs = (0..20000).map(|index| (format!("tmp{index}"), 0)).collect();
        let references = ["tmp7".to_string()].into_iter().collect();
        let globals = ["tmp9".to_string(), "missing".to_string()].into_iter().collect();
        assert_eq!(names(&epochs, &references, &globals, false), vec!["tmp7"]);
        assert_eq!(names(&epochs, &references, &globals, true).into_iter()
            .collect::<HashSet<_>>(), ["tmp7".to_string(), "tmp9".to_string()].into_iter().collect());
    }
}
