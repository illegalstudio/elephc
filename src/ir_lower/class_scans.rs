//! Purpose:
//! Answers questions about the whole class table once per lowered program.
//!
//! Called from:
//! - `crate::ir_lower::program` (installs the answers for one program).
//! - Expression lowering, for every call, condition and `new` that used to walk the table.
//!
//! Key details:
//! - The class table is fixed while a program is lowered, so each answer is computed at most once.
//! - A framework program declares thousands of classes and lowers tens of thousands of calls: one
//!   walk per call was most of the Symfony Demo's lowering.
//! - Outside an installed program (unit tests that lower a fragment) every answer is recomputed.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use super::context::LoweringContext;
use crate::fast_hash::FastMap;

/// The answers computed so far for the program being lowered.
#[derive(Default)]
struct ClassScans {
    /// Program classes the builtin contract describes, sorted.
    contract_classes: OnceCell<Rc<Vec<String>>>,
    /// Classes declaring or inheriting each method key, sorted.
    by_method: OnceCell<FastMap<String, Vec<String>>>,
    /// Whether initializing any class has an effect.
    any_initialization_effect: OnceCell<bool>,
    /// Whether initializing one class itself can raise a deprecation.
    initialization_reports: RefCell<FastMap<String, bool>>,
}

thread_local! {
    static ACTIVE: RefCell<Option<Rc<ClassScans>>> = const { RefCell::new(None) };
    static WALKS: Cell<usize> = const { Cell::new(0) };
}

/// Keeps the answers of one program installed, restoring the previous ones when dropped.
pub(super) struct Installed(Option<Rc<ClassScans>>);

/// Starts answering for a new program.
pub(super) fn install() -> Installed {
    Installed(ACTIVE.with(|active| active.replace(Some(Rc::default()))))
}

impl Drop for Installed {
    fn drop(&mut self) {
        let previous = self.0.take();
        ACTIVE.with(|active| *active.borrow_mut() = previous);
    }
}

/// Returns the installed answers, or empty ones that live for this question only.
fn scans() -> Rc<ClassScans> {
    ACTIVE.with(|active| active.borrow().clone()).unwrap_or_default()
}

/// Counts one walk of the whole class table, for the complexity tests.
fn record_walk() {
    WALKS.with(|walks| walks.set(walks.get() + 1));
}

/// Returns how many whole-table walks this thread has done.
#[cfg(test)]
pub(super) fn walks() -> usize {
    WALKS.with(Cell::get)
}

/// Returns the program's classes that the builtin contract describes, sorted: the only ones that
/// can carry native operators, casts or truthiness adapters.
pub(super) fn contract_classes(ctx: &LoweringContext<'_, '_>) -> Rc<Vec<String>> {
    scans()
        .contract_classes
        .get_or_init(|| {
            record_walk();
            let mut names: Vec<String> = ctx
                .classes
                .keys()
                .filter(|name| elephc_builtin_contract::lookup_class(name).is_some())
                .cloned()
                .collect();
            names.sort_unstable();
            Rc::new(names)
        })
        .clone()
}

/// Returns the classes whose method table holds `key`, sorted.
pub(super) fn classes_with_method(ctx: &LoweringContext<'_, '_>, key: &str) -> Vec<String> {
    scans()
        .by_method
        .get_or_init(|| {
            record_walk();
            let mut index: FastMap<String, Vec<String>> = FastMap::default();
            for (name, class) in ctx.classes {
                for method in class.methods.keys() {
                    index.entry(method.clone()).or_default().push(name.clone());
                }
            }
            for names in index.values_mut() {
                names.sort_unstable();
            }
            index
        })
        .get(key)
        .cloned()
        .unwrap_or_default()
}

/// Returns whether initializing any class of the program has an effect.
pub(super) fn any_initialization_effect(ctx: &LoweringContext<'_, '_>) -> bool {
    *scans().any_initialization_effect.get_or_init(|| {
        record_walk();
        ctx.classes
            .keys()
            .any(|class| super::property_default_errors::has_initialization_effect(ctx, class))
    })
}

/// Returns `compute()` for `class_name`, computed once per program.
pub(super) fn initialization_reports(class_name: &str, compute: impl FnOnce() -> bool) -> bool {
    let scans = scans();
    if let Some(answer) = scans.initialization_reports.borrow().get(class_name) {
        return *answer;
    }
    let answer = compute();
    scans.initialization_reports.borrow_mut().insert(class_name.to_string(), answer);
    answer
}
