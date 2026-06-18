//! Purpose:
//! Parser tests for eval class trait adaptation syntax.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - These cases verify `insteadof` and `as` clauses lower into class metadata.

use super::support::*;

/// Verifies trait `insteadof`, alias, and visibility adaptations are parsed.
#[test]
fn parse_fragment_accepts_trait_adaptations() {
    let program = parse_fragment(
        br#"class EvalAdaptBox {
    use EvalAdaptA, EvalAdaptB {
        EvalAdaptA::talk insteadof EvalAdaptB;
        EvalAdaptB::talk as talkB;
        EvalAdaptA::hidden as private;
        talk as public talkPublic;
    }
}"#,
    )
    .expect("fragment should parse");

    assert_eq!(
        program.statements(),
        &[EvalStmt::ClassDecl(
            EvalClass::with_modifiers_traits_adaptations_and_constants(
                "EvalAdaptBox",
                false,
                false,
                None,
                Vec::new(),
                vec!["EvalAdaptA".to_string(), "EvalAdaptB".to_string()],
                vec![
                    EvalTraitAdaptation::InsteadOf {
                        trait_name: Some("EvalAdaptA".to_string()),
                        method: "talk".to_string(),
                        instead_of: vec!["EvalAdaptB".to_string()],
                    },
                    EvalTraitAdaptation::Alias {
                        trait_name: Some("EvalAdaptB".to_string()),
                        method: "talk".to_string(),
                        alias: Some("talkB".to_string()),
                        visibility: None,
                    },
                    EvalTraitAdaptation::Alias {
                        trait_name: Some("EvalAdaptA".to_string()),
                        method: "hidden".to_string(),
                        alias: None,
                        visibility: Some(EvalVisibility::Private),
                    },
                    EvalTraitAdaptation::Alias {
                        trait_name: None,
                        method: "talk".to_string(),
                        alias: Some("talkPublic".to_string()),
                        visibility: Some(EvalVisibility::Public),
                    },
                ],
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        )]
    );
}
