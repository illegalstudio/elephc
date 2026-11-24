//! Purpose:
//! Dynamic property writes on an untyped receiver must behave the same whether the module is small
//! enough to keep the `__set` and dynamic-hash arms inline at each site or large enough to share
//! them through the module-wide tail.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The fixture is `dynamic_write_tail.php`; `dynamic_write_tail.expected` is `php -n` 8.5's own
//!   output for it (the `__set` recursion guard with the same and a different name, an exception
//!   through `__set`, dynamic-hash creation notices, typed and protected refusals with their lines,
//!   numeric-string and NUL names).
//! - The large variant adds enough `__set` subclasses and dynamic-property classes to cross the
//!   threshold at which sites call the tail; none is ever instantiated.

use crate::support::*;

/// The shared fixture with `extra` additional `__set` subclasses and dynamic-property classes.
fn fixture(extra: usize) -> String {
    let mut source = String::from("<?php\n");
    source.push_str(include_str!("dynamic_write_tail.php"));
    for index in 0..extra {
        source.push_str(&format!(
            "class MgX{index} extends Mg {{ public int $x{index} = {index}; }}\n#[AllowDynamicProperties] class PlX{index} {{ public $p{index} = {index}; }}\n"
        ));
    }
    if extra > 0 {
        source.push_str("function never_called() { return [");
        for index in 0..extra {
            source.push_str(&format!("new MgX{index}(), new PlX{index}(), "));
        }
        source.push_str("]; }\nif ($argc > 99) { never_called(); }\n");
    }
    source
}

/// Below the threshold the arms stay inline at the site.
#[test]
fn test_dynamic_writes_match_php_with_inline_arms() {
    let out = compile_and_run(&fixture(0));
    assert_eq!(out, include_str!("dynamic_write_tail.expected"));
}

/// Above it every site calls the module-wide tail, with identical behaviour.
#[test]
fn test_dynamic_writes_match_php_through_the_shared_tail() {
    let out = compile_and_run(&fixture(12));
    assert_eq!(out, include_str!("dynamic_write_tail.expected"));
}
