//! Purpose:
//! Regression coverage for exception-handler cleanup on explicit lexical exits.
//!
//! Called from:
//! - `cargo test` through the parent `crate::ir_lower::tests` module.
//!
//! Key details:
//! - Non-throwing exits must pop every `try` handler they leave, while a throw keeps the
//!   innermost handler active so catch dispatch can receive it.

use crate::ir::{Function, Op, Terminator};

use super::lower_source;

/// Counts handler-pop instructions emitted in one basic block.
fn handler_pop_count(function: &Function, block_index: usize) -> usize {
    function.blocks[block_index]
        .instructions
        .iter()
        .filter(|instruction| {
            function
                .instruction(**instruction)
                .is_some_and(|instruction| instruction.op == Op::TryPopHandler)
        })
        .count()
}

/// Returns the named user function from a lowered module.
fn user_function<'a>(module: &'a crate::ir::Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("missing lowered function {name}"))
}

/// Renders block names, handler-pop counts, and terminators for assertion diagnostics.
fn handler_block_summary(function: &Function) -> String {
    function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let ops = block
                .instructions
                .iter()
                .filter_map(|instruction| function.instruction(*instruction))
                .map(|instruction| instruction.op.name())
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{}: pops={} ops=[{}] term={:?}",
                block.name,
                handler_pop_count(function, index),
                ops,
                block.terminator
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Verifies a return leaving two nested try bodies pops both runtime handlers first.
#[test]
fn return_pops_every_nested_try_handler() {
    let module = lower_source(
        r#"<?php
function may_throw(bool $fail): int {
    if ($fail) { throw new RuntimeException('fail'); }
    return 7;
}
function nested_return(bool $fail): int {
    try {
        try {
            return may_throw($fail);
        }
        catch (RuntimeException $inner) { return 8; }
    } catch (Throwable $outer) { return 9; }
}
echo nested_return(false);
"#,
    );
    let function = user_function(&module, "nested_return");
    let returning_block = function
        .blocks
        .iter()
        .enumerate()
        .find(|(_, block)| {
            matches!(block.terminator, Some(Terminator::Return { value: Some(_) }))
                && handler_pop_count(function, block.id.as_raw() as usize) == 2
        });
    assert!(
        returning_block.is_some(),
        "nested return did not pop both try handlers:\n{}",
        handler_block_summary(function)
    );
}

/// Verifies break and continue pop the inner handler whose lexical try body they leave.
#[test]
fn loop_exits_pop_the_inner_try_handler() {
    let module = lower_source(
        r#"<?php
function loop_exits(bool $skip): int {
    $count = 0;
    while ($count < 2) {
        try {
            $count++;
            if ($skip && $count < 0) {
                throw new RuntimeException('keep catch reachable');
            }
            if ($skip) { continue; }
            break;
        } catch (Throwable $error) {
            return -1;
        }
    }
    return $count;
}
echo loop_exits(false), loop_exits(true);
"#,
    );
    let function = user_function(&module, "loop_exits");
    let pop_branches = function
        .blocks
        .iter()
        .enumerate()
        .filter(|(index, block)| {
            handler_pop_count(function, *index) == 1
                && matches!(block.terminator, Some(Terminator::Br { .. }))
                && !block.name.starts_with("try.")
        })
        .count();
    assert!(
        pop_branches >= 2,
        "break and continue did not each pop their exited try handler:\n{}",
        handler_block_summary(function)
    );
}

/// Verifies an ordinary throw keeps the current handler installed for catch dispatch.
#[test]
fn throw_keeps_the_current_try_handler_active() {
    let module = lower_source(
        r#"<?php
function caught_throw(): int {
    try { throw new RuntimeException('caught'); }
    catch (RuntimeException $error) { return 1; }
}
echo caught_throw();
"#,
    );
    let function = user_function(&module, "caught_throw");
    let throwing_block = function
        .blocks
        .iter()
        .enumerate()
        .find(|(_, block)| matches!(block.terminator, Some(Terminator::Throw { .. })))
        .expect("missing explicit throw block");
    assert_eq!(
        handler_pop_count(function, throwing_block.0),
        0,
        "throw popped the handler that must receive it"
    );
}
