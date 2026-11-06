//! Whole-module name lookups: what codegen asks per site must not walk a table per question.

use crate::ir::module::FACT_BUILDS;
use crate::ir::*;
use crate::types::PhpType;

fn module_with(names: &[&str]) -> Module {
    let mut module = Module::new(crate::codegen::platform::Target::detect_host());
    for name in names {
        module.add_function(Function::new((*name).into(), IrType::Void, PhpType::Void));
    }
    module
}

/// A dynamic `new` asks once per candidate class per site, so the table is walked once.
#[test]
fn asking_for_a_function_many_times_builds_the_index_once() {
    let module = module_with(&["_ctor_thunk_1", "_ctor_thunk_2"]);
    let before = FACT_BUILDS.with(std::cell::Cell::get);
    for _ in 0..1000 {
        assert!(module.has_function_named("_ctor_thunk_2"));
        assert!(!module.has_function_named("_ctor_thunk_3"));
    }
    assert_eq!(FACT_BUILDS.with(std::cell::Cell::get) - before, 1);
}

/// A thunk added after the first question must be seen: a frozen index would drop the class from
/// the dynamic `new` ladder, which is a missing constructor call and no diagnostic.
#[test]
fn a_function_added_after_the_first_question_is_found() {
    let mut module = module_with(&["_ctor_thunk_1"]);
    assert!(!module.has_function_named("_ctor_thunk_2"));
    module.add_function(Function::new("_ctor_thunk_2".into(), IrType::Void, PhpType::Void));
    assert!(module.has_function_named("_ctor_thunk_2"));
}

/// The name is exact: the thunk labels are compared as emitted, never folded.
#[test]
fn a_function_name_is_matched_exactly() {
    let module = module_with(&["_ctor_thunk_1"]);
    assert!(!module.has_function_named("_CTOR_THUNK_1"));
}
