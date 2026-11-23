//! Purpose:
//! Complexity checks for the shared helpers of writes through a receiver that lost its class.
//!
//! Called from:
//! - The AST-to-EIR regression suite.
//!
//! Key details:
//! - Symfony's dumped container has one class per service, all inheriting one declaration of
//!   `privates`; a helper must not grow by an arm per inheriting class, with eval linked or not.

/// A container shape: `services` factory classes inheriting a base that declares `privates`.
fn container(services: usize, eval: bool) -> String {
    let mut source = String::from("<?php\nclass Container { protected array $privates = []; }\nclass Svc { public int $n = 1; }\n");
    for index in 0..services {
        source.push_str(&format!(
            "class Factory{index} extends Container {{ public static function make(mixed $c) {{ $c->privates['s{index}'] = new Svc; return $c; }} }}\n"));
    }
    if eval { source.push_str("eval($argc > 5 ? 'echo 1;' : '');\n"); }
    source.push_str("$c = new Factory0;\n");
    for index in 0..services { source.push_str(&format!("Factory{index}::make($c);\n")); }
    source
}

/// EIR instructions of the shared helpers that write `privates`, and how many there are.
fn helper_size(services: usize, eval: bool) -> (usize, usize) {
    let module = super::lower_source(&container(services, eval));
    let helpers: Vec<_> = module.functions.iter()
        .filter(|function| function.name.starts_with(crate::names::ERASED_WRITE_HELPER_PREFIX)
            && function.name.ends_with("@privates")).collect();
    (helpers.iter().map(|function| function.instructions.len()).sum(), helpers.len())
}

/// One helper per scope group serves every inheriting class and its size is nearly independent of
/// their number: without eval the group is a run of class ids, with eval one name test each.
#[test]
fn container_write_helpers_do_not_grow_by_an_arm_per_inheriting_class() {
    for eval in [false, true] {
        let (small, small_count) = helper_size(8, eval);
        let (large, large_count) = helper_size(48, eval);
        assert_eq!(small_count, large_count, "eval={eval}: one helper per scope group, not per class");
        let per_class = (large - small) / 40;
        assert!(per_class <= if eval { 12 } else { 2 }, "eval={eval}: {per_class} instructions per inheriting class");
        assert!(large < 3_000, "eval={eval}: the helper is {large} instructions");
    }
}
