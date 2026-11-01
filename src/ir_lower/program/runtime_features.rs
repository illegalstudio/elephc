//! Purpose:
//! Runtime-feature discovery from lowered EIR and eval scope state.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;
use crate::ir::{CoreBuiltinOp, ResourceCleanupKind, RuntimeFnId};

/// Adds optional runtime features referenced by synthetic or lowered EIR functions.
pub(in crate::ir_lower) fn include_lowered_runtime_features(module: &mut Module) {
    let features = lowered_runtime_features(module);
    module.required_runtime_features.regex |= features.regex;
    // A runtime string naming `preg_match` / `preg_match_all` (`$match = 'preg_match'; $match(...)`)
    // dispatches to a descriptor wrapper that calls the regex runtime.
    module.required_runtime_features.regex |= module
        .data
        .strings
        .iter()
        .any(|text| {
            ["preg_match", "preg_match_all", "preg_replace", "preg_replace_callback"]
                .iter()
                .any(|name| text.eq_ignore_ascii_case(name))
        });
    module.required_runtime_features.mbstring |= features.mbstring;
    module.required_runtime_features.mbregex |= features.mbregex;
    module.required_runtime_features.mbstring_mime |= features.mbstring_mime;
    module.required_runtime_features.phar_archive |= features.phar_archive;
    module.required_runtime_features.descriptor_invoker |= features.descriptor_invoker;
    module.required_runtime_features.const_introspection |= features.const_introspection;
    module.required_runtime_features.class_introspection |= features.class_introspection;
    module.required_runtime_features.class_relation_introspection |=
        features.class_relation_introspection;
    module.required_runtime_features.class_methods_introspection |=
        features.class_methods_introspection;
    module.required_runtime_features.pdo_udf |= features.pdo_udf;
    module.required_runtime_features.eval_bridge |= features.eval_bridge;
    module.required_runtime_features.eval_scope |= features.eval_scope;
    module.required_runtime_features.popen_resource |= features.popen_resource;
    module.required_runtime_features.directory_resource |= features.directory_resource;
    module.required_runtime_features.handler_state |= features.handler_state;
    module.required_runtime_features.object_clone |= features.object_clone;
    // Not derived from the instruction stream like the rest: a Fiber object can only exist if the
    // builtin class was registered, and `types::checker::builtin_class_gate` has already decided
    // that from the program's own text. Reading the answer here is exact, where scanning EIR for
    // "something that makes a fiber" would be an approximation of it.
    module.required_runtime_features.fiber |= module.class_infos.contains_key("Fiber");
    module.required_runtime_features.generator |= module.class_infos.contains_key("Generator");
    // Also a fact rather than an analysis: `__rt_diag_warning` calls the compiled PHP function
    // `__elephc_diag_render` by its own symbol, so the reference is legal exactly when the
    // program still declares it. Read from the surviving function list — AFTER declaration
    // reachability has pruned — so a build that dropped the dispatch prelude never emits a
    // runtime that refers to it.
    module.required_runtime_features.diag_user_handler |= module
        .functions
        .iter()
        .any(|function| function.name == crate::names::DIAG_RENDER_FUNCTION);
}

/// Returns whether this call is `__elephc_opcache_rt_compile`.
///
/// Matched on the runtime-function id rather than the builtin target, because these helpers
/// are internal registry entries reached through `RuntimeCallTarget::Function`.
fn runtime_call_targets_opcache_compile(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.immediate,
        Some(crate::ir::Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::Function(crate::ir::RuntimeFnId::ElephcOpcacheRtCompile)
        ))
    )
}

/// Returns whether this call asks the ON-DISK cache: the file-cache query, or either half of
/// `opcache_invalidate()`, which removes the disk entry as well as the memory one.
fn runtime_call_reaches_the_file_cache(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.immediate,
        Some(crate::ir::Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(
            crate::ir::RuntimeFnId::ElephcOpcacheRtInFileCache
                | crate::ir::RuntimeFnId::ElephcOpcacheRtDiscard
                | crate::ir::RuntimeFnId::ElephcOpcacheRtSoftInvalidate
        )))
    )
}

/// Returns whether this compilation has an `opcache.file_cache` that is actually live: OPcache
/// enabled and a directory configured. Read from the same compile settings the startup
/// configuration is built from, which the pipeline installs before lowering runs.
fn compile_configures_a_live_file_cache() -> bool {
    let config = crate::opcache::runtime_cache::runtime_cache_config(
        crate::codegen_support::compile_php_version().version_id(),
        crate::codegen_support::compile_is_web_sapi(),
        &crate::codegen_support::ini_overrides(),
    );
    config.enabled && !config.file_cache.is_empty()
}

/// Derives optional runtime features from the actual EIR instruction stream.
pub(super) fn lowered_runtime_features(module: &Module) -> RuntimeFeatures {
    lowered_runtime_features_with(module, true)
}

/// Whether this program can EXECUTE interpreted code: `eval()`, a dynamic include, anything
/// that runs through the interpreter — as opposed to linking it only for an OPcache operation.
///
/// `opcache_compile_file()` and the file-cache operations link the eval bridge (see below) but
/// execute nothing through it: `opcache_compile_file()` parses and caches without running.
/// The post-link note that "evaluated code that uses preg_* will fail" was printed for them
/// too, about code such a program cannot run. Reported by DeepSeek.
///
/// Called only by the compiler binary's pipeline, which the lib target does not include, hence
/// the dead-code allowance there.
#[allow(dead_code)]
pub(crate) fn module_runs_interpreted_code(module: &Module) -> bool {
    lowered_runtime_features_with(module, false).eval_bridge
}

/// [`lowered_runtime_features`], optionally leaving out the OPcache operations' bridge links.
fn lowered_runtime_features_with(module: &Module, count_opcache_links: bool) -> RuntimeFeatures {
    let mut features = RuntimeFeatures::none();
    let explain = std::env::var_os("ELEPHC_EXPLAIN_EVAL_BRIDGE").is_some() || reasons_collecting();
    // Computed lazily and once: only a program that names a file-cache operation pays for it.
    let mut live_file_cache: Option<bool> = None;
    for function in all_lowered_functions(module) {
        features.mbstring |= function_references_callable_runtime(module, function, is_mbstring_callable_name);
        features.mbregex |= function_references_callable_runtime(module, function, is_mbregex_callable_name);
        features.mbstring_mime |= function_references_callable_runtime(
            module,
            function,
            is_mbstring_mime_callable_name,
        );
        if function_contains_eval_scope_state(function) {
            features.eval_scope = true;
        }
        if function_contains_eval_context_state(function) {
            features.eval_bridge = true;
            explain_eval_bridge(explain, function, None, None);
        }
        for (inst_index, inst) in function.instructions.iter().enumerate() {
            // Under ELEPHC_EXPLAIN_EVAL_BRIDGE every instruction that links the bridge is
            // reported, not only the first: the flag is cleared before each one and OR-ed back.
            let bridge_before = features.eval_bridge;
            if explain {
                features.eval_bridge = false;
            }
            match inst.op {
                Op::ClassLikeActivate => {
                    // The activation overlay and its existence helpers must be
                    // emitted even when every source query is a literal.
                    features.class_introspection = true;
                }
                Op::RuntimeCall => {
                    // A quiet property fetch does NOT link the bridge: its mode only matters to a
                    // receiver an eval context owns, and without the bridge there is none, so the
                    // backend drops the scope. Letting it link the bridge put the interpreter into
                    // every program with a `$this->arr[$k] = ...`, and two bridge calls around it.
                    // A closed-world include is answered from the compiled-source table.
                    features.eval_bridge |= !module.closed_world
                        && matches!(
                            inst.immediate,
                            Some(Immediate::RuntimeCall(
                                crate::ir::RuntimeCallTarget::DynamicInclude { .. }
                            ))
                        );
                    // `opcache_compile_file()` is the ONE runtime-tier operation whose job is
                    // to CREATE a cache entry, so it is the one whose pay-for-use fold is a
                    // lie rather than a truth. `is_cached` and `discard` fold to `false` in a
                    // binary with no dynamic tier and that IS the right answer — nothing can
                    // have been cached. `compile_file` folding to `false` instead reports a
                    // refusal reference PHP never issues: it returns `true` and the file is in
                    // the cache, whether or not the program uses `eval()`.
                    //
                    // So calling it is itself a reason to link the bridge. The cost lands
                    // exactly on programs that ask for the dynamic tier, which is what
                    // pay-for-use means — and no wider than that: with OPcache disabled at
                    // compile time the prelude's own gate short-circuits before this call is
                    // ever emitted, so a disabled build never reaches here and stays small.
                    if count_opcache_links && runtime_call_targets_opcache_compile(inst) {
                        features.eval_bridge = true;
                    }
                    // THE SAME ARGUMENT, FOR THE DISK. `is_cached` and `discard` may fold
                    // because nothing can have been cached in a binary with no dynamic tier —
                    // but a configured `opcache.file_cache` is shared with OTHER processes, so
                    // an entry can be on disk whatever this binary did. Folding the file-cache
                    // query to `false` then reported a miss reference never reports, and
                    // folding `opcache_invalidate()` left another process's entry on disk.
                    // MEASURED: a reader with no `eval()` asked about an entry a seed process
                    // left; reference answered `true`, elephc `false`.
                    //
                    // Gated on the compile-time configuration, so a build with no file cache —
                    // php-src's default — still folds and stays small.
                    if count_opcache_links
                        && runtime_call_reaches_the_file_cache(inst)
                        && *live_file_cache.get_or_insert_with(compile_configures_a_live_file_cache)
                    {
                        features.eval_bridge = true;
                    }
                    if let Some(target) = typed_builtin_target(inst) {
                        features.regex |= target.uses_regex_runtime();
                        features.mbstring |= target.uses_mbstring_runtime();
                        features.mbregex |= target.uses_mbregex_runtime();
                        features.mbstring_mime |= target.uses_mbstring_mime_runtime();
                        features.eval_bridge |= matches!(
                            target,
                            crate::ir::RuntimeFnId::Extract
                                // The highlighter lives in the interpreter (it owns the PHP-exact
                                // tokenizer), so both builtins always link the bridge.
                                | crate::ir::RuntimeFnId::HighlightFile
                                | crate::ir::RuntimeFnId::HighlightString
                                | crate::ir::RuntimeFnId::SplAutoloadRegister
                                // Lowered to `__elephc_eval_unregister_spl_autoload` even with no
                                // register call in the program: the bridge owns the callback chain.
                                | crate::ir::RuntimeFnId::SplAutoloadUnregister
                        );
                        features.phar_archive |= target.publishes_phar_symbols()
                            && function_belongs_to_phar_archive_helper_class(function);
                        features.descriptor_invoker |=
                            typed_builtin_requires_descriptor_invoker(function, inst, target);
                        features.const_introspection |= matches!(
                            target,
                            crate::ir::RuntimeFnId::Constant
                                | crate::ir::RuntimeFnId::Defined
                                | crate::ir::RuntimeFnId::EnumExists
                        );
                        features.class_introspection |= matches!(
                            target,
                            crate::ir::RuntimeFnId::ClassExists
                                | crate::ir::RuntimeFnId::InterfaceExists
                                | crate::ir::RuntimeFnId::TraitExists
                        ) && inst
                            .operands
                            .first()
                            .copied()
                            .is_some_and(|operand| !value_is_const_string(module, function, operand));
                        features.class_relation_introspection |= matches!(
                            target,
                            crate::ir::RuntimeFnId::ClassImplements
                                | crate::ir::RuntimeFnId::ClassParents
                                | crate::ir::RuntimeFnId::ClassUses
                        ) && inst
                            .operands
                            .first()
                            .copied()
                            .is_some_and(|operand| !value_is_const_string(module, function, operand));
                        features.class_methods_introspection |= matches!(
                            target,
                            crate::ir::RuntimeFnId::MethodExists
                                | crate::ir::RuntimeFnId::PropertyExists
                        ) && member_exists_call_requires_runtime_registry(module, function, inst);
                        // `get_class_methods()` always answers from the per-class method rows:
                        // the calling scope decides visibility at run time.
                        features.class_methods_introspection |=
                            matches!(target, crate::ir::RuntimeFnId::GetClassMethods);
                        // The resource a builtin boxes is the ONLY way its cleanup kind reaches
                        // `__rt_mixed_free_deep`: eval boxes its own handles as kind 0, and a
                        // callable named by a runtime-unknown string is a fatal, not a dispatch.
                        // So a call in this stream is exactly the condition for the arm.
                        match target.resource_cleanup_kind() {
                            Some(ResourceCleanupKind::PopenPipe) => features.popen_resource = true,
                            Some(ResourceCleanupKind::Directory) => {
                                features.directory_resource = true
                            }
                            Some(ResourceCleanupKind::StreamFd) | None => {}
                        }
                        features.object_clone |= target == RuntimeFnId::CloneWith;
                    }
                }
                Op::CoreBuiltin => {
                    let operation = match inst.immediate {
                        Some(Immediate::I64(value)) => CoreBuiltinOp::from_i64(value),
                        _ => None,
                    };
                    features.handler_state |= matches!(
                        operation,
                        Some(
                            CoreBuiltinOp::RestoreErrorHandler
                                | CoreBuiltinOp::RestoreExceptionHandler
                                | CoreBuiltinOp::SetErrorHandler
                                | CoreBuiltinOp::SetExceptionHandler
                                | CoreBuiltinOp::GetErrorHandler
                                | CoreBuiltinOp::GetExceptionHandler
                        )
                    );
                }
                Op::LanguageConstructCall => {
                    if language_construct_call_requires_eval(module, inst) {
                        features.eval_bridge = true;
                    }
                }
                Op::EvalLiteralCall => {
                    if eval_literal_call_requires_bridge(module, function, inst_index, inst) {
                        features.eval_bridge = true;
                    }
                }
                Op::EvalScopeGet | Op::EvalScopeSet => {
                    features.eval_scope = true;
                }
                Op::EvalFunctionCall
                | Op::EvalFunctionCallArray
                | Op::EvalFunctionExists
                | Op::EvalClassExists
                | Op::EvalConstantExists
                | Op::EvalConstantFetch
                | Op::EvalStaticMethodCall => {
                    features.eval_bridge = true;
                }
                Op::ExprCall => {
                    features.descriptor_invoker = true;
                }
                Op::CallableDescriptorInvoke => {
                    features.descriptor_invoker = true;
                    features.phar_archive |=
                        callable_descriptor_invoke_can_publish_phar(function, inst);
                }
                Op::PdoAdapterAddr => {
                    features.pdo_udf = true;
                }
                Op::ObjectNew => {
                    if object_new_requires_class_introspection(module, function, inst) {
                        features.class_introspection = true;
                    }
                    if object_new_requires_runtime_reflection(module, function, inst) {
                        features.eval_bridge = true;
                    }
                }
                _ => {}
            }
            if explain {
                if features.eval_bridge {
                    explain_eval_bridge(explain, function, Some(inst), Some(module));
                }
                features.eval_bridge |= bridge_before;
            }
        }
    }
    // A CALLABLE naming `method_exists`/`property_exists` -- `Closure::fromCallable(
    // 'property_exists')`, a string callback -- gets its builtin wrapper during codegen, after
    // this scan, and that wrapper probes `__rt_member_exists` like any call with a dynamic
    // member. Nothing in the instruction stream says so, so the name itself decides: Twig's
    // `CoreExtension::getPropertyChecker()` returns exactly that closure, and the program did not
    // link ("Undefined symbols: __rt_member_exists").
    if module.data.strings.iter().any(|name| {
        let name = name.trim_start_matches('\\');
        name.eq_ignore_ascii_case("method_exists") || name.eq_ignore_ascii_case("property_exists")
    }) {
        features.class_methods_introspection = true;
    }
    features
}

/// Reports, under `ELEPHC_EXPLAIN_EVAL_BRIDGE`, what links the eval bridge (the interpreter and
/// its probes) into the program: the function and, when one instruction is the reason, its
/// operation and PHP source line. A fully compiled build must show none.
fn explain_eval_bridge(
    explain: bool,
    function: &Function,
    inst: Option<&crate::ir::Instruction>,
    module: Option<&Module>,
) {
    if !explain {
        return;
    }
    if reasons_collecting() {
        let line = match (inst, module) {
            (Some(inst), Some(module)) => {
                let kind = match inst.op {
                    Op::EvalFunctionCall | Op::EvalFunctionCallArray | Op::EvalFunctionExists => {
                        "call to a function the closed world does not define"
                    }
                    Op::EvalStaticMethodCall => "static call to a class the closed world does not define",
                    Op::EvalConstantFetch | Op::EvalConstantExists => {
                        "constant the closed world does not define"
                    }
                    Op::EvalClassExists => "class lookup the closed world cannot answer",
                    Op::EvalObjectNew => "new of a class the closed world does not define",
                    Op::ObjectNew => "Reflection object built from a runtime class, function or callable",
                    Op::LanguageConstructCall => "eval()",
                    Op::EvalLiteralCall => "literal eval() that needs the interpreter",
                    Op::RuntimeCall => match &inst.immediate {
                        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(id))) => {
                            reasons_runtime_kind(id)
                        }
                        _ => "runtime call that needs the interpreter",
                    },
                    _ => "operation that needs the interpreter",
                };
                let name = bridge_operand_name(inst, module);
                format!(
                    "{kind}\u{1}{}{} (line {})",
                    if name.is_empty() { String::new() } else { format!("{name} in ") },
                    function.name,
                    inst.span.map(|span| span.line).unwrap_or(0)
                )
            }
            _ => format!(
                "function that owns an eval context (an interpreter-backed lookup lowered into it)\u{1}{}",
                function.name
            ),
        };
        BRIDGE_REASONS.with(|reasons| {
            if let Some(reasons) = reasons.borrow_mut().as_mut() {
                reasons.push(line);
            }
        });
    }
    if std::env::var_os("ELEPHC_EXPLAIN_EVAL_BRIDGE").is_none() {
        return;
    }
    match inst {
        Some(inst) => eprintln!(
            "eval-bridge: {} op={:?} imm={:?} name={} line={}",
            function.name,
            inst.op,
            inst.immediate,
            module.map(|module| bridge_operand_name(inst, module)).unwrap_or_default(),
            inst.span.map(|span| span.line).unwrap_or(0)
        ),
        None => eprintln!("eval-bridge: {} (eval context state)", function.name),
    }
}

thread_local! {
    /// While `Some`, every reason the eval bridge gets linked is recorded here (closed-world check).
    static BRIDGE_REASONS: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Whether a closed-world check is currently collecting bridge reasons.
fn reasons_collecting() -> bool {
    BRIDGE_REASONS.with(|reasons| reasons.borrow().is_some())
}

/// The class, function, constant or method name a bridge-linking operation asks the interpreter
/// for. Each operation's data immediate indexes a different pool.
fn bridge_operand_name(inst: &crate::ir::Instruction, module: &Module) -> String {
    let Some(Immediate::Data(data)) = &inst.immediate else { return String::new() };
    let index = data.as_raw() as usize;
    let pool = &module.data;
    match inst.op {
        Op::EvalFunctionCall | Op::EvalFunctionCallArray | Op::EvalFunctionExists => {
            pool.function_names.get(index)
        }
        Op::EvalConstantFetch | Op::EvalConstantExists => pool.global_names.get(index),
        Op::ObjectNew | Op::EvalObjectNew | Op::EvalClassExists => pool.class_names.get(index),
        _ => pool.strings.get(index),
    }
    .cloned()
    .unwrap_or_default()
}

/// Names the runtime function behind a bridge-linking `RuntimeCall` for the closed-world report.
fn reasons_runtime_kind(id: &RuntimeFnId) -> &'static str {
    match id {
        RuntimeFnId::SplAutoloadRegister => {
            "spl_autoload_register() (the autoload chain lives in the interpreter)"
        }
        RuntimeFnId::SplAutoloadUnregister => {
            "spl_autoload_unregister() (the autoload chain lives in the interpreter)"
        }
        RuntimeFnId::Extract => "extract() (shares the caller's scope with the interpreter)",
        RuntimeFnId::ElephcOpcacheRtCompile => "opcache_compile_file() at run time",
        _ => "runtime call that needs the interpreter",
    }
}

/// Lists every reason the finished module still needs the eval bridge (the interpreter), as
/// grouped, counted `kind (N sites: ...)` lines. Empty when nothing does.
///
/// A closed-world build (`--closed-world`) must link no interpreter, so the pipeline calls this
/// right after lowering and refuses the build with these lines instead of linking it silently.
pub(crate) fn closed_world_bridge_reasons(module: &Module) -> Vec<String> {
    BRIDGE_REASONS.with(|reasons| *reasons.borrow_mut() = Some(Vec::new()));
    let features = lowered_runtime_features_with(module, true);
    let raw = BRIDGE_REASONS
        .with(|reasons| reasons.borrow_mut().take())
        .unwrap_or_default();
    let mut grouped: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for line in raw {
        let (kind, site) = line.split_once('\u{1}').unwrap_or((line.as_str(), ""));
        grouped.entry(kind.to_string()).or_default().push(site.to_string());
    }
    let mut out = Vec::new();
    for (kind, mut sites) in grouped {
        sites.sort();
        sites.dedup();
        // `ELEPHC_CLOSED_WORLD_ALL_SITES=1` lists every site instead of the first four.
        let limit = if std::env::var_os("ELEPHC_CLOSED_WORLD_ALL_SITES").is_some() {
            usize::MAX
        } else {
            4
        };
        let shown = sites.iter().take(limit).cloned().collect::<Vec<_>>().join(", ");
        let more = if sites.len() > limit {
            format!(", ... {} more", sites.len() - limit)
        } else {
            String::new()
        };
        out.push(format!(
            "{kind} ({} site{}: {shown}{more})",
            sites.len(),
            if sites.len() == 1 { "" } else { "s" }
        ));
    }
    if out.is_empty() && (features.eval_bridge || module.required_runtime_features.eval_bridge) {
        out.push(
            "the eval bridge was requested by an earlier compiler stage (no lowered instruction names it)"
                .to_string(),
        );
    }
    out
}

/// Returns whether a descriptor invocation may select a string-named PHAR-aware file builtin.
fn callable_descriptor_invoke_can_publish_phar(
    function: &Function,
    inst: &crate::ir::Instruction,
) -> bool {
    inst.operands
        .first()
        .and_then(|callable| function.value(*callable))
        .is_some_and(|value| {
            matches!(
                value.php_type.codegen_repr(),
                PhpType::Str | PhpType::Mixed | PhpType::Union(_)
            )
        })
}

/// Returns true when a Reflection owner constructor depends on runtime-only metadata.
fn object_new_requires_runtime_reflection(
    module: &Module,
    function: &Function,
    inst: &crate::ir::Instruction,
) -> bool {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    let Some(class_name) = module.data.class_names.get(data.as_raw() as usize) else {
        return false;
    };
    if !matches!(
        php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
        "reflectionclass"
            | "reflectionobject"
            | "reflectionfunction"
            | "reflectionmethod"
            | "reflectionproperty"
            | "reflectionparameter"
            | "reflectionclassconstant"
            | "reflectionenum"
            | "reflectionenumunitcase"
            | "reflectionenumbackedcase"
    ) {
        return false;
    }
    // A closed-world build answers every class reflector from the module reflection table.
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    if module.closed_world
        && (matches!(
            class_key.as_str(),
            "reflectionclass" | "reflectionobject" | "reflectionenum" | "reflectionfunction"
        ) || crate::reflection_table::table_member_class(&class_key))
    {
        return false;
    }
    // A table member reflector is answered by its row when the class is a literal the program
    // compiled, or when a table-built class reflector constructs it from its name lists.
    if crate::reflection_table::enabled()
        && function.flags.is_synthetic
        && function.name.starts_with("Reflection")
        && (class_key == "reflectionclass"
            || class_key == "reflectionenum"
            || class_key == "reflectionparameter"
            || class_key == "reflectionfunction"
            || crate::reflection_table::table_member_class(&class_key))
    {
        return false;
    }
    if crate::reflection_table::enabled()
        && class_key == "reflectionfunction"
        && inst.operands.len() == 1
        && literal_function_operand(module, function, inst.operands[0])
    {
        return false;
    }
    if crate::reflection_table::enabled() && crate::reflection_table::table_member_class(&class_key) {
        if inst.operands.len() == 2 && literal_class_like_operand(module, function, inst.operands[0]) {
            return false;
        }
    }
    // `new ReflectionClass(Foo::class)` of a class-like the program compiled is answered by the
    // module reflection table, whatever representation wrapper the constructor's `mixed`
    // parameter put around the literal.
    if crate::reflection_table::enabled()
        && matches!(
            php_symbol_key(class_name.trim_start_matches('\\')).as_str(),
            "reflectionclass" | "reflectionenum"
        )
        && inst.operands.len() == 1
        && literal_class_like_operand(module, function, inst.operands[0])
    {
        return false;
    }
    inst.operands.iter().any(|operand| {
        function.value(*operand).is_some_and(|value| {
            let ty = value.php_type.codegen_repr();
            matches!(
                ty,
                PhpType::Callable | PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_)
            ) || (ty == PhpType::Str && !value_is_const_string(module, function, *operand))
        })
    })
}

/// Whether `value` is a literal string naming a function the module compiled or a builtin the
/// reflection tables know.
fn literal_function_operand(module: &Module, function: &Function, mut value: crate::ir::ValueId) -> bool {
    let text = loop {
        let Some(inst) = function.instructions.iter().find(|inst| inst.result == Some(value)) else {
            return false;
        };
        match (inst.op, inst.immediate.as_ref()) {
            (Op::MixedBox | Op::MixedClone | Op::Acquire, _) => match inst.operands.first() {
                Some(source) => value = *source,
                None => return false,
            },
            (Op::ConstStr, Some(Immediate::Data(data))) => {
                break module.data.strings.get(data.as_raw() as usize);
            }
            _ => return false,
        }
    };
    let Some(text) = text else { return false };
    let key = php_symbol_key(text.trim_start_matches('\\'));
    module
        .functions
        .iter()
        .chain(module.closures.iter())
        .any(|candidate| php_symbol_key(candidate.name.trim_start_matches('\\')) == key)
        || crate::types::first_class_callable_builtin_sig(&key).is_some()
}

/// Whether `value` is a literal (through representation wrappers) naming a class-like the module
/// compiled: a class, an enum, an interface or a trait.
fn literal_class_like_operand(
    module: &Module,
    function: &Function,
    mut value: crate::ir::ValueId,
) -> bool {
    let text = loop {
        let Some(inst) = function.instructions.iter().find(|inst| inst.result == Some(value)) else {
            return false;
        };
        match (inst.op, inst.immediate.as_ref()) {
            (Op::MixedBox | Op::MixedClone | Op::Acquire, _) => match inst.operands.first() {
                Some(source) => value = *source,
                None => return false,
            },
            (Op::ConstStr, Some(Immediate::Data(data))) => {
                break module.data.strings.get(data.as_raw() as usize);
            }
            (Op::ConstClassName, Some(Immediate::Data(data))) => {
                break module.data.class_names.get(data.as_raw() as usize);
            }
            _ => return false,
        }
    };
    let Some(text) = text else { return false };
    let key = php_symbol_key(text.trim_start_matches('\\'));
    if key == "static" || key == "self" || key == "parent" {
        return false;
    }
    module.class_infos.keys().any(|name| php_symbol_key(name) == key)
        || module.interface_infos.keys().any(|name| php_symbol_key(name) == key)
        || module.enum_infos.keys().any(|name| php_symbol_key(name) == key)
        || module.trait_table.names.iter().any(|name| php_symbol_key(name) == key)
}

/// Returns true when a dynamic Reflection member constructor needs class lookup tables.
fn object_new_requires_class_introspection(
    module: &Module,
    function: &Function,
    inst: &crate::ir::Instruction,
) -> bool {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    let Some(class_name) = module.data.class_names.get(data.as_raw() as usize) else {
        return false;
    };
    if class_name != "ReflectionMethod" && class_name != "ReflectionProperty" {
        return false;
    }
    inst.operands
        .iter()
        .take(2)
        .any(|&operand| !value_is_const_string(module, function, operand))
}

/// Returns whether a lowered value is produced by a literal string instruction.
fn value_is_const_string(module: &Module, function: &Function, value: crate::ir::ValueId) -> bool {
    function
        .instructions
        .iter()
        .find(|inst| inst.result == Some(value))
        .is_some_and(|inst| match inst.op {
            Op::ConstStr => true,
            // `static::class` is the late-static class of the running call, not a literal: the
            // backend resolves its name at run time, so it needs the same registry as any other
            // computed string.
            Op::ConstClassName => !matches!(
                inst.immediate,
                Some(Immediate::Data(data))
                    if module.data.class_names.get(data.as_raw() as usize).map(String::as_str)
                        == Some("static")
            ),
            _ => false,
        })
}

/// Returns whether member-existence lowering needs the closed-world runtime registry.
fn member_exists_call_requires_runtime_registry(
    module: &Module,
    function: &Function,
    inst: &crate::ir::Instruction,
) -> bool {
    let dynamic_member = inst
        .operands
        .get(1)
        .copied()
        .is_some_and(|operand| !value_is_const_string(module, function, operand));
    let gradual_target = inst
        .operands
        .first()
        .and_then(|operand| function.value(*operand))
        .is_some_and(|value| {
            matches!(
                value.php_type.codegen_repr(),
                crate::types::PhpType::Mixed
                    | crate::types::PhpType::Union(_)
                    | crate::types::PhpType::Object(_)
            )
        });
    let dynamic_string_target = inst
        .operands
        .first()
        .copied()
        .is_some_and(|operand| {
            matches!(
                function.value(operand).map(|value| value.php_type.codegen_repr()),
                Some(crate::types::PhpType::Str)
            ) && !value_is_const_string(module, function, operand)
        });
    dynamic_member || gradual_target || dynamic_string_target
}

/// Returns true when a lowered function owns hidden eval scope handle slots.
/// Scope-only functions use the native scope helpers and must not force the
/// magician bridge staticlib into the link.
pub(super) fn function_contains_eval_scope_state(function: &Function) -> bool {
    function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::EvalScope | LocalKind::EvalGlobalScope
        )
    })
}

/// Returns true when a lowered function owns an interpreter context handle,
/// which requires the full magician eval bridge runtime.
pub(super) fn function_contains_eval_context_state(function: &Function) -> bool {
    function
        .locals
        .iter()
        .any(|local| matches!(local.kind, LocalKind::EvalContext))
}

/// Returns true when a literal eval call still needs the magician bridge runtime.
pub(crate) fn eval_literal_call_requires_bridge(
    module: &Module,
    function: &Function,
    inst_index: usize,
    inst: &crate::ir::Instruction,
) -> bool {
    let (data, strict_php) = match inst.immediate {
        Some(Immediate::Data(data)) => (data, false),
        Some(Immediate::ProfiledData { data, strict_php }) => (data, strict_php),
        _ => return true,
    };
    let Some(fragment) = module.data.strings.get(data.as_raw() as usize) else {
        return true;
    };
    let plan = crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
        fragment,
        module.source_path.as_deref(),
        strict_php,
        |name, args| eval_literal_static_function_supported_by_module(module, name, args),
        |receiver, method, args| {
            eval_literal_static_method_supported_by_module(module, receiver, method, args)
        },
    );
    if plan.uses_scope_read_params() {
        return !eval_literal_call_can_use_scope_read_params(module, function, inst_index, &plan);
    }
    if plan.requires_runtime_eval_scope()
        && !eval_literal_call_scope_constraints_supported(module, function, inst_index, &plan)
    {
        return true;
    }
    plan.requires_runtime_eval_bridge()
}

/// Returns true when a local slot is initialized before the eval instruction.
pub(super) fn eval_scope_read_slot_initialized(
    function: &Function,
    slot: crate::ir::LocalSlotId,
    inst_index: usize,
) -> bool {
    if function
        .params
        .get(slot.as_raw() as usize)
        .is_some_and(|param| !param.by_ref)
    {
        return true;
    }
    function
        .instructions
        .iter()
        .take(inst_index)
        .any(|inst| inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot)))
}

/// Returns true when a read-only eval call can pass direct Mixed params safely.
pub(super) fn eval_literal_call_can_use_scope_read_params(
    module: &Module,
    function: &Function,
    inst_index: usize,
    plan: &crate::eval_aot::EvalAotPlan,
) -> bool {
    plan.reads().iter().all(|name| {
        eval_literal_call_scope_read_param_supported(
            module,
            function,
            inst_index,
            name,
            plan.quiet_reads().contains(name),
        )
    }) && plan.array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_array_param_supported(module, function, inst_index, name)
    }) && plan.assoc_array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_assoc_array_param_supported(module, function, inst_index, name)
    }) && plan.float_predicate_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_float_predicate_param_supported(
            module, function, inst_index, name,
        )
    })
}

/// Returns true when scope-based eval AOT satisfies caller-side type constraints.
pub(super) fn eval_literal_call_scope_constraints_supported(
    module: &Module,
    function: &Function,
    inst_index: usize,
    plan: &crate::eval_aot::EvalAotPlan,
) -> bool {
    plan.array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_array_param_supported(module, function, inst_index, name)
    }) && plan.assoc_array_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_assoc_array_param_supported(module, function, inst_index, name)
    }) && plan.float_predicate_read_constraints().iter().all(|name| {
        eval_literal_call_scope_read_float_predicate_param_supported(
            module, function, inst_index, name,
        )
    })
}

/// Returns true when one caller read is initialized and can be boxed directly.
pub(super) fn eval_literal_call_scope_read_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
    quiet: bool,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return quiet;
    };
    eval_scope_read_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when one caller read is initialized with an array-compatible type.
pub(super) fn eval_literal_call_scope_read_array_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return false;
    };
    eval_scope_read_array_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when one caller read is initialized with an associative-array type.
pub(super) fn eval_literal_call_scope_read_assoc_array_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return false;
    };
    eval_scope_read_assoc_array_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when one caller read can feed IEEE float predicates safely.
pub(super) fn eval_literal_call_scope_read_float_predicate_param_supported(
    _module: &Module,
    function: &Function,
    inst_index: usize,
    name: &str,
) -> bool {
    if crate::superglobals::is_superglobal(name) {
        return false;
    }
    let Some(slot) = eval_scope_local_slot(function, name) else {
        return false;
    };
    eval_scope_read_float_predicate_param_type_supported(&slot.php_type)
        && eval_scope_read_slot_initialized(function, slot.id, inst_index)
}

/// Returns true when a caller local can be boxed into a direct eval read param.
pub(super) fn eval_scope_read_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Str
            | PhpType::Void
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Returns true when a caller local satisfies array-only read-param semantics.
pub(super) fn eval_scope_read_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. }
    )
}

/// Returns true when a caller local satisfies associative-array-only semantics.
pub(super) fn eval_scope_read_assoc_array_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::AssocArray { .. })
}

/// Returns true when a caller local can feed IEEE float predicates without TypeError.
pub(super) fn eval_scope_read_float_predicate_param_type_supported(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Int | PhpType::Float)
}

/// Returns the caller local slot that can provide a direct scope-read parameter.
pub(super) fn eval_scope_local_slot<'a>(
    function: &'a Function,
    name: &str,
) -> Option<&'a crate::ir::LocalSlot> {
    function
        .locals
        .iter()
        .find(|local| local.name.as_deref() == Some(name) && local.kind == LocalKind::PhpLocal)
}

/// Returns true when a static function call matches the codegen-supported subset.
pub(super) fn eval_literal_static_function_supported_by_module(
    module: &Module,
    name: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let Some(function) = module
        .functions
        .iter()
        .find(|function| php_symbol_key(function.name.trim_start_matches('\\')) == key)
    else {
        return false;
    };
    let Some(signature) = &function.signature else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Returns true when a static method call matches the codegen-supported subset.
pub(super) fn eval_literal_static_method_supported_by_module(
    module: &Module,
    receiver: &StaticReceiver,
    method: &str,
    args: &[Expr],
) -> bool {
    if args.len() > 6 {
        return false;
    }
    let StaticReceiver::Named(class_name) = receiver else {
        return false;
    };
    let class_name = class_name.as_str().trim_start_matches('\\');
    let method_key = php_symbol_key(method);
    let Some(receiver_info) = module.class_infos.get(class_name) else {
        return false;
    };
    if receiver_info
        .static_method_visibilities
        .get(&method_key)
        .unwrap_or(&Visibility::Public)
        != &Visibility::Public
    {
        return false;
    }
    let impl_class = receiver_info
        .static_method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    let Some(signature) = module
        .class_infos
        .get(impl_class)
        .and_then(|class_info| class_info.static_methods.get(&method_key))
    else {
        return false;
    };
    crate::eval_aot::static_function_signature_supported(signature, args)
}

/// Finds a semantic runtime family in callable instructions without enabling unrelated callbacks.
fn function_references_callable_runtime(module: &Module, function: &Function, accepts: fn(&str) -> bool) -> bool {
    let names = std::cell::OnceCell::new();
    function.instructions.iter().any(|inst| {
        if inst.op == Op::FirstClassCallableNew {
            let data = match inst.immediate {
                Some(Immediate::Data(data) | Immediate::ProfiledData { data, .. }) => data,
                _ => return false,
            };
            return module.data.strings.get(data.as_raw() as usize)
                .is_some_and(|name| accepts(name));
        }
        let index = match inst.op {
            Op::NormalizeCallable | Op::CallablePtr | Op::ExprCall | Op::CallableDescriptorInvoke => 0,
            Op::RuntimeCall => match typed_builtin_target(inst)
                .and_then(|target| target.string_callback_operand_index()) {
                    Some(index) => index,
                    None => return false,
                },
            _ => return false,
        };
        let Some(value) = inst.operands.get(index) else { return false; };
        let names = names.get_or_init(||
            crate::codegen::callable_reachability::CallableReachabilityAnalysis::new(module, function));
        names.candidates(*value).is_some_and(|names|
            names.iter().any(|name| accepts(name)))
    })
}

/// Resolves a callable name through the semantic registry to its optional runtime family.
fn is_mbstring_callable_name(name: &str) -> bool {
    crate::builtins::registry::lookup(name).is_some_and(|def| {
        matches!(def.spec.semantics.runtime_functions,
            crate::builtins::semantics::BuiltinRuntimeFunctions::One(target)
                if target.uses_mbstring_runtime())
    })
}

/// Resolves matching callables through their typed operation instead of a second PHP-name inventory.
fn is_mbregex_callable_name(name: &str) -> bool {
    crate::builtins::registry::lookup(name).is_some_and(|def| {
        matches!(def.spec.semantics.runtime_functions,
            crate::builtins::semantics::BuiltinRuntimeFunctions::One(target)
                if target.uses_mbregex_runtime())
    })
}

/// Resolves output-handler callables through the typed MIME-provider operation.
fn is_mbstring_mime_callable_name(name: &str) -> bool {
    crate::builtins::registry::lookup(name).is_some_and(|def| {
        matches!(def.spec.semantics.runtime_functions,
            crate::builtins::semantics::BuiltinRuntimeFunctions::One(target)
                if target.uses_mbstring_mime_runtime())
    })
}

/// Whether any lowered body contains a runtime `include`/`require` (an internal dynamic include).
pub(crate) fn module_has_dynamic_include(module: &Module) -> bool {
    all_lowered_functions(module).any(|function| {
        function.instructions.iter().any(|inst| {
            matches!(
                inst.immediate,
                Some(Immediate::RuntimeCall(
                    crate::ir::RuntimeCallTarget::DynamicInclude { .. }
                ))
            )
        })
    })
}
