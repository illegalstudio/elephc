//! Purpose:
//! Defines the `BuiltinSpec` type that describes a single PHP builtin function:
//! its name, arity, type signature, and shared backend-neutral semantics.
//!
//! Called from:
//! - `crate::builtins::registry` (collected via `inventory`).
//! - Checker, optimizer, EIR lowering, ownership, callable, and runtime consumers
//!   through `crate::builtins::semantics`.
//!
//! Key details:
//! - Every builtin must submit exactly one `BuiltinSpec` via the `builtin!` macro;
//!   duplicate names are detected at registry init time.
//! - All `BuiltinSpec` fields are `'static` so the struct can be used in `const` context
//!   and stored in the `inventory`-collected registry without allocation.

// Checker hooks are public registry metadata but intentionally receive the crate-private
// checker implementation rather than exposing compiler internals as public API.
#![allow(private_interfaces)]

pub use elephc_builtin_contract::{Area, DefaultSpec, TypeSpec};
#[cfg(test)]
pub use elephc_builtin_contract::ParamSpec;

/// Context passed to a builtin's optional `check` hook during type-checking.
///
/// Gives the hook access to the checker state, the call site name, the argument
/// list, the source span, and the current type environment so it can emit
/// diagnostics and return a refined return type.
pub struct BuiltinCheckCtx<'a> {
    /// The active type checker (mutable so the hook can emit warnings and errors).
    pub checker: &'a mut crate::types::checker::Checker,
    /// The canonical lower-cased builtin name at the call site.
    pub name: &'a str,
    /// The unevaluated argument expressions passed to the builtin.
    pub args: &'a [crate::parser::ast::Expr],
    /// Shared source-to-parameter plan retained across named/default normalization.
    pub(crate) argument_plan: Option<&'a crate::types::call_args::CallArgPlan>,
    /// Source span of the call expression, for diagnostic messages.
    pub span: crate::span::Span,
    /// The type environment active at the call site.
    pub env: &'a crate::types::TypeEnv,
}

impl BuiltinCheckCtx<'_> {
    /// Returns whether argument normalization synthesized this optional parameter's default.
    pub fn argument_was_omitted(&self, parameter_index: usize) -> bool {
        matches!(
            self.argument_plan
                .and_then(|plan| plan.regular_args.get(parameter_index)),
            Some(crate::types::call_args::PlannedRegularArg::Default(_))
        )
    }
}

/// Rejects a process-spawning builtin on a target whose sandbox forbids `fork`.
///
/// On iOS every one of these exists as a libSystem symbol and links happily,
/// then fails at run time on a device — the worst place to discover it. The call
/// is refused here instead, at the exact source position, so the mistake is a
/// compile error rather than a support ticket.
///
/// Call this first from the `check:` hook of any builtin that spawns a process.
/// The set is `system`, `passthru`, `exec`, `shell_exec`, `popen` and `pclose`;
/// `proc_open` and its family do not exist in the compiler yet, and must adopt
/// this guard on the day they do.
pub fn reject_if_process_spawn_forbidden(
    cx: &BuiltinCheckCtx<'_>,
) -> Result<(), crate::errors::CompileError> {
    if !cx.checker.target.forbids_process_spawn() {
        return Ok(());
    }
    Err(crate::errors::CompileError::new(
        cx.span,
        &format!(
            "{}() cannot be compiled for {}: that sandbox forbids spawning a process, \
             so the call would always fail at run time",
            cx.name,
            cx.checker.target.as_str()
        ),
    ))
}

/// A type-checking hook for a builtin that needs logic beyond the static parameter list.
///
/// The hook receives a mutable `BuiltinCheckCtx` and returns the refined return
/// `PhpType` for the call, or a `CompileError` if the call is ill-typed.
pub type CheckFn = for<'ctx, 'a> fn(
    &'ctx mut BuiltinCheckCtx<'a>,
) -> Result<crate::types::PhpType, crate::errors::CompileError>;

/// Contract source used by one AOT implementation binding.
pub enum BuiltinContractRef {
    /// Production binding joined to the canonical shared catalog.
    Shared(elephc_builtin_contract::BuiltinId),
    /// Test-only inline contract used by focused registry probes.
    #[cfg(test)]
    Inline(elephc_builtin_contract::BuiltinContract),
}

/// Complete static descriptor for one PHP builtin function.
///
/// All fields are `'static` so the spec can be declared as a `const` item and
/// collected into the inventory-based registry at link time without heap allocation.
pub struct BuiltinSpec {
    /// Shared production contract or an inline focused-test probe contract.
    pub contract: BuiltinContractRef,
    /// Shared backend-neutral semantics consumed by checker, optimizer, EIR, ownership,
    /// requirements, and callable paths.
    pub semantics: crate::builtins::semantics::BuiltinSemantics,
}

impl std::ops::Deref for BuiltinSpec {
    type Target = elephc_builtin_contract::BuiltinContract;

    /// Exposes neutral contract fields through the existing `BuiltinSpec` view.
    fn deref(&self) -> &Self::Target {
        match &self.contract {
            BuiltinContractRef::Shared(id) => elephc_builtin_contract::lookup_id(*id)
                .expect("AOT builtin implementation must reference a shared contract"),
            #[cfg(test)]
            BuiltinContractRef::Inline(contract) => contract,
        }
    }
}

impl BuiltinSpec {
    /// Returns the stable identity of the joined shared or inline test contract.
    pub fn id(&self) -> elephc_builtin_contract::BuiltinId {
        std::ops::Deref::deref(self).id
    }

    /// Returns the boxed-runtime ABI identity when this contract has one.
    pub fn runtime_builtin_id(&self) -> Option<elephc_builtin_contract::RuntimeBuiltinId> {
        elephc_builtin_contract::runtime_builtin_id(self.id())
    }
}

inventory::collect!(BuiltinSpec);

#[cfg(test)]
mod macro_tests {
    use crate::builtins::spec::*;
    builtin! { name: "__macro_probe", area: Types, params: [x: Int], returns: Int, semantics: crate::builtins::semantics::test_probe_semantics(), summary: "probe", internal: true }
    builtin! { name: "__macro_ext_probe", area: Types, params: [], returns: Void, semantics: crate::builtins::semantics::test_probe_semantics(), summary: "extension probe", extension: true, internal: true }

    /// Verifies macro-generated bindings join a shared contract to AOT semantics.
    #[test]
    fn macro_registers_builtin() {
        let strlen = inventory::iter::<BuiltinSpec>
            .into_iter()
            .find(|spec| spec.name == "strlen")
            .expect("strlen AOT binding must be registered");
        assert_eq!(
            strlen.id(),
            elephc_builtin_contract::BuiltinId::from_canonical_name("strlen")
        );
        assert!(matches!(
            strlen.semantics.result_ownership,
            crate::builtins::semantics::BuiltinResultOwnership::NonHeap
        ));
    }

    /// Verifies extension visibility now comes from the joined shared contract.
    #[test]
    fn macro_registers_extension_flag() {
        let strlen = inventory::iter::<BuiltinSpec>
            .into_iter()
            .find(|spec| spec.name == "strlen")
            .expect("strlen AOT binding must be registered");
        let pointer = inventory::iter::<BuiltinSpec>
            .into_iter()
            .find(|spec| spec.name == "ptr")
            .expect("ptr AOT binding must be registered");
        assert!(!strlen.extension);
        assert!(pointer.extension);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a const BuiltinSpec can be built and read (const-friendly shape).
    #[test]
    fn const_spec_is_constructible() {
        const S: BuiltinSpec = BuiltinSpec {
            contract: BuiltinContractRef::Shared(
                elephc_builtin_contract::BuiltinId::from_canonical_name("strlen"),
            ),
            semantics: crate::builtins::semantics::test_probe_semantics(),
        };
        assert_eq!(S.name, "strlen");
        assert_eq!(S.params.len(), 1);
        assert!(matches!(
            S.semantics.result_ownership,
            crate::builtins::semantics::BuiltinResultOwnership::MayAliasArguments
        ));
    }
}
