//! Purpose:
//! Rebinds unqualified function references whose conditional namespace declaration was pruned.
//!
//! Called from:
//! - `crate::optimize::namespace_fallbacks` after the first target-folding pass.
//!
//! Key details:
//! - Only resolver-recorded fallback candidates may change; explicit PHP names stay exact.
//! - Surviving conditional declarations still shadow globals, even when their guard is unknown.

use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind, Program};
use crate::span::Span;

use super::{expressions, resolved_name, symbols, Imports, Symbols};

/// Looks up global user functions, registered builtins, and resolver-provided date aliases.
pub(super) fn global_candidate(symbols: &Symbols, name: &str) -> Option<String> {
    symbols.canonical_function(name).or_else(|| {
        super::is_global_date_procedural_alias(name).then(|| name.to_ascii_lowercase())
    })
}

/// Surviving symbols used to reconsider only references affected by declaration pruning.
pub(crate) struct FunctionFallbacks {
    symbols: Symbols,
}

impl FunctionFallbacks {
    /// Collects declarations still present after target-dependent branches have been removed.
    pub(crate) fn new(program: &Program) -> Self {
        let mut symbols = Symbols::new(crate::codegen::platform::Platform::MacOS);
        symbols::collect_symbols(program, None, &mut symbols);
        Self { symbols }
    }

    /// Returns a recorded global candidate only when its original local declaration is gone.
    pub(crate) fn resolve_name(&self, name: &Name) -> Option<Name> {
        let fallback = name.function_fallback()?;
        if self.symbols.declares_function(name.as_str()) {
            return None;
        }
        global_candidate(&self.symbols, fallback).map(resolved_name)
    }

    /// Reuses normal builtin alias/desugaring rules when a call falls back to a global function.
    pub(crate) fn resolve_call(&self, name: &Name, args: &[Expr], span: Span) -> Option<Expr> {
        let name = self.resolve_name(name)?;
        let call = Expr::new(ExprKind::FunctionCall { name, args: args.to_vec() }, span);
        Some(expressions::resolve_expr(&call, None, &Imports::default(), &self.symbols))
    }
}
