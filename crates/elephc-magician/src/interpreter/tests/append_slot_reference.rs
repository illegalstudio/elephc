//! Purpose:
//! Interpreter tests pinning `$name = &TARGET[];` -- an APPEND used as the reference SOURCE.
//! `$closure = &$this->optimized[$eventName][];` is `event-dispatcher`'s `EventDispatcher`.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::append_slot_reference`.
//!
//! Key details:
//! - Every expected string is `php -n` 8.5.6's output on the same fragment.
//! - PHP CREATES the element before binding, and that is observable rather than incidental:
//!   `count()` grows by one and the new entry reads back as null even if the bound name is never
//!   written. A lowering that only produced an alias, or that waited for the first write, would
//!   pass a test that checked the write alone.
//! - `parse_postfix` stops in front of an empty `[]`, so a source ending in one used to arrive
//!   at the statement parser with the brackets unconsumed and the diagnostic landed on the `[`
//!   as a missing semicolon -- which reads like a broken statement rather than a missing rule.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse append slot reference fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute append slot reference fragment");
    values.output.clone()
}

/// Verifies the element is created, reads back null, and receives what the bound name is given.
///
/// `php -n` 8.5.6 prints `null;1;set`. The middle number is the one that says the element exists
/// before any write: a lowering that deferred creation until the alias was written would print
/// `null;0;set`.
#[test]
fn an_append_source_creates_the_element_and_binds_to_it() {
    assert_eq!(
        out(
            br#"class Bag { public array $opt = []; }
$b = new Bag();
$closure = &$b->opt["e"][];
echo ($closure === null) ? "null" : "notnull", ";";
echo count($b->opt["e"]), ";";
$closure = "set";
echo $b->opt["e"][0];"#
        ),
        "null;1;set",
    );
}

/// Verifies each bind takes the NEXT key rather than reusing one.
///
/// `php -n` 8.5.6 prints `[5,6]`. Two binds in a row must land on two elements; sharing a key
/// would print `[6]` and still satisfy a test that only read the last one.
#[test]
fn two_append_sources_take_two_successive_keys() {
    assert_eq!(
        out(
            br#"$src = [];
$c1 = &$src[];
$c1 = 5;
$c2 = &$src[];
$c2 = 6;
echo json_encode($src);"#
        ),
        "[5,6]",
    );
}

/// Verifies a plain local array works the same, which is the shape without a property in it.
///
/// `php -n` 8.5.6 prints `5`.
#[test]
fn an_append_source_on_a_local_array_binds() {
    assert_eq!(
        out(
            br#"$src = [];
$c = &$src[];
$c = 5;
echo $src[0];"#
        ),
        "5",
    );
}
