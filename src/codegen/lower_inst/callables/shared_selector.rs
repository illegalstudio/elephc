//! Purpose:
//! Shares boxed-callable descriptor selection across every conversion site with one policy.
//!
//! Called from:
//! - `super::emit_runtime_mixed_callable_descriptor_value_impl()` at each eligible site.
//! - `crate::codegen::finalize_user_asm()`, which drains the queued selector bodies.
//!
//! Key details:
//! - A selector body expands the public candidate inventory once per module (issue 552): the
//!   tag switch, the function-name and static-method comparisons, and the receiver-class
//!   ladders for callable arrays and invokable objects. A site only loads its value and calls it.
//! - The key is the whole conversion policy: operation name, both error surfaces, host arity,
//!   existing-descriptor retain, fresh Closure copy, and the effective and declared name sets.
//! - Creation-scope extras (private and protected methods a lexical class may bind, and the
//!   private shadows of child methods) stay inline at the site and are tried FIRST, so a
//!   shadow still wins. Every miss falls through to the public selector, which reports every
//!   error with the same policy, so a class scope never duplicates the public body.
//! - A site whose function owns an eval context stays inline: a selector cannot reach the
//!   caller's eval slot. The frame-independent `FrameActive` eval fallback is shared.
//! - Selector ABI: a borrowed boxed Mixed in the integer result register in, the descriptor in
//!   the same register out; the nested-call register is preserved. A raise unwinds through
//!   the selector frame to the caller's handler, as in every shared helper.

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_helper::{helper_function, helper_value};
use crate::codegen::shared_state::SharedCodegenState;
use crate::codegen::{frame, platform::Arch};
use crate::ir::Module;

use super::*;

/// Every input that changes the code a boxed-callable selector emits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::codegen) struct SharedCallableSelectorKey {
    op_name: String,
    retain_existing_descriptor: bool,
    string_type_error: Option<CallableConversionError>,
    invalid_type_error: Option<CallableConversionError>,
    arity: Option<usize>,
    fresh_closure: bool,
    /// The site's effective runtime-name set (`FunctionContext::runtime_callable_candidates`).
    candidates: Option<Vec<String>>,
    /// The declared-`callable` set, which also changes the string-name miss path.
    declared_candidates: Option<Vec<String>>,
}

/// The conversion policy a selector site carries; the inline expansion takes the same fields.
#[derive(Clone, Copy)]
pub(super) struct SelectorPolicy<'p> {
    pub(super) op_name: &'p str,
    pub(super) retain_existing_descriptor: bool,
    pub(super) string_type_error: Option<CallableConversionError>,
    pub(super) invalid_type_error: Option<CallableConversionError>,
    pub(super) arity: Option<usize>,
    pub(super) scoped: bool,
    pub(super) fresh_closure: bool,
}

/// Emits a call to the policy's shared selector, or returns `false` when the site must stay
/// inline (a shared selector body, or a function owning an eval context the body cannot reach).
pub(super) fn emit_shared_selector_site(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    policy: SelectorPolicy<'_>,
) -> Result<bool> {
    if ctx.inline_callable_selectors {
        return Ok(false);
    }
    if policy.retain_existing_descriptor
        && matches!(eval_callback_context(ctx), Some(EvalCallbackContext::Slot(_)))
    {
        return Ok(false);
    }
    let key = SharedCallableSelectorKey {
        op_name: policy.op_name.to_string(),
        retain_existing_descriptor: policy.retain_existing_descriptor,
        string_type_error: policy.string_type_error,
        invalid_type_error: policy.invalid_type_error,
        arity: policy.arity,
        fresh_closure: policy.fresh_closure,
        candidates: ctx.runtime_callable_candidates(callable),
        declared_candidates: ctx.declared_callable_candidates.clone(),
    };
    let label = ctx.shared.request_callable_selector(&key);
    let done_label = ctx.next_label("shared_callable_selector_done");
    if policy.scoped && ctx.lexical_class().is_some() {
        emit_scope_extras(ctx, callable, policy, &done_label)?;
    }
    ctx.load_value_to_result(callable)?;
    abi::emit_call_label(ctx.emitter, &label);
    ctx.emitter.label(&done_label);
    Ok(true)
}

/// Emits every selector body requested so far, including those requested while emitting one.
pub(in crate::codegen) fn emit_pending_callable_selectors(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
) -> Result<bool> {
    let mut emitted = false;
    while let Some((key, label)) = shared.next_pending_callable_selector() {
        emit_callable_selector(module, emitter, data, shared, &key, &label)?;
        emitted = true;
    }
    Ok(emitted)
}

/// Emits one selector body: the unscoped inline expansion inside a frame of its own.
fn emit_callable_selector(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    key: &SharedCallableSelectorKey,
    label: &str,
) -> Result<()> {
    let function = helper_function(label, PhpType::Callable);
    let layout = frame::layout_for_function(&function, emitter.target, false, false, false);
    let saved_nested_offset = layout.frame_size - 8;
    let frame_size = layout.frame_size + 16;
    let mut ctx = FunctionContext::new(
        module, &function, emitter, data, shared, layout, false, false, false, None,
    );
    ctx.inline_callable_selectors = true;
    ctx.shared_selector_candidates = key.candidates.clone();
    ctx.declared_callable_candidates = key.declared_candidates.clone();
    ctx.emitter.blank();
    ctx.emitter.comment(&format!("shared callable selector for {}", key.op_name));
    ctx.emitter.label_global(label);
    abi::emit_frame_prologue(ctx.emitter, frame_size);
    let nested = abi::nested_call_reg(ctx.emitter);
    abi::store_at_offset(ctx.emitter, nested, saved_nested_offset);
    ctx.store_result_value(helper_value())?;
    emit_inline_mixed_callable_descriptor_value(
        &mut ctx,
        helper_value(),
        SelectorPolicy {
            op_name: &key.op_name,
            retain_existing_descriptor: key.retain_existing_descriptor,
            string_type_error: key.string_type_error,
            invalid_type_error: key.invalid_type_error,
            arity: key.arity,
            scoped: false,
            fresh_closure: key.fresh_closure,
        },
    )?;
    abi::load_at_offset(ctx.emitter, nested, saved_nested_offset);
    abi::emit_frame_restore(ctx.emitter, frame_size);
    abi::emit_return(ctx.emitter);
    Ok(())
}

/// Returns whether two instance targets bind the same method body for the same receiver class.
pub(super) fn same_instance_target(
    left: &RuntimeArrayInstanceMethodTarget,
    right: &RuntimeArrayInstanceMethodTarget,
) -> bool {
    left.class_id == right.class_id
        && left.method_key == right.method_key
        && left.impl_class == right.impl_class
}

/// Keeps the static cases only the lexical scope can bind.
pub(super) fn scope_only_static_cases(
    ctx: &mut FunctionContext<'_>,
    candidate_names: Option<&[String]>,
) -> Vec<callable_dispatch::RuntimeStaticMethodCallableCase> {
    let public = runtime_static_method_descriptor_cases_with_scope(ctx, candidate_names, false);
    runtime_static_method_descriptor_cases_with_scope(ctx, candidate_names, true)
        .into_iter()
        .filter(|case| {
            !public.iter().any(|existing| existing.case.php_name == case.case.php_name)
        })
        .collect()
}

/// Returns the instance targets only the lexical scope can bind (non-public methods and the
/// private shadows of child methods), leaving out every target the public inventory has.
pub(super) fn scope_only_instance_targets(
    ctx: &FunctionContext<'_>,
) -> Vec<RuntimeArrayInstanceMethodTarget> {
    let public_instance = runtime_array_instance_method_targets_with_scope(ctx, false);
    runtime_array_instance_method_targets_with_scope(ctx, true)
        .into_iter()
        .filter(|target| !public_instance.iter().any(|public| same_instance_target(public, target)))
        .collect()
}

/// Tries the methods only the lexical scope can bind, jumping to `done_label` on a match and
/// falling through (stack balanced) to the public selector call on any miss.
pub(super) fn emit_scope_extras(
    ctx: &mut FunctionContext<'_>,
    callable: ValueId,
    policy: SelectorPolicy<'_>,
    done_label: &str,
) -> Result<()> {
    if ctx.lexical_class().is_none() {
        return Ok(());
    }
    let instance_targets = scope_only_instance_targets(ctx);
    let invokable_targets = instance_targets
        .iter()
        .filter(|target| target.method_key == "__invoke")
        .cloned()
        .collect::<Vec<_>>();
    let static_cases = scope_only_static_cases(ctx, None);
    let candidate_names = ctx.runtime_callable_candidates(callable);
    let string_cases = scope_only_static_cases(ctx, candidate_names.as_deref())
        .into_iter()
        .map(|case| case.case)
        .collect::<Vec<_>>();
    if instance_targets.is_empty() && static_cases.is_empty() && string_cases.is_empty() {
        return Ok(());
    }
    ctx.emitter.comment("creation-scope callable candidates before the shared public selector");
    let string_label = (!string_cases.is_empty()).then(|| ctx.next_label("scope_callable_string"));
    let array_label = (!instance_targets.is_empty() || !static_cases.is_empty())
        .then(|| ctx.next_label("scope_callable_array"));
    let object_label = (!invokable_targets.is_empty()).then(|| ctx.next_label("scope_callable_object"));
    let public_label = ctx.next_label("scope_callable_public");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(callable, "x0")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            if let Some(label) = &string_label {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_STRING)); // classify a scope-private static method name
                abi::emit_branch_if_equal_wide(ctx.emitter, label);             // try the scope's own static method names
            }
            if let Some(label) = &array_label {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_INDEXED_ARRAY)); // classify a callable array
                abi::emit_branch_if_equal_wide(ctx.emitter, label);             // try the scope's own instance and static methods
            }
            if let Some(label) = &object_label {
                ctx.emitter.instruction(&format!("cmp x0, #{}", MIXED_TAG_OBJECT)); // classify an invokable object
                abi::emit_branch_if_equal_wide(ctx.emitter, label);             // try a scope-visible non-public __invoke
            }
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(callable, "rax")?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            if let Some(label) = &string_label {
                ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_STRING)); // classify a scope-private static method name
                ctx.emitter.instruction(&format!("je {}", label));              // try the scope's own static method names
            }
            if let Some(label) = &array_label {
                ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_INDEXED_ARRAY)); // classify a callable array
                ctx.emitter.instruction(&format!("je {}", label));              // try the scope's own instance and static methods
            }
            if let Some(label) = &object_label {
                ctx.emitter.instruction(&format!("cmp rax, {}", MIXED_TAG_OBJECT)); // classify an invokable object
                ctx.emitter.instruction(&format!("je {}", label));              // try a scope-visible non-public __invoke
            }
        }
    }
    abi::emit_jump(ctx.emitter, &public_label);

    if let Some(label) = &string_label {
        ctx.emitter.label(label);
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        if ctx.emitter.target.arch == Arch::X86_64 {
            ctx.emitter.instruction(&format!("mov {}, rdi", ptr_reg));          // move the unboxed string pointer into the string result register
        }
        emit_strip_callable_name_separator(ctx, ptr_reg, len_reg);
        abi::emit_push_reg_pair(ctx.emitter, ptr_reg, len_reg);
        let matched_label = ctx.next_label("scope_callable_string_matched");
        let string_miss = ctx.next_label("scope_callable_string_miss");
        selector_tables::emit_string_case_table_lookup(ctx, &string_cases, &matched_label, &string_miss);
        ctx.emitter.label(&string_miss);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        abi::emit_jump(ctx.emitter, &public_label);
        ctx.emitter.label(&matched_label);
        abi::emit_release_temporary_stack(ctx.emitter, 16);
        if policy.fresh_closure {
            emit_fresh_closure_from_persistent_descriptor(ctx);
        }
        abi::emit_jump(ctx.emitter, done_label);
    }

    if let Some(label) = &array_label {
        ctx.emitter.label(label);
        emit_mixed_callable_array_selector_slots_with_error(
            ctx, &CallableArraySource::BoxedArray(callable), policy.invalid_type_error,
        )?;
        let selected_label = ctx.next_label("scope_callable_array_done");
        let static_selected_label = ctx.next_label("scope_callable_array_static_done");
        let static_label = ctx.next_label("scope_callable_array_static");
        selector_tables::emit_instance_target_table_lookup(
            ctx,
            &instance_targets,
            selector_tables::ReceiverSelector::CallableArray,
            &selected_label,
            &static_label,
        )?;
        ctx.emitter.label(&static_label);
        for case in &static_cases {
            let next_label = ctx.next_label("scope_callable_array_static_next");
            emit_branch_if_mixed_static_case_mismatch(ctx, case, &next_label);
            abi::emit_symbol_address(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &case.case.descriptor_label,
            );
            abi::emit_jump(ctx.emitter, &static_selected_label);
            ctx.emitter.label(&next_label);
        }
        abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
        abi::emit_jump(ctx.emitter, &public_label);
        ctx.emitter.label(&static_selected_label);
        if policy.fresh_closure {
            emit_fresh_closure_from_persistent_descriptor(ctx);
        }
        ctx.emitter.label(&selected_label);
        abi::emit_release_temporary_stack(ctx.emitter, MIXED_SELECTOR_BYTES);
        abi::emit_jump(ctx.emitter, done_label);
    }

    if let Some(label) = &object_label {
        ctx.emitter.label(label);
        emit_push_mixed_unbox_payload(ctx);
        let selected_label = ctx.next_label("scope_callable_object_done");
        let object_miss = ctx.next_label("scope_callable_object_miss");
        selector_tables::emit_instance_target_table_lookup(
            ctx,
            &invokable_targets,
            selector_tables::ReceiverSelector::InvokableObject,
            &selected_label,
            &object_miss,
        )?;
        ctx.emitter.label(&object_miss);
        abi::emit_release_temporary_stack(ctx.emitter, MIXED_VALUE_BYTES);
        abi::emit_jump(ctx.emitter, &public_label);
        ctx.emitter.label(&selected_label);
        abi::emit_release_temporary_stack(ctx.emitter, MIXED_VALUE_BYTES);
        abi::emit_jump(ctx.emitter, done_label);
    }

    ctx.emitter.label(&public_label);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::Target;

    /// Returns the assembly between one function's source-map markers.
    fn function_body<'a>(assembly: &'a str, name: &str) -> &'a str {
        assembly
            .split_once(&format!("@fn name={name} symbol="))
            .unwrap_or_else(|| panic!("missing function {name}"))
            .1
            .split_once(&format!("@endfn name={name}"))
            .unwrap()
            .0
    }

    /// Returns the one shared selector a function body calls.
    fn called_selector(body: &str) -> &str {
        let start = body.find("_eir_callable_selector_").expect("body calls a shared selector");
        let tail = &body[start..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        &tail[..end]
    }

    /// Factory and declared-`callable` conversions in different class scopes and functions
    /// call one selector per policy instead of expanding the candidate inventory per site,
    /// and only a scope with non-public candidates keeps a small inline prefix.
    #[test]
    fn conversions_share_one_selector_per_policy_on_every_target() {
        let source = "<?php
            class Worker {
                public function a(): int { return 1; }
                public function b(): int { return 2; }
                public static function s(): int { return 3; }
            }
            final class First {
                private ?Closure $cb = null;
                public function __construct(callable $cb) { $this->cb = Closure::fromCallable($cb); }
                private function own(): int { return 4; }
                public function total(): int { return $this->own() + ($this->cb)(); }
            }
            final class Second {
                private ?Closure $cb = null;
                public function __construct(callable $cb) { $this->cb = Closure::fromCallable($cb); }
                public function total(): int { return ($this->cb)(); }
            }
            function callFirst(callable $cb): mixed { return $cb(); }
            function callSecond(callable $cb): mixed { return $cb(); }
            echo (new First([new Worker(), 'a']))->total(), (new Second('Worker::s'))->total(),
                callFirst(fn(): int => 5), callSecond([new Worker(), 'b']);";
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                source,
                std::path::Path::new("main.php"),
                std::path::Path::new("."),
                Target::parse(name).unwrap(),
            );
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            let mut selectors = Vec::new();
            for function in ["First::__construct", "Second::__construct", "callFirst", "callSecond"] {
                let body = function_body(&assembly, function);
                assert!(
                    !body.contains("mixed_callable_value_string")
                        && !body.contains("mixed_callable_value_array")
                        && !body.contains("mixed_callable_value_object"),
                    "{name}: {function} must not copy the public candidate selector"
                );
                selectors.push(called_selector(body).to_string());
            }
            assert_eq!(selectors[0], selectors[1], "{name}: both factories share one selector");
            assert_eq!(selectors[2], selectors[3], "{name}: both declared calls share one selector");
            for selector in &selectors {
                assert_eq!(
                    assembly.matches(&format!("{selector}:\n")).count(),
                    1,
                    "{name}: {selector} is emitted once"
                );
            }
            assert!(
                function_body(&assembly, "First::__construct").contains("scope_callable_array"),
                "{name}: First keeps its private method inline"
            );
            assert!(
                !function_body(&assembly, "Second::__construct").contains("scope_callable_"),
                "{name}: Second has no scope-only candidates"
            );
        }
    }
    /// Invoking a `?Closure` value needs no candidate selector, so a program whose only dynamic
    /// call goes through one links neither the argument normalizer nor any builtin wrapper.
    #[test]
    fn nullable_closure_invocations_link_no_selector_on_every_target() {
        let source = "<?php
            final class Job {
                private ?Closure $run = null;
                public function set(Closure $run): void { $this->run = $run; }
                public function go(): mixed { $run = $this->run; return $run === null ? null : $run(); }
            }
            $job = new Job();
            $job->set(fn(): int => 1);
            echo $job->go();";
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = crate::ir_lower::tests::lower_source_at_for_target(
                source,
                std::path::Path::new("main.php"),
                std::path::Path::new("."),
                Target::parse(name).unwrap(),
            );
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            assert!(function_body(&assembly, "Job::go").contains("closure_union_done"), "{name}");
            assert!(
                !assembly.contains("_eir_callable_argument_normalizer:")
                    && !assembly.contains("_callable_builtin_")
                    && !assembly.contains("_eir_callable_selector_"),
                "{name}: a Closure-only invocation must not link a candidate selector"
            );
        }
    }
}
