//! Purpose:
//! Records expected AOT/eval backend coverage and the reason for every intentional
//! absence in the shared builtin catalog.
//!
//! Called from:
//! - Compiler and Magician registry audits after joining implementation bindings.
//! - Cross-backend parity tests that must not maintain independent name allowlists.
//!
//! Key details:
//! - Implemented registry entries are proven by backend inventory, not duplicated here.
//! - Only non-registry routes and intentional unsupported surfaces need explicit data.

use crate::{
    eval_signature, runtime_builtin_id, Area, BuiltinContract, BuiltinId, BuiltinKind,
    RuntimeBuiltinId,
};

/// Backend whose support contract is being queried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinBackend {
    /// Static compiler checker/EIR/codegen path.
    Aot,
    /// Magician dynamic eval path.
    Eval,
}

/// How a supported contract reaches one backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendImplementation {
    /// Backend-owned inventory binding joined by `BuiltinId`.
    Registry,
    /// Parser/checker/lowering path for a PHP language construct.
    LanguageConstruct,
    /// Dedicated syntax node rather than an ordinary function call.
    DedicatedSyntax,
    /// Injected elephc-PHP prelude backed by internal compiler builtins.
    Prelude,
}

/// Why a shared catalog surface is deliberately absent from one backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedReason {
    /// Compiler-internal helper that is not part of Magician's PHP surface.
    InternalCompilerSurface,
    /// PHP-visible AOT implementation whose Magician implementation has not landed.
    EvalImplementationPending,
    /// Reflection behavior currently exists only for eval-declared/runtime objects.
    EvalOnlyReflection,
}

/// Expected support for one contract/backend pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendSupport {
    /// The backend must expose exactly one implementation through this route.
    Implemented(BackendImplementation),
    /// Absence is intentional and carries a machine-auditable reason.
    Unsupported(UnsupportedReason),
}

/// Why Magician must retain an interpreter-level adapter instead of using only
/// the versioned boxed-runtime dispatcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalAdapterReason {
    /// Caller-addressable storage, writeback, or lvalue evaluation is required.
    ByReferenceOrLvalue,
    /// Runtime callable resolution or class/member reflection is required.
    CallableOrReflection,
    /// Object conversion depends on declarations owned by the eval program.
    DynamicObjectCoercion,
    /// Language-construct, dedicated-syntax, or prelude behavior is eval-owned.
    DynamicLanguageSurface,
    /// Availability or behavior depends on an optional runtime capability.
    CapabilityDependent,
    /// Files, resources, process state, output state, or opaque handles are involved.
    RuntimeStateOrResource,
    /// The boxed-value algorithm remains an interpreter implementation rather than
    /// a generated-runtime helper with an equivalent ownership/error contract.
    InterpreterSpecificValueSemantics,
    /// A shared runtime helper covers only a strict subset of the PHP signature.
    AdditionalSignatureSemantics,
}

/// Expected Magician execution route after joining one implementation binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvalExecution {
    /// All supported arities dispatch through the versioned boxed-runtime ABI.
    SharedRuntime(RuntimeBuiltinId),
    /// Magician retains a documented adapter, optionally falling back from a
    /// shared runtime helper for unsupported signature variants.
    Adapter {
        /// Shared runtime subset used before the adapter, when one exists.
        runtime_builtin: Option<RuntimeBuiltinId>,
        /// Dynamic/interpreter-specific reason the adapter remains.
        reason: EvalAdapterReason,
    },
}

impl EvalExecution {
    /// Returns the typed runtime subset used by this execution route.
    pub const fn runtime_builtin(self) -> Option<RuntimeBuiltinId> {
        match self {
            Self::SharedRuntime(runtime_builtin) => Some(runtime_builtin),
            Self::Adapter {
                runtime_builtin, ..
            } => runtime_builtin,
        }
    }
}

/// Returns the expected support route for one shared contract and backend.
pub fn backend_support(contract: &BuiltinContract, backend: BuiltinBackend) -> BackendSupport {
    match backend {
        BuiltinBackend::Aot => aot_support(contract),
        BuiltinBackend::Eval => eval_support(contract),
    }
}

/// Returns the expected compiler route for one shared contract.
pub fn aot_support(contract: &BuiltinContract) -> BackendSupport {
    if is_eval_only_reflection(contract.id) {
        return BackendSupport::Unsupported(UnsupportedReason::EvalOnlyReflection);
    }
    let implementation = match contract.kind {
        BuiltinKind::Function => BackendImplementation::Registry,
        BuiltinKind::LanguageConstruct => BackendImplementation::LanguageConstruct,
        BuiltinKind::DedicatedSyntax => BackendImplementation::DedicatedSyntax,
        BuiltinKind::PreludeProvided => BackendImplementation::Prelude,
    };
    BackendSupport::Implemented(implementation)
}

/// Returns the expected Magician route for one shared contract.
pub fn eval_support(contract: &BuiltinContract) -> BackendSupport {
    if contract.internal {
        return BackendSupport::Unsupported(UnsupportedReason::InternalCompilerSurface);
    }
    if EVAL_IMPLEMENTATION_PENDING
        .iter()
        .any(|name| contract.id == BuiltinId::from_canonical_name(name))
    {
        return BackendSupport::Unsupported(UnsupportedReason::EvalImplementationPending);
    }
    BackendSupport::Implemented(BackendImplementation::Registry)
}

/// Returns the documented execution route for an eval-supported contract.
pub fn eval_execution(contract: &BuiltinContract) -> Option<EvalExecution> {
    if !matches!(
        eval_support(contract),
        BackendSupport::Implemented(BackendImplementation::Registry)
    ) {
        return None;
    }

    if let Some(runtime_builtin) = runtime_builtin_id(contract.id) {
        return Some(if matches!(
            runtime_builtin,
            RuntimeBuiltinId::Intval | RuntimeBuiltinId::Round
        ) {
            EvalExecution::Adapter {
                runtime_builtin: Some(runtime_builtin),
                reason: EvalAdapterReason::AdditionalSignatureSemantics,
            }
        } else {
            EvalExecution::SharedRuntime(runtime_builtin)
        });
    }

    let reason = if contract.name == "strval" {
        EvalAdapterReason::DynamicObjectCoercion
    } else if eval_signature(contract)
        .params
        .iter()
        .any(|param| param.by_ref)
    {
        EvalAdapterReason::ByReferenceOrLvalue
    } else if matches!(contract.area, Area::Callables | Area::Spl) {
        EvalAdapterReason::CallableOrReflection
    } else if !matches!(contract.kind, BuiltinKind::Function) {
        EvalAdapterReason::DynamicLanguageSurface
    } else if !contract.requirements.is_empty() {
        EvalAdapterReason::CapabilityDependent
    } else if matches!(contract.area, Area::Io | Area::System | Area::Pointers) {
        EvalAdapterReason::RuntimeStateOrResource
    } else {
        EvalAdapterReason::InterpreterSpecificValueSemantics
    };
    Some(EvalExecution::Adapter {
        runtime_builtin: None,
        reason,
    })
}

/// Returns whether a function contract is intentionally available only in Magician.
fn is_eval_only_reflection(id: BuiltinId) -> bool {
    [
        "get_called_class",
        "get_class_methods",
        "get_class_vars",
    ]
    .into_iter()
    .any(|name| id == BuiltinId::from_canonical_name(name))
}

/// PHP-visible AOT contracts that do not yet have a Magician implementation binding.
const EVAL_IMPLEMENTATION_PENDING: &[&str] = &[
    "array_all",
    "array_any",
    "array_diff_assoc",
    "array_find",
    "array_intersect_assoc",
    "array_is_list",
    "array_key_first",
    "array_key_last",
    "array_merge_recursive",
    "array_multisort",
    "array_replace",
    "array_replace_recursive",
    "array_udiff",
    "array_uintersect",
    "array_walk_recursive",
    "bindec",
    "decbin",
    "dechex",
    "decoct",
    // The compiler serves `dir()` through the `Directory` class prelude, which eval does not
    // parse: the interpreter would need a Directory cell kind with property reads and method
    // dispatch of its own, the way `hash_init()` needed one for `HashContext`.
    "dir",
    "hexdec",
    // php 8.4's response-header pair reads engine state the interpreter does not own: the
    // compiler answers both from `_http_resp_header_end` / `_http_resp_buf`, which are
    // host-runtime symbols the eval value model has no cell kind for.
    "http_clear_last_response_headers",
    "http_get_last_response_headers",
    "join",
    "octdec",
    "serialize",
    "strncasecmp",
    "strncmp",
    "substr_count",
    "unserialize",
    "zval_free",
    "zval_pack",
    "zval_type",
    "zval_unpack",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{contracts, lookup};

    /// Verifies every catalog entry has one explicit support result per backend.
    #[test]
    fn every_contract_has_a_backend_support_record() {
        let mut eval_registry = 0;
        let mut eval_internal = 0;
        let mut eval_pending = 0;
        let mut aot_registry = 0;
        let mut aot_external = 0;
        let mut aot_unsupported = 0;

        for contract in contracts() {
            match eval_support(contract) {
                BackendSupport::Implemented(BackendImplementation::Registry) => {
                    eval_registry += 1;
                }
                BackendSupport::Unsupported(UnsupportedReason::InternalCompilerSurface) => {
                    eval_internal += 1;
                }
                BackendSupport::Unsupported(UnsupportedReason::EvalImplementationPending) => {
                    eval_pending += 1;
                }
                other => panic!("unexpected eval support for {}: {other:?}", contract.name),
            }
            match aot_support(contract) {
                BackendSupport::Implemented(BackendImplementation::Registry) => {
                    aot_registry += 1;
                }
                BackendSupport::Implemented(_) => aot_external += 1,
                BackendSupport::Unsupported(UnsupportedReason::EvalOnlyReflection) => {
                    aot_unsupported += 1;
                }
                other => panic!("unexpected AOT support for {}: {other:?}", contract.name),
            }
        }

        assert_eq!(eval_registry, 484);
        assert_eq!(eval_internal, 40);
        assert_eq!(eval_pending, 34);
        // 544 on the merged catalogue: this branch counted 543 and main 531, and main also
        // PROMOTES get_object_vars out of the external surface into the registry. Neither
        // branch's number survives the merge; this one is measured on the result.
        assert_eq!(aot_registry, 544);
        assert_eq!(aot_external, 11);
        assert_eq!(aot_unsupported, 3);
    }

    /// Verifies representative exceptional routes are attached to their contracts.
    #[test]
    fn exceptional_backend_routes_are_explicit() {
        assert_eq!(
            aot_support(lookup("hash_init").expect("hash_init contract")),
            BackendSupport::Implemented(BackendImplementation::Prelude)
        );
        assert_eq!(
            aot_support(lookup("get_object_vars").expect("get_object_vars contract")),
            BackendSupport::Implemented(BackendImplementation::Registry)
        );
        assert_eq!(
            eval_support(lookup("array_all").expect("array_all contract")),
            BackendSupport::Unsupported(UnsupportedReason::EvalImplementationPending)
        );
    }

    /// Verifies runtime-only, hybrid, and interpreter adapter routes cover all eval bindings.
    #[test]
    fn every_eval_binding_has_a_documented_execution_route() {
        let mut shared_runtime = 0;
        let mut hybrid_adapter = 0;
        let mut interpreter_adapter = 0;
        let mut unsupported = 0;

        for contract in contracts() {
            match eval_execution(contract) {
                Some(EvalExecution::SharedRuntime(_)) => shared_runtime += 1,
                Some(EvalExecution::Adapter {
                    runtime_builtin: Some(_),
                    reason: EvalAdapterReason::AdditionalSignatureSemantics,
                }) => hybrid_adapter += 1,
                Some(EvalExecution::Adapter {
                    runtime_builtin: None,
                    ..
                }) => interpreter_adapter += 1,
                Some(other) => panic!("invalid eval execution for {}: {other:?}", contract.name),
                None => unsupported += 1,
            }
        }

        assert_eq!(shared_runtime, 19);
        assert_eq!(hybrid_adapter, 2);
        assert_eq!(interpreter_adapter, 463);
        assert_eq!(unsupported, 74);
        assert_eq!(
            eval_execution(lookup("strval").expect("strval contract")),
            Some(EvalExecution::Adapter {
                runtime_builtin: None,
                reason: EvalAdapterReason::DynamicObjectCoercion,
            })
        );
    }
}
