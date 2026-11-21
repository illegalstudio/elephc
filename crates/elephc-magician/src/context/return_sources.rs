//! Purpose:
//! Scopes immutable declared-return metadata independently from caller argument frames.
//!
//! Called from:
//! - Interpreted function/method bodies and nested program execution boundaries.
//!
//! Key details:
//! - Nested eval/include returns shadow the enclosing function's return declaration.
//! - Frames retain metadata only and restore their prior depth through Rust failures or panics.

use super::{ElephcEvalContext, EvalParameterType};

/// Declaration context for a reached return statement, independent from its returned value.
#[derive(Clone)]
pub(crate) struct ReturnSourceContract {
    pub(crate) ty: EvalParameterType,
    pub(crate) declaring_class: Option<String>,
    pub(crate) called_class: Option<String>,
}

impl ElephcEvalContext {
    /// Borrows the declaration belonging to the innermost function or program return scope.
    pub(crate) fn return_source_contract(&self) -> Option<&ReturnSourceContract> {
        self.return_source_contracts.last().and_then(Option::as_ref)
    }

    /// Restores only metadata when an execution region completes, including panic propagation.
    pub(crate) fn with_return_source_contract<T>(&mut self, contract: Option<ReturnSourceContract>,
        body: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let depth = self.return_source_contracts.len();
        self.return_source_contracts.push(contract);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(self)));
        self.return_source_contracts.truncate(depth);
        match result { Ok(result) => result, Err(panic) => std::panic::resume_unwind(panic) }
    }
}
