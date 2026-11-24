//! Purpose:
//! Publishes the program-wide facts that decide how EIR lowering reads an unresolved constant:
//! whether dynamic `eval()` can run, and which names a compiled `define()` declares.
//!
//! Called from:
//! - `crate::ir_lower::program::lower_on_compiler_stack()` installs the facts for one program
//!   lowering.
//! - `crate::ir_lower::expr::constants::lower_const_ref()` consults them before choosing the
//!   eval-aware runtime lookup or native global storage.
//!
//! Key details:
//! - A property default can reference a constant that only `eval()` defines. Native global
//!   storage is never written for such a constant, so reading it there silently yields its
//!   zero default instead of PHP's value or undefined-constant error.
//! - A name that any compiled `define()` declares keeps native global storage as one possible
//!   home, but finding the call is not proof it ran. Lowering therefore reads that storage
//!   only through the runtime `define()` flag and falls back to the eval registry otherwise.
//! - The facts are scoped to one lowering call; a nested lowering restores the outer facts on
//!   drop, exactly like the other per-program lowering registries.
//! - Programs without `eval()` keep the native fallback, so they never link the eval bridge.

use std::cell::RefCell;
use std::collections::HashSet;

/// Program-wide facts consulted by unresolved constant lowering.
#[derive(Clone, Default)]
struct ProgramEvalFacts {
    /// Whether the checked source contains an `eval()` call, even one the optimizer removes.
    contains_eval: bool,
    /// Literal names passed to a compiled `define()`, including calls inside functions.
    native_defines: HashSet<String>,
}

thread_local! {
    /// Facts for the program currently being lowered.
    static PROGRAM_EVAL_FACTS: RefCell<ProgramEvalFacts> = RefCell::new(ProgramEvalFacts::default());
}

/// Installs the program facts for the duration of the returned scope.
pub(crate) fn install(
    contains_eval: bool,
    native_defines: &HashSet<String>,
) -> ProgramEvalScope {
    let facts = ProgramEvalFacts {
        contains_eval,
        native_defines: native_defines.clone(),
    };
    let previous = PROGRAM_EVAL_FACTS.with(|slot| slot.replace(facts));
    ProgramEvalScope { previous }
}

/// Returns whether the program currently being lowered can define constants through `eval()`.
pub(crate) fn contains_eval() -> bool {
    PROGRAM_EVAL_FACTS.with(|slot| slot.borrow().contains_eval)
}

/// Returns whether a compiled `define()` declares `name` anywhere in the program.
///
/// A true answer means native global storage MAY hold the value once that call runs; it does
/// not establish that the call ran, so the read still needs the runtime flag check. The
/// leading `\` of a fully qualified constant name is not part of its identity.
pub(crate) fn has_native_define(name: &str) -> bool {
    let name = name.trim_start_matches('\\');
    PROGRAM_EVAL_FACTS.with(|slot| slot.borrow().native_defines.contains(name))
}

/// Restores the previous program-eval facts when the enclosing lowering finishes.
pub(crate) struct ProgramEvalScope {
    previous: ProgramEvalFacts,
}

impl Drop for ProgramEvalScope {
    /// Restores the facts that were active before this lowering scope.
    fn drop(&mut self) {
        let previous = self.previous.clone();
        PROGRAM_EVAL_FACTS.with(|slot| slot.replace(previous));
    }
}
