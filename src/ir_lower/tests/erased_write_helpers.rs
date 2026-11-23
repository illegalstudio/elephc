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

/// Unrelated classes that declare the name, and classes that overload it (`__get`, ArrayAccess), as
/// many as a framework holds: `declarers` plain declaring classes, `magic` overloaded ones.
fn crowd(declarers: usize, magic: usize) -> String {
    let mut source = String::from("<?php\nclass Svc { public int $n = 1; }\n");
    for index in 0..declarers {
        source.push_str(&format!("class Decl{index} {{ public array $privates = []; public array $other{index} = []; }}\n"));
    }
    for index in 0..magic {
        if index % 2 == 0 {
            source.push_str(&format!("class Magic{index} {{ public function __set($n, $v) {{}} public function __get($n) {{ return []; }} }}\n"));
        } else {
            source.push_str(&format!(
                "class Magic{index} implements ArrayAccess {{ public array $privates = []; public function offsetExists($o): bool {{ return false; }} public function offsetGet($o): mixed {{ return 1; }} public function offsetSet($o, $v): void {{}} public function offsetUnset($o): void {{}} }}\n"));
        }
    }
    source.push_str("function write(mixed $c) { $c->privates['x'] = new Svc; }\nwrite(new Decl0);\n");
    source
}

/// The ladder of a write grows by a handful of instructions per overloaded or declaring class, not
/// by a copy of the write for each: the overloaded kinds share one tail.
#[test]
fn write_helpers_share_the_overloaded_tail_and_the_arms_of_unrelated_declarers() {
    let size = |declarers, magic| {
        let module = super::lower_source(&crowd(declarers, magic));
        module.functions.iter()
            .filter(|function| function.name.starts_with(crate::names::ERASED_WRITE_HELPER_PREFIX)
                && function.name.ends_with("@privates"))
            .map(|function| function.instructions.len()).sum::<usize>()
    };
    let (small, large) = (size(10, 20), size(60, 220));
    let per_class = (large - small) / 250;
    assert!(per_class <= 12, "{per_class} instructions per added class ({small} -> {large})");
    assert!(large < 6_000, "the helper is {large} instructions");
}
