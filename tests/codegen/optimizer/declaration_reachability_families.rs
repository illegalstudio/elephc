//! Purpose:
//! Exercises virtual slot retention across many unrelated inheritance families.
//!
//! Called from:
//! - The core declaration reachability codegen suite.
//!
//! Key details:
//! - Mixed receivers require inherited and overridden implementations to survive.
//! - Separate families reuse method names without sharing lineage roots.

use super::*;

/// Preserves dispatch while seeding each virtual family through indexed descendants.
#[test]
fn test_reachability_retains_many_virtual_slot_families() {
    let mut source = String::from("<?php\n");
    for family in 0..32 {
        source.push_str(&format!(
            "class FamilyRoot{family} {{ public function value(): int {{return {family};}} \
             public function retained(): int {{return {};}} }}\n",
            1000 + family,
        ));
        for child in 0..8 {
            source.push_str(&format!(
                "class FamilyChild{family}_{child} extends FamilyRoot{family} {{ \
                 public function value(): int {{return {};}} }}\n\
                 class FamilyLeaf{family}_{child} extends FamilyChild{family}_{child} {{}}\n",
                family * 100 + child,
            ));
        }
    }
    source.push_str(
        "function family_dispatch(mixed $object): int {\n\
         return (int) $object->value() + (int) $object->retained();\n}\n",
    );
    let mut expected = String::new();
    for family in 0..32 {
        for child in 0..8 {
            source.push_str(&format!(
                "echo family_dispatch(new FamilyLeaf{family}_{child}()), ';';\n",
            ));
            expected.push_str(&format!("{};", family * 101 + child + 1000));
        }
    }
    assert_eq!(compile_and_run(&source), expected);
}

/// Keeps sparse inherited names without matching unrelated classes against every name.
#[test]
fn test_reachability_retains_sparse_inherited_method_references() {
    let mut source = String::from("<?php\n");
    let mut expected = String::new();
    for family in 0..96 {
        source.push_str(&format!(
            "class SparseParent{family} {{ public function method{family}(): int {{return {family};}} }}\n\
             class SparseChild{family} extends SparseParent{family} {{}}\n\
             function sparse_dispatch{family}(mixed $object): int {{return (int) $object->method{family}();}}\n\
             echo sparse_dispatch{family}(new SparseChild{family}()), ';';\n",
        ));
        expected.push_str(&format!("{family};"));
    }
    assert_eq!(compile_and_run(&source), expected);
}
