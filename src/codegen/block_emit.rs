//! Purpose:
//! Walks EIR basic blocks in function order and delegates instruction/terminator lowering.
//! Owns native function, method, generator, and program-entry setup.
//!
//! Called from:
//! - `crate::codegen::generate_user_asm_from_ir()`.
//!
//! Key details:
//! - PHP source ranges end before separately emitted cleanup helpers; generator
//!   constructors are synthetic, while their bodies carry PHP debug locations.
//! - The main prologue initializes supported static-property storage before
//!   user blocks run.
//! - Executable and library PHP frames publish local cleanup activations so a throw
//!   releases abandoned frame owners before control reaches the surviving catch.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};
use std::time::Instant;
use std::fmt::Write as _;

use crate::codegen::abi;
use crate::codegen::data_section::DataSection;
use crate::codegen_support::data_section::DataWord;
use crate::codegen::emit::Emitter;
use crate::codegen::emit_fiber_wrapper;
use crate::codegen::platform::Arch;
use crate::codegen::Emit;
use crate::codegen::WebIsolation;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::codegen_support::DeferredFiberWrapper;
use crate::ir::{BasicBlock, Function, InstId, Module, Op};
use crate::names::{
    function_epilogue_symbol, function_symbol, method_symbol, php_symbol_key,
    static_method_symbol, static_property_symbol,
};
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

use super::block_layout;
use super::context::FunctionContext;
use super::machine_peephole;
use super::fibers;
use super::frame;
use super::function_variants;
use super::literal_defaults::{
    emit_array_literal_default_to_result, emit_assoc_array_literal_default_to_result,
    emit_boxed_bool_literal_to_result, emit_boxed_float_literal_to_result,
    emit_boxed_int_literal_to_result, emit_boxed_null_literal_to_result,
    emit_boxed_string_literal_default_to_result, emit_empty_assoc_array_literal_to_result,
    emit_string_literal_default_to_result, emit_tagged_int_literal_to_result,
    emit_tagged_null_literal_to_result, literal_default_value, LiteralDefaultValue,
};
use super::lower_inst;
use super::lower_term;
use super::shared_state::SharedCodegenState;
use super::{CodegenIrError, Result};

/// The hosted-extension engine's process-exit routine (RSHUTDOWN, then MSHUTDOWN), called by
/// every exit sequence of a program that declares it.
const HOSTED_EXIT_HOOK: &str = "elephc_php_ext_exit_hook";
/// The hosted-extension engine's request shutdown (RSHUTDOWN), called by the `--web` reset after
/// the previous request's destructors and before its hosting tables are released.
pub(super) const HOSTED_REQUEST_SHUTDOWN: &str = "elephc_php_ext_request_shutdown";
/// The hosted-extension engine's request startup (RINIT), called at the start of every `--web`
/// request once the reset is done.
pub(super) const HOSTED_REQUEST_STARTUP: &str = "elephc_php_ext_request_startup";

/// Returns whether the program calls into a hosted PHP extension: reachability keeps an
/// `elephc_php_ext_*` engine extern only when a call that reaches the engine survives. The
/// lifecycle routines themselves are never called from PHP, so their own declarations are pruned.
pub(super) fn module_hosts_php_ext(module: &Module) -> bool {
    module
        .extern_decls
        .iter()
        .any(|decl| decl.name.trim_start_matches('\\').starts_with("elephc_php_ext_"))
}


/// Fails on a body that did not lower, or records it when surveying (see `ELEPHC_CODEGEN_SURVEY`).
fn record_or_fail(
    emitted: Result<()>,
    survey: bool,
    surveyed: &mut Vec<String>,
) -> Result<()> {
    match emitted {
        Ok(()) => Ok(()),
        Err(err) if survey => {
            surveyed.push(err.to_string());
            Ok(())
        }
        Err(err) => Err(err),
    }
}

/// Emits EIR bodies and the process entry, retaining shared state for deferred helper emission.
///
/// `web` restructures the entry point: the top-level body is emitted as the
/// C-callable `_elephc_web_handler` and the real entry becomes a stub that calls
/// the bridge entry selected by `web_isolation`. When false the normal
/// exit-based main is emitted unchanged.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_module(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    gc_stats: bool,
    counters: bool,
    instrument: crate::codegen::Instrumentation,
    probe: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    emit: Emit,
    regalloc_linear: bool,
    web: bool,
    web_isolation: WebIsolation,
) -> Result<SharedCodegenState> {
    // A program hosting PHP extensions declares the engine's shutdown routine; every
    // process-exit sequence emitted below then runs it (RSHUTDOWN, MSHUTDOWN) after output
    // is flushed, as php-src does. Executables only: a library never exits the process.
    if emit == Emit::Executable && module_hosts_php_ext(module) {
        emitter.exit_hook = Some(emitter.target.extern_symbol(HOSTED_EXIT_HOOK));
    }
    // Folding compares isolated emissions built with diagnostics off, so it must only run when
    // the FINAL emission is diagnostics-off too (captured before `instrument` moves into the
    // shared state below).
    let diagnostics_off = !counters
        && instrument == crate::codegen::Instrumentation::Off
        && !probe
        && !heap_debug
        && !gc_stats
        // The isolated comparison builds a plain `Emitter::new(target)`; cdylib and staticlib
        // emissions render PIC operands through a different emitter configuration, so a fold
        // decided on the plain text would gate an emission it did not measure. Folding is a
        // size optimization for executables; library builds keep every body.
        && emit == Emit::Executable;
    let size_trace = std::env::var_os("ELEPHC_CODEGEN_SIZE_TRACE").is_some();
    // `ELEPHC_CODEGEN_SURVEY=1` keeps emitting after a body fails and reports every backend error
    // in one pass. Codegen is the LAST phase: a `--web` build of the Symfony app spends twelve
    // minutes in the front end before the first instruction is lowered, so learning one blocker
    // per run is the expensive way to learn five. Off by default -- a real build must still stop
    // at the first error, because everything after it is emitted against half-registered state.
    let survey = std::env::var_os("ELEPHC_CODEGEN_SURVEY").is_some();
    // Diagnostic: how many bodies could have been emitted without the shared caches at all.
    let count_shared_touches = std::env::var_os("ELEPHC_CODEGEN_SHARED_TOUCHES").is_some();
    let mut bodies_touching_shared = 0usize;
    let mut bodies_total = 0usize;
    let mut surveyed: Vec<String> = Vec::new();
    let mut shared = SharedCodegenState::for_module(module);
    shared.counters = counters;
    // Read before the move: the parallel pass below stays off while instrumentation is on,
    // because the id a body is given depends on the order bodies are emitted in.
    let instrumented = !matches!(instrument, crate::codegen::Instrumentation::Off);
    shared.instrument = instrument;
    if probe {
        let main_symbol = if web {
            frame::WEB_HANDLER_SYMBOL
        } else {
            emitter.entry_symbol()
        };
        shared.probe_table = Some(emit_probe_symbol_table(module, data, main_symbol));
        if let Some(key) = module.probe_key {
            let symbol = emitter.target.extern_symbol("elephc_probe_build_key");
            data.add_named_symbol(symbol, &key);
        }
    }
    let mut inventory = BackendInventory::from_env();
    super::source_units::emit_state_helpers(module, emitter, data);
    function_variants::emit_dispatchers(module, emitter, data);
    super::shared_mbstring_callable::emit(module, emitter, data, &mut shared)?;
    // Emitted before the module's own bodies so every string context that calls them is
    // lowered against helpers that already exist.
    super::shared_mixed_string::emit_shared_mixed_string_helpers(
        module,
        emitter,
        data,
        &mut shared,
        regalloc_linear,
    )?;
    super::shared_mixed_callable::emit_shared_mixed_callable_helpers(
        module,
        emitter,
        data,
        &mut shared,
        regalloc_linear,
    )?;
    super::shared_count_guard::emit_shared_count_guard(
        module,
        emitter,
        data,
        &mut shared,
        regalloc_linear,
    )?;
    super::shared_static_throw::emit_shared_static_throw(
        module,
        emitter,
        data,
        &mut shared,
        regalloc_linear,
    )?;
    super::shared_null_method_trace::emit_shared_null_method_trace_helper(
        module,
        emitter,
        data,
        &mut shared,
        regalloc_linear,
    )?;
    super::shared_type_error_given::emit_shared_type_error_given(
        module,
        emitter,
        data,
        &mut shared,
        regalloc_linear,
    )?;
    super::shared_eval_status::emit_shared_eval_status(
        module,
        emitter,
        data,
        &mut shared,
        regalloc_linear,
    )?;
    super::lower_inst::emit_reflection_class_table(module, emitter, data, &mut shared, regalloc_linear)?;
    super::lower_inst::emit_shared_eval_registration_helper(
        module,
        emitter,
        data,
        &mut shared,
        regalloc_linear,
    )?;
    // In `--web` builds the reset routine references every request superglobal.
    // If a superglobal is never read or written by user/prelude code, the symbol
    // would otherwise be missing from the object, so reserve storage up front.
    if web {
        let sg_type = crate::superglobals::superglobal_type();
        let sg_size = sg_type.codegen_repr().stack_size().max(8);
        for name in crate::superglobals::SUPERGLOBALS {
            data.add_comm(crate::names::ir_global_symbol(name), sg_size);
        }
    }
    // A folded candidate keeps its EIR function — so a caller still resolves its parameter types
    // and picks its own coercion from the original instantiation — but emits a jump to the
    // survivor's body. The map is computed from the shape-normalized assembly oracle, so only
    // bodies that lower to the same machine code are folded.
    //
    // Diagnostics modes are excluded: counters and instrumentation embed PER-NAME state into the
    // final emission that the isolated comparison (which runs with diagnostics off) cannot see,
    // so a folded victim would attribute its calls to the survivor or lose them entirely. Heap
    // debug, probes and gc stats are gated the same conservative way — folding is a size
    // optimization, and those modes exist to observe per-body behavior.
    let body_aliases = if diagnostics_off {
        generic_body_aliases(module, regalloc_linear)
    } else {
        Default::default()
    };
    let bodies = module_bodies(module, &body_aliases);
    // A body that needs nothing from `shared` can be emitted on any thread; one that reaches a
    // cache cannot, because the helper behind that cache belongs to the module exactly once.
    // The split is discovered by emitting and rolling back, so no predicate has to be kept in
    // step with the lowering. Every diagnostic mode below depends on emission ORDER, so each one
    // keeps the single pass.
    let jobs = codegen_jobs();
    let mut serial_bodies: Vec<usize> = (0..bodies.len()).collect();
    // Shared helpers already emitted, by key. The serial pass shares it with the workers: a
    // helper a worker produced must not be produced again here, or the assembler sees the same
    // symbol defined twice.
    let mut seen_helpers: HashSet<String> = HashSet::default();
    // The module-wide helpers emitted above the body loop (the shared Mixed string ladder, the
    // shared callable dispatch, the count guard) can already hold a keyed helper. Seed the set
    // from what is in the buffer, or a worker emits a second copy of it.
    collect_helper_keys(emitter.text_from(0), &mut seen_helpers);
    // Out-of-line functions (descriptor invokers, the named-argument normalizer) are not in the
    // buffer until the very end, but they carry helper markers too.
    collect_helper_keys(emitter.out_of_line_from(0), &mut seen_helpers);
    let mut parallel_ran = false;
    if count_shared_touches {
        eprintln!(
            "[elephc-codegen-shared] parallel gate: jobs={} bodies={} min={} survey={} inventory={} counters={} instrumented={}",
            jobs,
            bodies.len(),
            parallel_min_bodies(),
            survey,
            inventory.is_enabled(),
            counters,
            instrumented
        );
    }
    // The backend inventory no longer forces the single pass: a worker hands a body that fails
    // to the serial pass below (`emit_chunk`), whose inventory records its refusal in order. It
    // is on in every build that may defer class files to the runtime autoloader, so keeping the
    // single pass for it emitted every body of the Symfony demo serially.
    if jobs > 1
        && bodies.len() >= parallel_min_bodies()
        && !survey
        && !counters
        && !instrumented
    {
        let worker_pass_started = Instant::now();
        super::context::prefill_module_facts(module, &mut shared);
        let preset = shared.module_preset();
        let template = emitter.fresh();
        let mut data_index = data.merge_index();
        let data_base = data.base();
        let (mut kept, mut deferred_bytes) = (0usize, 0usize);
        let mut postponed_helpers = Vec::new();
        serial_bodies.clear();
        // Chunks arrive in any order and are merged in chunk order, each as soon as every earlier
        // one is in: the text reaches the spill file while the workers are still running, instead
        // of the whole program's text waiting in memory for the last worker.
        emit_bodies_in_parallel(
            module,
            &bodies,
            &template,
            &preset,
            &data_base,
            jobs,
            regalloc_linear,
            |chunk| {
                kept += chunk.text.len();
                deferred_bytes += chunk.deferred_bytes;
                serial_bodies.extend(chunk.deferred.iter().copied());
                postponed_helpers.extend(chunk.postponed_helpers);
                // A helper reached by two chunks was emitted twice, spelling the same symbols
                // both times. Keep the first copy and drop the rest, or the assembler sees a
                // duplicate definition; every reference resolves to the survivor either way.
                match drop_duplicate_helpers(&chunk.text, &mut seen_helpers) {
                    Some(deduped) => emitter.append_block(&deduped),
                    None => emitter.append_block(&chunk.text),
                }
                emitter.spill_completed_text();
                data.merge_indexed(chunk.data, &mut data_index);
            },
        )?;
        for helper in postponed_helpers {
            shared.request_dynamic_prop_helper(
                &helper.key,
                &helper.label,
                helper.kind,
                helper.scope.as_deref(),
            );
        }
        let worker_pass = worker_pass_started.elapsed();
        if count_shared_touches {
            eprintln!(
                "[elephc-codegen-shared] worker_pass={:.2}s",
                worker_pass.as_secs_f64()
            );
            eprintln!(
                "[elephc-codegen-shared] parallel_bytes={} deferred_bytes={} deferred_share={:.1}%",
                kept,
                deferred_bytes,
                100.0 * deferred_bytes as f64 / (kept + deferred_bytes).max(1) as f64,
            );
        }
        parallel_ran = true;
        // The workers' shared bodies (invoker bodies, the named-argument normalizer) live in the
        // merged text, but this emitter never emitted them. Whatever it emits from here on (the
        // module's tail passes run on it after `main`, past the last de-duplication) must treat
        // them as defined, or the assembler sees the same symbol twice.
        emitter.mark_shared_bodies_emitted(seen_helpers.iter());
        shared.mark_merged_helper_scopes(seen_helpers.iter());
        // The appended bodies carried their own section directives; put the emitter back on a
        // known one before the serial pass continues after them.
        emitter.reopen_text_section(None);
    }
    let serial_started = Instant::now();
    let serial_count = serial_bodies.len();
    for index in serial_bodies {
        let body = bodies[index];
        let touches_before = shared.cache_touches();
        let body_started = emitter.checkpoint();
        let body_ool_started = emitter.out_of_line_len();
        let emitted = inventory.run_with_shared(
            emitter,
            &mut shared,
            &body.function.name,
            |emitter, shared| {
                emit_module_body(module, body, emitter, data, shared, regalloc_linear)
            },
        );
        record_or_fail(emitted, survey, &mut surveyed)?;
        if parallel_ran {
            // This body's shared state is empty of whatever the workers emitted, so it may have
            // produced a second copy of a helper they already hold.
            dedupe_emitted_since(emitter, body_started, body_ool_started, &mut seen_helpers);
        }
        // Between bodies no checkpoint is held, so completed text may leave memory here.
        emitter.spill_completed_text();
        if count_shared_touches {
            bodies_total += 1;
            if shared.cache_touches() != touches_before {
                bodies_touching_shared += 1;
            }
        }
    }
    if count_shared_touches {
        eprintln!(
            "[elephc-codegen-shared] serial_tail={:.2}s over {} bodies",
            serial_started.elapsed().as_secs_f64(),
            serial_count,
        );
    }
    trace_codegen_size(size_trace, "bodies-complete", emitter);
    if count_shared_touches {
        eprintln!(
            "[elephc-codegen-shared] bodies={} touching_shared_caches={} independent={}",
            bodies_total,
            bodies_touching_shared,
            bodies_total - bodies_touching_shared,
        );
    }
    if !surveyed.is_empty() {
        return Err(CodegenIrError::unsupported(format!(
            "{} function(s) did not lower:\n  {}",
            surveyed.len(),
            surveyed.join("\n  ")
        )));
    }
    inventory.finish()?;
    emit_eir_fiber_wrappers(module, emitter);
    // Enum case materializers are plain out-of-line functions that any user body,
    // method or closure may call, so they must exist in every emit kind — including
    // `Emit::Cdylib`, which returns before the main function is emitted.
    super::enum_singletons::emit_enum_case_materializers(emitter, module, data);
    // Every body drains its own typed-property guard requests; anything raised outside a body
    // (a wrapper, a materializer) is emitted here, and a copy a worker already holds is dropped.
    {
        let strays_started = emitter.checkpoint();
        let strays_ool_started = emitter.out_of_line_len();
        super::lower_inst::emit_pending_property_guard_helpers(
            module, emitter, data, &mut shared,
        )?;
        if parallel_ran {
            dedupe_emitted_since(emitter, strays_started, strays_ool_started, &mut seen_helpers);
        }
    }
    if emit.is_library() {
        return Ok(shared);
    }
    let main = module
        .functions
        .iter()
        .find(|function| is_main(module, function))
        .ok_or_else(|| CodegenIrError::invalid_module("EIR module has no main function"))?;
    let main_started = emitter.checkpoint();
    let main_ool_started = emitter.out_of_line_len();
    inventory.run_with_shared(emitter, &mut shared, &main.name, |emitter, shared| {
        emit_main_function(
            module,
            main,
            emitter,
            data,
            shared,
            gc_stats,
            heap_debug,
            requires_elephc_tls,
            regalloc_linear,
            web,
            web_isolation,
        )?;
        super::lower_inst::emit_pending_property_guard_helpers(module, emitter, data, shared)
    })?;
    if parallel_ran {
        // main is emitted against the compiling thread's caches, which hold none of what the
        // workers emitted, so it can repeat a helper a worker already produced.
        dedupe_emitted_since(emitter, main_started, main_ool_started, &mut seen_helpers);
        emitter.mark_shared_bodies_emitted(seen_helpers.iter());
        // The module tail (the callable-argument normalizer in `codegen::emit`) runs past this
        // last de-duplication: a keyed helper it reaches that a worker already emitted must be
        // referenced, not defined again.
        shared.mark_merged_helper_scopes(seen_helpers.iter());
    }
    inventory.finish()?;
    // Generate the per-request reset routine only for `--web`, and only after the
    // handler body is emitted so every function static local (including any in the
    // main body) has been recorded into `data`. The handler prologue's
    // `bl __rt_web_reset` forward-references the label emitted here.
    if super::frame::module_binds_extern_callbacks(module) {
        super::frame::emit_extern_callback_release_routine(emitter, data);
    }
    if web {
        super::web::emit_web_reset(emitter, module, data);
    }
    // The probe symbol table's terminating sentinel resolves to this label,
    // emitted after every function so it bounds the last real function's range.
    //
    // Out-of-line thunks (`Emitter::begin_out_of_line`) land AFTER it, so a sample in a descriptor
    // invoker or wrapper resolves to `<native>`, like any other compiler-generated glue. That is
    // deliberate: inline, it was charged to whichever function happened to mint the thunk first —
    // and invokers are cached by signature and shared, so a call from `g` was billed to `f`.
    // Moving this label past the thunks would be worse still: they would all be charged to the
    // last user function in the file.
    if probe {
        emitter.raw(&format!("{PROBE_TEXT_END_LABEL}:"));
    }
    Ok(shared)
}

/// Records every shared-helper key already present in `text`.
fn collect_helper_keys(text: &str, seen: &mut HashSet<String>) {
    // One `memmem` pass over the whole text, not a `str::find` per line: building a searcher
    // for each of millions of assembly lines showed up in the codegen profile.
    let open = crate::codegen::context::HELPER_MARKER_OPEN;
    let mut line_end = 0;
    for at in memchr::memmem::find_iter(text.as_bytes(), open.as_bytes()) {
        if at < line_end {
            continue; // a line counts its first marker only, as the per-line scan did
        }
        let rest = &text[at + open.len()..];
        let len = rest.find('\n').unwrap_or(rest.len());
        line_end = at + open.len() + len;
        seen.insert(rest[..len].trim().to_string());
    }
}

/// Drops, from everything `emitter` wrote since the two start markers, the shared-helper regions
/// whose key was already emitted.
///
/// Both the buffer and the out-of-line text are looked at: a helper that a body requested from
/// inside an out-of-line scope (a descriptor invoker, the named-argument normalizer) lives in the
/// latter, and a worker's copy of it would otherwise be defined a second time.
fn dedupe_emitted_since(
    emitter: &mut Emitter,
    buf_started: usize,
    out_of_line_started: usize,
    seen: &mut HashSet<String>,
) {
    if let Some(deduped) = drop_duplicate_helpers(emitter.text_from(buf_started), seen) {
        emitter.rollback_to(buf_started);
        emitter.append_block(&deduped);
    }
    if let Some(deduped) = drop_duplicate_helpers(emitter.out_of_line_from(out_of_line_started), seen)
    {
        emitter.replace_out_of_line_from(out_of_line_started, &deduped);
    }
}

/// Removes shared-helper regions whose key has already been emitted.
///
/// Workers emit against caches of their own, so the same helper can be produced more than once.
/// Its symbols come from its cache key, so the copies are interchangeable: the first one seen
/// stays and the others are dropped. Regions nest (a helper may reach another), and the decision
/// is taken at EVERY depth of a region being kept: a builtin-callable wrapper seen for the first
/// time still carries the invoker it reached, and when another body had already emitted that
/// invoker the kept copy defined `_eir_shared_callable_invoker_*` a second time. Inside a dropped
/// region nothing is looked at, so its nested keys stay unrecorded.
fn drop_duplicate_helpers(text: &str, seen: &mut HashSet<String>) -> Option<String> {
    use crate::codegen::context::{HELPER_MARKER_CLOSE, HELPER_MARKER_OPEN};
    // The markers are rare against the text they sit in, so they are found with `memmem` and
    // only their own lines are looked at; the rest is copied as whole runs. Scanning every line
    // for both markers was most of the cost of merging a parallel pass's output.
    let bytes = text.as_bytes();
    let opens = memchr::memmem::find_iter(bytes, HELPER_MARKER_OPEN.as_bytes())
        .map(|at| (at, true));
    let closes = memchr::memmem::find_iter(bytes, HELPER_MARKER_CLOSE.as_bytes())
        .map(|at| (at, false));
    let mut markers: Vec<(usize, bool)> = opens.chain(closes).collect();
    if markers.is_empty() {
        return None;
    }
    markers.sort_unstable();
    let line_start = |at: usize| memchr::memrchr(b'\n', &bytes[..at]).map_or(0, |nl| nl + 1);
    let line_end = |at: usize| {
        memchr::memchr(b'\n', &bytes[at..]).map_or(bytes.len(), |nl| at + nl + 1)
    };
    let mut out: Option<String> = None;
    let mut copied_until = 0usize;
    let mut depth = 0usize;
    // The depth at which the region being dropped opened, while one is.
    let mut dropping_at: Option<usize> = None;
    // A shared body found inside a dropped region: where it starts, and the depth it opened at.
    let mut rescuing: Option<(usize, usize)> = None;
    for (at, open) in markers {
        if open {
            let end = line_end(at);
            let key = text[at + HELPER_MARKER_OPEN.len()..end].trim();
            if dropping_at.is_none() {
                if !seen.insert(key.to_string()) {
                    dropping_at = Some(depth);
                    let start = line_start(at);
                    out.get_or_insert_with(|| String::with_capacity(text.len()))
                        .push_str(&text[copied_until..start]);
                }
            } else if rescuing.is_none() && key.starts_with("_eir_") && seen.insert(key.to_string()) {
                // A content-shared body stands alone. The stub it sat behind is a duplicate, but
                // the emitter already counts this body as written, so every later stub with the
                // same content jumps to it and no other copy will ever exist.
                rescuing = Some((line_start(at), depth));
            }
            depth += 1;
        } else {
            depth = depth.saturating_sub(1);
            if let Some((start, opened_at)) = rescuing.filter(|(_, opened_at)| *opened_at == depth) {
                debug_assert_eq!(opened_at, depth);
                out.get_or_insert_with(String::new).push_str(&text[start..line_end(at)]);
                rescuing = None;
            }
            if dropping_at == Some(depth) {
                dropping_at = None;
                copied_until = line_end(at);
            }
        }
    }
    let mut out = out?;
    if dropping_at.is_none() {
        out.push_str(&text[copied_until..]);
    }
    Some(out)
}

/// How many threads may emit bodies at once: one per core, up to `MAX_CODEGEN_JOBS`.
///
/// This was one for a long time, for a measured reason: a body that reached a shared codegen
/// cache had to be emitted serially, and those were the expensive bodies, so a parallel pass
/// spent eight threads on the cheap ones and made the build slower. What changed is that no
/// family of those caches forces a body serial any more when the helpers emitted ahead of the
/// bodies already filled it (`ModulePreset`), the eval registration helper is one of those, and
/// the keyed helpers merge; on the Symfony `--web` module 111 of 9 005 bodies still defer.
/// Measured on that module, codegen 13.3 s serial, 9.4 s at 8 jobs, 8.8 s at 12; the peak
/// footprint goes from 1.75 GB to 2.2 GB, which is what the ceiling holds down.
///
/// `ELEPHC_CODEGEN_JOBS` overrides it; `ELEPHC_CODEGEN_JOBS=1` is the serial pass.
fn codegen_jobs() -> usize {
    if let Ok(value) = std::env::var("ELEPHC_CODEGEN_JOBS") {
        return value.trim().parse::<usize>().unwrap_or(1).clamp(1, 64);
    }
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(1, MAX_CODEGEN_JOBS)
}

/// Ceiling on `codegen_jobs`: past it the phase stops getting faster and only uses more memory.
const MAX_CODEGEN_JOBS: usize = 12;

/// Fewest bodies a module needs before its bodies are emitted in parallel.
///
/// Below it the pass costs more than it saves -- threads to start, a fresh cache state per chunk,
/// helpers emitted once per chunk and merged -- and a small program keeps the output the serial
/// pass gives it. `ELEPHC_CODEGEN_PARALLEL_MIN` overrides it, which is how a test run sends every
/// small fixture through the parallel pass.
fn parallel_min_bodies() -> usize {
    std::env::var("ELEPHC_CODEGEN_PARALLEL_MIN")
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(256)
}

/// Which lowering one emittable body takes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BodyKind {
    /// A plain function or a closure: both go through `emit_user_function`.
    UserFunction,
    /// A class method, which carries the legacy runtime metadata symbol shape.
    ClassMethod,
}

/// One body the module owns, paired with the lowering it takes.
#[derive(Clone, Copy)]
struct ModuleBody<'a> {
    kind: BodyKind,
    function: &'a Function,
    /// The survivor whose machine code this body shares, when the fold map names it.
    survivor: Option<&'a str>,
}

/// Lists every body `emit_module` lowers, in the order a serial build emits them.
fn module_bodies<'a>(
    module: &'a Module,
    aliases: &'a crate::fast_hash::FastMap<String, String>,
) -> Vec<ModuleBody<'a>> {
    let body = |kind, function: &'a Function| ModuleBody {
        kind,
        function,
        survivor: aliases.get(&function.name).map(String::as_str),
    };
    module
        .functions
        .iter()
        .filter(|function| !is_main(module, function))
        .map(|function| body(BodyKind::UserFunction, function))
        .chain(
            module
                .class_methods
                .iter()
                .map(|function| body(BodyKind::ClassMethod, function)),
        )
        .chain(
            module
                .closures
                .iter()
                .map(|function| body(BodyKind::UserFunction, function)),
        )
        .collect()
}

/// Emits one body through the lowering its kind names.
fn emit_module_body(
    module: &Module,
    body: ModuleBody<'_>,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    // A folded candidate keeps its EIR function but emits a jump to the survivor's body.
    if let Some(survivor) = body.survivor {
        return emit_folded_body(body.function, survivor, emitter);
    }
    match body.kind {
        BodyKind::UserFunction => {
            emit_user_function(module, body.function, emitter, data, shared, regalloc_linear)?
        }
        BodyKind::ClassMethod => {
            emit_class_method(module, body.function, emitter, data, shared, regalloc_linear)?
        }
    }
    // A typed-property guard this body asked for is emitted right behind it, never inside it.
    super::lower_inst::emit_pending_property_guard_helpers(module, emitter, data, shared)
}

/// How many chunks the parallel pass cuts the bodies into, whatever the number of threads.
///
/// Fixed so that the output does not depend on the machine: each chunk starts from the same
/// state and emits the same text whichever thread runs it, and chunks are merged in order, so a
/// build is byte-identical at any job count. Many more chunks than threads keeps a thread from
/// being handed one slow stretch of the module, and keeps each chunk's text small while it waits
/// to be merged: 256 chunks peaked 90 MB below 64 on the Symfony module, at the same speed.
const CODEGEN_CHUNKS: usize = 256;

/// What one chunk produced: the text of the bodies it could emit alone, and the indexes of the
/// ones that reached a shared cache and must be re-emitted serially.
struct ChunkOutput {
    /// Shared helpers its bodies call and the compiling thread has to emit.
    postponed_helpers: Vec<super::shared_state::PendingDynamicPropHelper>,
    /// The kept bodies' assembly, in module order.
    text: String,
    /// Bytes emitted for bodies that were then deferred; see `ELEPHC_CODEGEN_SHARED_TOUCHES`.
    deferred_bytes: usize,
    /// Body indexes that reached a shared cache, in module order.
    deferred: Vec<usize>,
    /// The chunk's data-section shard.
    data: DataSection,
}

/// Emits every body it can on `jobs` threads and hands each chunk's output to `merge`, in chunk
/// order, on the calling thread.
///
/// Each chunk owns its emitter, its data shard and its own `SharedCodegenState` started from
/// `preset`, so nothing is shared mutably and the borrow checker -- not a convention -- is what
/// guarantees it. A body whose emission reaches a cache outside the preset is rolled back and
/// deferred, because the helper behind that cache belongs to the module exactly once.
fn emit_bodies_in_parallel<'a>(
    module: &'a Module,
    bodies: &[ModuleBody<'a>],
    template: &Emitter,
    preset: &std::sync::Arc<super::shared_state::ModulePreset>,
    data_base: &std::sync::Arc<crate::codegen::data_section::DataBase>,
    jobs: usize,
    regalloc_linear: bool,
    mut merge: impl FnMut(ChunkOutput),
) -> Result<()> {
    let chunk_len = bodies.len().div_ceil(CODEGEN_CHUNKS).max(1);
    let chunks: Vec<&[ModuleBody<'a>]> = bodies.chunks(chunk_len).collect();
    let next_chunk = std::sync::atomic::AtomicUsize::new(0);
    // The compile's thread-local facts, which a new thread would otherwise read as defaults.
    let context = crate::compile_thread_context::CompileThreadContext::capture();
    let (sender, receiver) = std::sync::mpsc::channel::<(usize, Result<ChunkOutput>)>();
    std::thread::scope(|scope| {
        for _ in 0..jobs.min(chunks.len()) {
            let sender = sender.clone();
            let (chunks, next_chunk, context) = (&chunks, &next_chunk, &context);
            scope.spawn(move || {
                context.install();
                loop {
                let index = next_chunk.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(chunk) = chunks.get(index) else {
                    break;
                };
                let output = emit_chunk(
                    module,
                    chunk,
                    index * chunk_len,
                    index,
                    template,
                    preset,
                    data_base,
                    regalloc_linear,
                );
                if sender.send((index, output)).is_err() {
                    break;
                }
                }
            });
        }
        drop(sender);
        let mut pending: std::collections::BTreeMap<usize, Result<ChunkOutput>> =
            std::collections::BTreeMap::new();
        let mut next_merge = 0usize;
        let mut failure = None;
        for (index, output) in receiver {
            pending.insert(index, output);
            while let Some(output) = pending.remove(&next_merge) {
                next_merge += 1;
                match output {
                    Ok(output) if failure.is_none() => merge(output),
                    Ok(_) => {}
                    Err(error) => {
                        failure.get_or_insert(error);
                    }
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None if next_merge < chunks.len() => {
                Err(CodegenIrError::invalid_module("a codegen worker panicked"))
            }
            None => Ok(()),
        }
    })
}

/// Emits one chunk of bodies from a fresh state; see `emit_bodies_in_parallel`.
fn emit_chunk<'a>(
    module: &'a Module,
    chunk: &[ModuleBody<'a>],
    base: usize,
    shard: usize,
    template: &Emitter,
    preset: &std::sync::Arc<super::shared_state::ModulePreset>,
    data_base: &std::sync::Arc<crate::codegen::data_section::DataBase>,
    regalloc_linear: bool,
) -> Result<ChunkOutput> {
    let mut emitter = template.fresh();
    let mut data = DataSection::for_shard(shard, Some(std::sync::Arc::clone(data_base)));
    let mut shared = SharedCodegenState::for_module(module);
    shared.apply_module_preset(preset);
    let mut deferred = Vec::new();
    let mut deferred_bytes = 0usize;
    // While refusals are being collected, a failing body is left to the serial pass, which
    // records it in the inventory; failing the whole pass here would lose every other refusal.
    let collect_refusals = BackendInventory::from_env().is_enabled();
    for (offset, body) in chunk.iter().enumerate() {
        let start = emitter.checkpoint();
        // Data is rolled back with the text: a callable descriptor is a data entry that points
        // at a text label this body defines, so keeping one without the other hands the
        // assembler a reference with no definition.
        let data_start = data.checkpoint();
        let out_of_line_start = emitter.out_of_line_len();
        let cache_mark = shared.out_of_line_cache_mark();
        // The CACHE is deliberately not rolled back with them. Undoing a deferred body's entries
        // makes every following body re-emit the helpers it needs, and the watch below does not
        // bound that: one instruction's lowering can emit thousands of descriptors, so the abort
        // cannot land inside it. Measured on the Symfony module, adding the rollback took the
        // discarded text from 2 101 439 143 bytes to 29 397 554 561 and the worker pass from 40 s
        // to 121 s.
        shared.arm_deferral_watch();
        let outcome =
            emit_module_body(module, *body, &mut emitter, &mut data, &mut shared, regalloc_linear);
        shared.disarm_deferral_watch();
        match outcome {
            Ok(()) => {}
            Err(error) if error.is_deferred_body() || collect_refusals => {
                deferred_bytes += emitter.text_from(start).len();
                deferred.push(base + offset);
                data.rollback_to(data_start);
                // The body's out-of-line functions (invokers, wrappers) leave with it, and so do
                // the cache entries naming them: the serial pass emits the body again with the
                // same global symbols, and a surviving copy defined them a second time.
                emitter.rollback_out_of_line_to(out_of_line_start);
                shared.forget_out_of_line_caches_since(cache_mark);
                emitter.rollback_to(start);
                shared.forget_worker_templates();
            }
            Err(error) => return Err(error),
        }
    }
    Ok(ChunkOutput {
        postponed_helpers: shared.take_postponed_helpers(),
        text: emitter.text_with_out_of_line(),
        deferred_bytes,
        deferred,
        data,
    })
}

/// Emits one opt-in checkpoint between code-generation body families.
fn trace_codegen_size(enabled: bool, stage: &str, emitter: &Emitter) {
    if enabled {
        eprintln!(
            "[elephc-codegen-size] stage={stage} emitter_bytes={}",
            emitter.checkpoint(),
        );
    }
}

/// Environment variable enabling the scan-every-body backend survey.
const BACKEND_INVENTORY_VAR: &str = "ELEPHC_BACKEND_INVENTORY";

/// Collects one backend refusal per body while discarding partial assembly.
struct BackendInventory {
    enabled: bool,
    scanned: usize,
    failures: Vec<InventoryFailure>,
}

/// Records the body, groupable cause, and fully rendered backend refusal.
struct InventoryFailure {
    body: String,
    cause: String,
    detail: String,
}

impl BackendInventory {
    /// Reads the inventory switch from the process environment.
    fn from_env() -> Self {
        Self::new(
            matches!(std::env::var(BACKEND_INVENTORY_VAR).as_deref(), Ok("1"))
                || super::collects_backend_refusals(),
        )
    }

    /// Returns true when every body is scanned for its first refusal instead of stopping.
    ///
    /// The scan records one failure per body and rolls that body's assembly back, which depends
    /// on emitting them one after another, so it keeps the single pass.
    fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Builds an inventory with an explicit switch for deterministic tests.
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            scanned: 0,
            failures: Vec::new(),
        }
    }

    /// Emits one body or records its first refusal and rolls its output back.
    #[cfg(test)]
    fn run(
        &mut self,
        emitter: &mut Emitter,
        name: &str,
        emit: impl FnOnce(&mut Emitter) -> Result<()>,
    ) -> Result<()> {
        if !self.enabled {
            return emit(emitter);
        }
        self.scanned += 1;
        let checkpoint = emitter.checkpoint();
        match emit(emitter) {
            Ok(()) => Ok(()),
            Err(error) => {
                emitter.rollback_to(checkpoint);
                self.failures.push(InventoryFailure {
                    body: error.owner_body().unwrap_or(name).to_string(),
                    cause: cause_key(error.message()),
                    detail: error.to_string(),
                });
                Ok(())
            }
        }
    }

    /// Emits one body while rolling back Reflection label reservations with failed assembly.
    fn run_with_shared(
        &mut self,
        emitter: &mut Emitter,
        shared: &mut SharedCodegenState,
        name: &str,
        emit: impl FnOnce(&mut Emitter, &mut SharedCodegenState) -> Result<()>,
    ) -> Result<()> {
        if !self.enabled {
            return emit(emitter, shared);
        }
        self.scanned += 1;
        let emitter_checkpoint = emitter.checkpoint();
        let reflection_checkpoint = shared.reflection.materializer_checkpoint();
        match emit(emitter, shared) {
            Ok(()) => Ok(()),
            Err(error) => {
                emitter.rollback_to(emitter_checkpoint);
                shared
                    .reflection
                    .rollback_materializers(reflection_checkpoint);
                self.failures.push(InventoryFailure {
                    body: error.owner_body().unwrap_or(name).to_string(),
                    cause: cause_key(error.message()),
                    detail: error.to_string(),
                });
                Ok(())
            }
        }
    }

    /// Returns an aggregate error when at least one scanned body refused.
    fn finish(&self) -> Result<()> {
        if !self.enabled || self.failures.is_empty() {
            return Ok(());
        }
        let mut error = CodegenIrError::invalid_module(self.report());
        error.failed_bodies = self.failures.iter().map(|failure| (failure.body.clone(), failure.detail.clone())).collect();
        Err(error)
    }

    /// Renders a cause histogram followed by per-body diagnostic details.
    fn report(&self) -> String {
        let mut report = format!(
            "backend inventory: {} of {} scanned bodies could not be lowered\n\
             (first refusal per body only, so this is a floor; no artifact was produced)\n\n\
             by cause:\n",
            self.failures.len(), self.scanned
        );
        for (cause, count) in self.causes() {
            let _ = writeln!(report, "  {:>4}  {}", count, cause);
        }
        report.push_str("\nby body:\n");
        for failure in &self.failures {
            let _ = writeln!(report, "  {}\n        {}", failure.body, failure.detail);
        }
        report
    }

    /// Groups identical causes and sorts them by descending frequency.
    fn causes(&self) -> Vec<(&str, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::default();
        for failure in &self.failures {
            *counts.entry(failure.cause.as_str()).or_insert(0) += 1;
        }
        let mut causes: Vec<(&str, usize)> = counts.into_iter().collect();
        causes.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
        causes
    }
}

/// Removes a trailing body attribution so equivalent causes share one bucket.
fn cause_key(message: &str) -> String {
    match message.rfind(" in '") {
        Some(index) if message.ends_with('\'') => message[..index].to_string(),
        _ => message.to_string(),
    }
}

/// Emits the static EIR Fiber wrappers needed for closure callbacks.
fn emit_eir_fiber_wrappers(module: &Module, emitter: &mut Emitter) {
    for wrapper in required_eir_fiber_wrappers(module) {
        let wrapper = DeferredFiberWrapper {
            label: wrapper.label,
            sig: wrapper.sig,
            visible_param_count: wrapper.visible_param_count,
            hidden_arg_types: wrapper.hidden_arg_types,
            retain_hidden_args_for_closure_call: false,
            use_descriptor_invoker: wrapper.use_descriptor_invoker,
        };
        emit_fiber_wrapper(emitter, &wrapper);
    }
}

/// Collects unique Fiber wrappers needed by this module.
fn required_eir_fiber_wrappers(module: &Module) -> Vec<fibers::FiberWrapper> {
    let mut wrappers = Vec::new();
    for function in all_module_functions(module) {
        for inst in &function.instructions {
            let Some(wrapper) = fibers::wrapper_for_fiber_new(module, function, inst) else {
                continue;
            };
            if wrappers
                .iter()
                .any(|existing: &fibers::FiberWrapper| existing.label == wrapper.label)
            {
                continue;
            }
            wrappers.push(wrapper);
        }
    }
    wrappers
}

/// Iterates every function-like body owned by the EIR module.
fn all_module_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Emits a non-main EIR function as a direct-call target.
fn emit_user_function(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    let entry_label = user_function_entry_symbol(function);
    let synthetic = !function.has_source_identity() || is_property_init_thunk(function);
    emit_fn_marker(emitter, function.display_name(), &entry_label, synthetic || function.flags.is_generator);
    if function.flags.is_generator {
        emit_generator_function(
            module,
            function,
            &entry_label,
            emitter,
            data,
            shared,
            regalloc_linear,
        )?;
        return Ok(());
    }
    let layout = frame::layout_for_function(
        function,
        emitter.target,
        regalloc_linear,
        true,
        frame::module_uses_backtrace(module) && function.has_source_identity(),
    );
    let epilogue_label = user_function_epilogue_symbol(function);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        shared,
        layout,
        false,
        false,
        false,
        Some(epilogue_label),
    );
    frame::emit_function_prologue_with_label(&mut ctx, &entry_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    emit_endfn_marker(ctx.emitter, function.display_name());
    frame::emit_exception_cleanup_callback(&mut ctx, &entry_label);
    Ok(())
}

/// Emits a synthetic EIR function body at an explicit already-published entry label.
pub(super) fn emit_synthetic_function_with_label(
    module: &Module,
    function: &Function,
    entry_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    let layout = frame::layout_for_function(
        function,
        emitter.target,
        regalloc_linear,
        true,
        false,
    );
    let epilogue_label = format!("{}_epilogue", entry_label);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        shared,
        layout,
        false,
        false,
        false,
        Some(epilogue_label),
    );
    frame::emit_function_prologue_with_label(&mut ctx, entry_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    frame::emit_exception_cleanup_callback(&mut ctx, entry_label);
    Ok(())
}

/// Text-section label marking the end of user code, used as the probe symbol
/// table's terminating sentinel so a PC in a runtime helper or libc — past the
/// last real function — resolves to `<native>` instead of the last function.
const PROBE_TEXT_END_LABEL: &str = "_elephc_probe_text_end";

/// Builds the `--probe` symbol table in the data section: one
/// `[entry address, name pointer, name length]` triple per PHP source entry,
/// including private variants under their original source identity. Main is named `{main}`.
/// The probe sorts entries by address at dump time; a final sentinel triple uses
/// `PROBE_TEXT_END_LABEL`. Returns the
/// table's data label and its entry count for `elephc_probe_init`.
fn emit_probe_symbol_table(
    module: &Module,
    data: &mut DataSection,
    main_symbol: &str,
) -> (String, usize) {
    let mut words = Vec::new();
    let mut count = 0usize;
    let push = |data: &mut DataSection, words: &mut Vec<DataWord>, entry: String, name: &str| {
        let (name_label, name_len) = data.add_string(name.as_bytes());
        words.push(DataWord::Symbol(entry));
        words.push(DataWord::Symbol(name_label));
        words.push(DataWord::U64(name_len as u64));
    };
    for function in &module.functions {
        if !function.has_source_identity() {
            continue;
        }
        if is_main(module, function) {
            push(data, &mut words, main_symbol.to_string(), "{main}");
        } else {
            push(data, &mut words, user_function_entry_symbol(function), function.display_name());
        }
        count += 1;
    }
    for method in &module.class_methods {
        if method.flags.is_synthetic {
            continue;
        }
        if let Ok(entry) = class_method_entry_symbol(method) {
            push(data, &mut words, entry, &method.name);
            count += 1;
        }
    }
    for closure in &module.closures {
        if !closure.has_source_identity() {
            continue;
        }
        push(data, &mut words, user_function_entry_symbol(closure), closure.display_name());
        count += 1;
    }
    // Terminating sentinel: bounds the last real function's address range.
    push(data, &mut words, PROBE_TEXT_END_LABEL.to_string(), "<end>");
    count += 1;
    let label = data.add_words(words);
    (label, count)
}

/// Returns the assembly entry label for a user or synthetic EIR function.
fn user_function_entry_symbol(function: &Function) -> String {
    if is_property_init_thunk(function) {
        return function.name.clone();
    }
    function_symbol(&function.name)
}

/// Returns the epilogue label paired with `user_function_entry_symbol()`.
fn user_function_epilogue_symbol(function: &Function) -> String {
    if is_property_init_thunk(function) {
        return format!("{}_epilogue", function.name);
    }
    function_epilogue_symbol(&function.name)
}

/// Returns true for synthetic property-default init thunks referenced by runtime metadata.
fn is_property_init_thunk(function: &Function) -> bool {
    function.name.starts_with("_class_propinit_")
}

/// Emits a class method and gives runtime-called destructors exceptional local cleanup.
fn emit_class_method(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    let entry_label = class_method_entry_symbol(function)?;
    emit_fn_marker(emitter, function.display_name(), &entry_label, !function.has_source_identity() || function.flags.is_generator);
    if function.flags.is_generator {
        emit_generator_function(
            module,
            function,
            &entry_label,
            emitter,
            data,
            shared,
            regalloc_linear,
        )?;
        return Ok(());
    }
    let layout = frame::layout_for_function(
        function,
        emitter.target,
        regalloc_linear,
        true,
        frame::module_uses_backtrace(module) && function.has_source_identity(),
    );
    let epilogue_label = format!("{}_epilogue", entry_label);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        shared,
        layout,
        false,
        false,
        false,
        Some(epilogue_label),
    );
    frame::emit_function_prologue_with_label(&mut ctx, &entry_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    emit_endfn_marker(ctx.emitter, function.display_name());
    frame::emit_exception_cleanup_callback(&mut ctx, &entry_label);
    Ok(())
}

/// Emits a generator as a stackful coroutine: a constructor at the public entry
/// label, the Mixed-returning body via the normal backend, and an arg-unboxing
/// callback that the fiber entry trampoline runs on the coroutine stack.
///
/// The constructor (`entry_label`) allocates a Fiber-shaped Generator object via
/// `__rt_fiber_construct`, stamping the Generator class id and the callback as
/// the coroutine entry wrapper, then returns the object. The body
/// (`<entry_label>__genbody`) is the generator's EIR function, which returns the
/// value passed to `return` (read back by `getReturn`). The callback
/// (`<entry_label>__gencb`) runs the body and parks its return value in the
/// generator's `gen_return_value` slot.
fn emit_generator_function(
    module: &Module,
    function: &Function,
    entry_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    let body_label = format!("{}__genbody", entry_label);
    let callback_label = format!("{}__gencb", entry_label);
    let param_types = generator_param_types(function);
    // Parameters (and closure captures, which arrive as call arguments) are boxed into the
    // coroutine's fixed `start_args` slots; there are only `GENERATOR_START_ARGS_MAX` of them.
    // Reject overflow with a clear diagnostic instead of silently writing past the slots into
    // adjacent fiber fields.
    let max_params = crate::codegen::runtime::GENERATOR_START_ARGS_MAX as usize;
    if param_types.len() > max_params {
        return Err(CodegenIrError::unsupported(format!(
            "generator '{}' has {} parameters including closure captures; \
             generators support at most {} because parameters are stored in the \
             coroutine's start-argument slots",
            function.name,
            param_types.len(),
            max_params
        )));
    }
    emit_generator_constructor(emitter, entry_label, &callback_label, &param_types);
    emit_endfn_marker(emitter, &function.name);
    emit_generator_body(
        module,
        function,
        &body_label,
        emitter,
        data,
        shared,
        regalloc_linear,
    )?;
    emit_generator_callback(emitter, &callback_label, &body_label, &param_types);
    Ok(())
}

/// Returns the codegen representations of every value a generator's caller
/// forwards into the coroutine in argument registers: the visible parameters
/// plus any closure-capture parameters (closure captures are lowered as direct
/// call arguments — `call $capture data[symbol]` — so they arrive in registers
/// exactly like ordinary parameters). The constructor boxes each into the
/// generator's `start_args` slots and the callback unboxes them back into the
/// body's parameter registers.
fn generator_param_types(function: &Function) -> Vec<PhpType> {
    function
        .params
        .iter()
        .map(|param| param.php_type.codegen_repr())
        .collect()
}

/// ABI placement category for a generator parameter as it crosses the
/// box/unbox boundary between the constructor and the coroutine body.
#[derive(Clone, Copy, PartialEq)]
enum GenParamKind {
    /// One integer/pointer register (int, bool, object, array, callable, ...).
    IntLike,
    /// One floating-point register.
    Float,
    /// Two integer registers: string pointer followed by length.
    Str,
    /// Two integer registers: tagged-scalar payload followed by its runtime tag.
    TaggedScalar,
    /// An already-boxed Mixed cell pointer forwarded as-is.
    Mixed,
}

/// Classifies a parameter's codegen representation into a `GenParamKind`.
fn gen_param_kind(ty: &PhpType) -> GenParamKind {
    match ty {
        PhpType::Float => GenParamKind::Float,
        PhpType::Str => GenParamKind::Str,
        PhpType::TaggedScalar => GenParamKind::TaggedScalar,
        PhpType::Mixed | PhpType::Union(_) => GenParamKind::Mixed,
        _ => GenParamKind::IntLike,
    }
}

/// Frame-pointer-relative byte offset of the slot caching the saved
/// generator-object register. Frame slots use negative offsets from the frame
/// pointer (`[x29,#-off]` / `[rbp-off]`); offset 0 aliases the saved frame
/// pointer on x86_64, so the first usable slot starts at 16.
const GEN_SAVE_OFF: usize = 16;

/// Returns the frame-pointer-relative byte offset of the scratch slot for
/// generator parameter `idx`. Each parameter reserves a contiguous 16-byte cell
/// `[slot - 8, slot + 8)`: the primary value (pointer/scalar/float) sits at `slot`
/// and the string length / tagged tag at `slot - 8`, matching the secondary-word
/// convention of `emit_store_incoming_param` and the outgoing call helpers.
fn gen_param_slot(idx: usize) -> usize {
    32 + idx * 16
}

/// Computes the 16-byte-aligned frame size for the generator constructor/callback
/// scratch frame holding `n` parameter slots (plus the saved generator-object
/// slot and the 16-byte frame footer).
fn gen_arg_frame_size(n: usize) -> usize {
    let bytes = n * 16 + 48;
    (bytes + 15) & !15
}

/// Emits the generator constructor at `entry_label`.
///
/// Allocates the Generator coroutine object (reusing `__rt_fiber_construct` with
/// the Generator class id and `callback_label` as the coroutine entry wrapper),
/// boxes each caller-visible parameter into the generator's `start_args` slots
/// (so the values survive until the body lazily starts, owned by the generator),
/// then returns the object.
fn emit_generator_constructor(
    emitter: &mut Emitter,
    entry_label: &str,
    callback_label: &str,
    param_types: &[PhpType],
) {
    use crate::codegen::runtime::{generator_start_arg_offset, FIBER_START_ARG_COUNT_OFFSET};
    let target = emitter.target;
    let n = param_types.len();
    let frame_size = gen_arg_frame_size(n);
    let gen_reg = abi::int_result_reg(emitter); // x0 / rax — also the construct result
    let gen_cache = match target.arch {
        Arch::AArch64 => "x19",
        Arch::X86_64 => "r12",
    };

    emitter.blank();
    emitter.comment(&format!("--- generator constructor {} ---", entry_label));
    emitter.label_global(entry_label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(emitter, gen_cache, GEN_SAVE_OFF); // preserve the caller's callee-saved generator-object cache register

    // -- spill the incoming parameters into frame slots before __rt_fiber_construct clobbers them --
    // `emit_store_incoming_param` consumes the argument registers in ABI order and falls back to the
    // caller stack once the integer/float register budget is exhausted (e.g. a 7th integer parameter
    // on x86_64 SysV), so stack-passed parameters survive instead of being dropped. The register reads
    // must happen before the construct call; caller-stack reads are frame-pointer-relative and remain
    // valid across it.
    let mut cursor = abi::IncomingArgCursor::for_target(target, 0);
    for (idx, ty) in param_types.iter().enumerate() {
        abi::emit_store_incoming_param(
            emitter,
            &format!("arg{idx}"),
            ty,
            gen_param_slot(idx),
            false,
            &mut cursor,
        );
    }

    // -- allocate the Generator coroutine object --
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // no callable descriptor — the wrapper runs the compiled body
            abi::emit_load_symbol_to_reg(emitter, "x1", "_generator_class_id", 0); // x1 = runtime class id of Generator
            abi::emit_symbol_address(emitter, "x2", callback_label); // x2 = generator coroutine entry wrapper
            emitter.instruction("bl __rt_fiber_construct");                     // x0 = freshly allocated Generator coroutine object
            emitter.instruction("mov x19, x0");                                 // cache the generator object across the per-parameter boxing calls
        }
        Arch::X86_64 => {
            emitter.instruction("xor edi, edi");                                // no callable descriptor — the wrapper runs the compiled body
            abi::emit_load_symbol_to_reg(emitter, "rsi", "_generator_class_id", 0); // rsi = runtime class id of Generator
            abi::emit_symbol_address(emitter, "rdx", callback_label); // rdx = generator coroutine entry wrapper
            emitter.instruction("call __rt_fiber_construct");                   // rax = freshly allocated Generator coroutine object
            emitter.instruction("mov r12, rax");                                // cache the generator object across the per-parameter boxing calls
        }
    }

    // -- box each spilled parameter into an owned Mixed cell stored in start_args --
    for (idx, ty) in param_types.iter().enumerate() {
        let slot = gen_param_slot(idx);
        let store_off = generator_start_arg_offset(idx);
        match gen_param_kind(ty) {
            GenParamKind::Mixed => {
                abi::load_at_offset(emitter, gen_reg, slot);
                match target.arch {
                    Arch::AArch64 => emitter.instruction("bl __rt_incref"),     // own a reference to the forwarded Mixed cell
                    Arch::X86_64 => emitter.instruction("call __rt_incref"),    // own a reference to the forwarded Mixed cell
                }
            }
            GenParamKind::Float => {
                let freg = match target.arch {
                    Arch::AArch64 => "d0",
                    Arch::X86_64 => "xmm0",
                };
                abi::load_at_offset(emitter, freg, slot);
                crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Float);
            }
            GenParamKind::Str => match target.arch {
                Arch::AArch64 => {
                    abi::load_at_offset(emitter, "x1", slot);
                    abi::load_at_offset(emitter, "x2", slot - 8);
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Str);
                }
                Arch::X86_64 => {
                    abi::load_at_offset(emitter, "rax", slot);
                    abi::load_at_offset(emitter, "rdx", slot - 8);
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Str);
                }
            },
            GenParamKind::TaggedScalar => match target.arch {
                Arch::AArch64 => {
                    abi::load_at_offset(emitter, "x0", slot);
                    abi::load_at_offset(emitter, "x1", slot - 8);
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::TaggedScalar);
                }
                Arch::X86_64 => {
                    abi::load_at_offset(emitter, "rax", slot);
                    abi::load_at_offset(emitter, "rdx", slot - 8);
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::TaggedScalar);
                }
            },
            GenParamKind::IntLike => {
                abi::load_at_offset(emitter, gen_reg, slot);
                crate::codegen::emit_box_current_value_as_mixed(emitter, ty);
            }
        }
        match target.arch {
            Arch::AArch64 => {
                emitter.instruction(                                            // store the owned Mixed cell into the generator start_args slot
                    &format!("str {}, [x19, #{}]", gen_reg, store_off)
                )
            }
            Arch::X86_64 => {
                emitter.instruction(                                            // store the owned Mixed cell into the generator start_args slot
                    &format!("mov QWORD PTR [r12 + {}], {}", store_off, gen_reg)
                )
            }
        }
    }

    // -- publish the forwarded argument count and return the Generator object --
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x9, #{}", n));                    // number of boxed start arguments forwarded to the body
            emitter.instruction(                                                // publish the start argument count
                &format!("str x9, [x19, #{}]", FIBER_START_ARG_COUNT_OFFSET)
            );
            emitter.instruction("mov x0, x19");                                 // return the Generator object to the caller
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(                                       // publish the start argument count
                "mov QWORD PTR [r12 + {}], {}",
                FIBER_START_ARG_COUNT_OFFSET, n
            ));
            emitter.instruction("mov rax, r12");                                // return the Generator object to the caller
        }
    }
    abi::load_at_offset(emitter, gen_cache, GEN_SAVE_OFF); // restore the caller's callee-saved generator-object cache register
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Emits the generator body as a normal EIR function at `body_label`.
///
/// The body is the generator's EIR function, lowered with a Mixed return type so
/// `return $x` produces the value read back by `getReturn`. It runs on the
/// coroutine stack and reaches `__rt_gen_suspend` at each `yield`.
fn emit_generator_body(
    module: &Module,
    function: &Function,
    body_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    // PHP source locations belong to the body, not to the synthetic coroutine constructor.
    emit_fn_marker(emitter, function.display_name(), body_label, !function.has_source_identity());
    let layout = frame::layout_for_function(
        function,
        emitter.target,
        regalloc_linear,
        true,
        frame::module_uses_backtrace(module) && function.has_source_identity(),
    );
    let epilogue_label = format!("{}_epilogue", body_label);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        shared,
        layout,
        false,
        false,
        false,
        Some(epilogue_label),
    );
    frame::emit_function_prologue_with_label(&mut ctx, body_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    emit_endfn_marker(ctx.emitter, function.display_name());
    frame::emit_exception_cleanup_callback(&mut ctx, body_label);
    Ok(())
}

/// Emits the generator coroutine entry wrapper at `callback_label`.
///
/// The fiber entry trampoline calls this wrapper on the coroutine stack with the
/// Generator object in the first argument register. The wrapper unboxes the
/// generator's `start_args` cells back into the body's parameter registers, runs
/// the body, stores the body's boxed return value into the generator's
/// `gen_return_value` slot, then returns null so the fiber transfer value does
/// not alias the parked return value.
fn emit_generator_callback(
    emitter: &mut Emitter,
    callback_label: &str,
    body_label: &str,
    param_types: &[PhpType],
) {
    use crate::codegen::runtime::generators::coro::GEN_RETURN_VALUE_OFFSET;
    use crate::codegen::runtime::generator_start_arg_offset;
    let target = emitter.target;
    let n = param_types.len();
    let frame_size = gen_arg_frame_size(n);
    let gen_cache = match target.arch {
        Arch::AArch64 => "x19",
        Arch::X86_64 => "r12",
    };
    let result_reg = abi::int_result_reg(emitter); // x0 / rax
    let assignments = abi::build_outgoing_arg_assignments_for_target(target, param_types, 0);

    emitter.blank();
    emitter.comment(&format!(
        "--- generator entry wrapper {} ---",
        callback_label
    ));
    emitter.label_global(callback_label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(emitter, gen_cache, GEN_SAVE_OFF); // preserve the caller's callee-saved generator-object cache register
    match target.arch {
        Arch::AArch64 => emitter.instruction("mov x19, x0"),                    // x19 = Generator object passed by __rt_fiber_entry
        Arch::X86_64 => emitter.instruction("mov r12, rdi"),                    // r12 = Generator object passed by __rt_fiber_entry
    };

    // -- unbox each start_args cell into its scratch frame slot --
    for (idx, ty) in param_types.iter().enumerate() {
        let slot = gen_param_slot(idx);
        let load_off = generator_start_arg_offset(idx);
        match target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x0, [x19, #{}]", load_off));  // load the boxed Mixed start argument
            }
            Arch::X86_64 => {
                emitter.instruction(                                            // load the boxed Mixed start argument
                    &format!("mov rax, QWORD PTR [r12 + {}]", load_off)
                );
            }
        }
        if gen_param_kind(ty) == GenParamKind::Mixed {
            abi::store_at_offset(emitter, result_reg, slot); // Mixed parameters keep their boxed cell pointer
            continue;
        }
        match target.arch {
            Arch::AArch64 => emitter.instruction("bl __rt_mixed_unbox"),        // x1 = primary payload, x2 = string length
            Arch::X86_64 => emitter.instruction("call __rt_mixed_unbox"),       // rdi = primary payload, rdx = string length
        }
        match (gen_param_kind(ty), target.arch) {
            (GenParamKind::Float, Arch::AArch64) => {
                emitter.instruction("fmov d0, x1");                             // reinterpret the unboxed float payload bits
                abi::store_at_offset(emitter, "d0", slot);
            }
            (GenParamKind::Float, Arch::X86_64) => {
                emitter.instruction("movq xmm0, rdi");                          // reinterpret the unboxed float payload bits
                abi::store_at_offset(emitter, "xmm0", slot);
            }
            (GenParamKind::Str, Arch::AArch64) => {
                abi::store_at_offset(emitter, "x1", slot);
                abi::store_at_offset(emitter, "x2", slot - 8);
            }
            (GenParamKind::Str, Arch::X86_64) => {
                abi::store_at_offset(emitter, "rdi", slot);
                abi::store_at_offset(emitter, "rdx", slot - 8);
            }
            (GenParamKind::TaggedScalar, Arch::AArch64) => {
                abi::store_at_offset(emitter, "x1", slot); // preserve the payload word
                abi::store_at_offset(emitter, "x0", slot - 8); // preserve the Mixed runtime tag
            }
            (GenParamKind::TaggedScalar, Arch::X86_64) => {
                abi::store_at_offset(emitter, "rdi", slot); // preserve the payload word
                abi::store_at_offset(emitter, "rax", slot - 8); // preserve the Mixed runtime tag
            }
            (_, Arch::AArch64) => abi::store_at_offset(emitter, "x1", slot),
            (_, Arch::X86_64) => abi::store_at_offset(emitter, "rdi", slot),
        }
    }

    // -- forward the unboxed parameters to the body through the normal call ABI --
    // Push each parameter onto the temporary call stack in source order, then let
    // `materialize_outgoing_args` place them into argument registers and the caller-stack
    // overflow area exactly as an ordinary call would. This routes parameters beyond the
    // ABI register budget (e.g. a 7th integer parameter on x86_64 SysV) through the stack
    // instead of dropping them.
    for (idx, ty) in param_types.iter().enumerate() {
        let slot = gen_param_slot(idx);
        match gen_param_kind(ty) {
            GenParamKind::Float => {
                let freg = match target.arch {
                    Arch::AArch64 => "d0",
                    Arch::X86_64 => "xmm0",
                };
                abi::load_at_offset(emitter, freg, slot);
                abi::emit_push_float_reg(emitter, freg); // stage the float parameter on the temporary call stack
            }
            GenParamKind::Str | GenParamKind::TaggedScalar => {
                let (lo, hi) = match target.arch {
                    Arch::AArch64 => ("x9", "x10"),
                    Arch::X86_64 => ("r10", "r11"),
                };
                abi::load_at_offset(emitter, lo, slot);
                abi::load_at_offset(emitter, hi, slot - 8);
                abi::emit_push_reg_pair(emitter, lo, hi); // stage the two-word string/tagged-scalar argument
            }
            GenParamKind::IntLike | GenParamKind::Mixed => {
                let reg = match target.arch {
                    Arch::AArch64 => "x9",
                    Arch::X86_64 => "r10",
                };
                abi::load_at_offset(emitter, reg, slot);
                abi::emit_push_reg(emitter, reg); // stage the scalar/pointer parameter on the temporary call stack
            }
        }
    }
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);
    // ARM64 keeps a 16-byte nested-call save slot below the outgoing stack arguments;
    // x86_64 needs no such pad because the call instruction itself pushes the return address.
    let call_pad = if overflow_bytes > 0 && target.arch == Arch::AArch64 {
        16
    } else {
        0
    };
    abi::emit_reserve_temporary_stack(emitter, call_pad);

    // -- run the body, park its return value, and yield a null fiber transfer value --
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("bl {}", body_label));                 // run the generator body to completion; x0 = boxed return value
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("call {}", body_label));               // run the generator body to completion; rax = boxed return value
        }
    }
    abi::emit_release_temporary_stack(emitter, call_pad); // drop the ARM64 nested-call alignment pad
    abi::emit_release_temporary_stack(emitter, overflow_bytes); // drop any stack-passed parameters after the body returns
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction(                                                // park the body return value for getReturn()
                &format!("str x0, [x19, #{}]", GEN_RETURN_VALUE_OFFSET)
            );
            emitter.instruction("mov x0, #0");                                  // hand the fiber transfer value a null so it does not alias the return
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(                                       // park the body return value for getReturn()
                "mov QWORD PTR [r12 + {}], rax",
                GEN_RETURN_VALUE_OFFSET
            ));
            emitter.instruction("xor eax, eax");                                // hand the fiber transfer value a null so it does not alias the return
        }
    }
    abi::load_at_offset(emitter, gen_cache, GEN_SAVE_OFF); // restore the caller's callee-saved generator-object cache register
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Returns the runtime metadata entry label for an EIR class-method function.
fn class_method_entry_symbol(function: &Function) -> Result<String> {
    let Some((class_name, method_name)) = function.name.rsplit_once("::") else {
        return Err(CodegenIrError::invalid_module(format!(
            "class method function '{}' has no class receiver",
            function.name
        )));
    };
    let method_key = php_symbol_key(method_name);
    if function.flags.is_static {
        Ok(static_method_symbol(class_name, &method_key))
    } else {
        Ok(method_symbol(class_name, &method_key))
    }
}

/// Emits the EIR main function as the process entry point.
///
/// When `web` is false this is the normal exit-based process entry. When `web`
/// is true the top-level body is emitted as the C-callable `_elephc_web_handler`
/// (a `ret`-based function that runs the body and returns without exiting), and
/// a separate process-entry stub is emitted that calls `elephc_web_run` with
/// argc/argv and the handler address, then exits with the bridge return value.
#[allow(clippy::too_many_arguments)]
fn emit_main_function(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    gc_stats: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    regalloc_linear: bool,
    web: bool,
    web_isolation: WebIsolation,
) -> Result<()> {
    let entry_symbol = if web {
        frame::WEB_HANDLER_SYMBOL
    } else {
        emitter.entry_symbol()
    };
    emit_fn_marker(emitter, &function.name, entry_symbol, false);
    let mut layout = frame::layout_for_function(
        function,
        emitter.target,
        regalloc_linear,
        false,
        frame::module_uses_backtrace(module),
    );
    if web {
        frame::preserve_host_callee_saved(&mut layout, emitter.target);
    }
    let mut ctx = FunctionContext::new(
        module, function, emitter, data, shared, layout, true, gc_stats, heap_debug, None,
    );
    if web {
        ctx.web = true;
        frame::emit_web_handler_prologue(&mut ctx);
    } else {
        frame::emit_main_prologue(&mut ctx);
    }
    if requires_elephc_tls {
        crate::codegen::tls::publish_tls_function_pointers(ctx.emitter);
    }
    if module.required_runtime_features.mbstring || module.required_runtime_features.eval_bridge {
        if module.mbstring_startup.is_some() {
            abi::emit_call_label(ctx.emitter, "__rt_mbstring_startup");
        }
        // This entry runs once per CLI invocation or once per web request. Eval fragments
        // enter below it, preserving the same request settings and encoding lookup cache.
        abi::emit_call_label(ctx.emitter, "__rt_mbstring_request_reset");
    }
    // Enum cases are NOT initialized here any more: each case now materializes on
    // its first evaluation through `super::enum_singletons`, so a case that user
    // code never touches allocates nothing and burns no object handle — which is
    // what PHP does and what the eager prologue diverged from.
    emit_static_property_initializers(&mut ctx)?;
    emit_blocks(&mut ctx)?;
    if !ctx.epilogue_emitted {
        if web {
            frame::emit_web_handler_epilogue(&mut ctx);
        } else {
            frame::emit_main_epilogue(&mut ctx);
        }
    }
    emit_endfn_marker(ctx.emitter, function.display_name());
    if web {
        frame::emit_web_entry_stub(&mut ctx, web_isolation)?;
    }
    Ok(())
}

/// Returns true when a function is the process entry function.
///
/// The `is_main` flag decides. The bare name only stands in for a module that flags no entry at
/// all (hand-built modules): a PHP program may declare its own `function main()`, which the
/// module lists BEFORE the flagged entry, so a name match picked it as the process entry and
/// dropped it from the user bodies -- top-level code was lost and eval's function registration
/// referenced an `_fn_main` that was never emitted.
fn is_main(module: &Module, function: &Function) -> bool {
    function.flags.is_main
        || (function.name == "main" && !module.functions.iter().any(|f| f.flags.is_main))
}

/// Initializes static-property storage before user code runs.
pub(super) fn emit_static_property_initializers(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let mut default_initializers = Vec::new();
    let mut uninitialized_static_properties = Vec::new();
    let mut null_word_static_properties = Vec::new();
    let mut referenced = super::runtime_referenced_class_names(ctx.module);
    // The eval bridge's static-property helpers (`eval_static_property_helpers`) cover every
    // compiled class, so a fragment can read a static property of a class no native code
    // references; its default and uninitialized sentinel must then exist as well.
    if ctx.module.required_runtime_features.eval_bridge {
        referenced.extend(ctx.module.class_infos.keys().cloned());
    }
    let mut class_names = referenced.into_iter().collect::<Vec<_>>();
    class_names.sort();
    for class_name in class_names {
        let Some(class_info) = ctx.module.class_infos.get(&class_name) else {
            continue;
        };
        if class_info.deferred_property_default_error.is_some() {
            // Initialization raises through EIR before any of this class's storage is observed.
            continue;
        }
        for (index, (property, php_type)) in class_info.static_properties.iter().enumerate() {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if declaring_class != class_name {
                continue;
            }
            let written_default = class_info.static_defaults.get(index).and_then(Option::as_ref);
            // A null default whose slot cannot represent null (a scalar slot
            // rebound by later type refinement) is skipped; the slot is always
            // written before an observable read on those paths.
            let default = written_default.filter(|default_expr| {
                !matches!(default_expr.kind, crate::parser::ast::ExprKind::Null)
                    || php_type.null_property_default_required()
            });
            if let Some(default_expr) = default {
                default_initializers.push((
                    class_name.clone(),
                    property.clone(),
                    php_type.clone(),
                    default_expr.kind.clone(),
                ));
            } else if class_info.declared_static_properties.contains(property) {
                if written_default.is_some()
                    && matches!(php_type.codegen_repr(), PhpType::Pointer(_))
                {
                    // A pointer slot whose null default was skipped above, such as
                    // `public static ptr $p = null;`, holds null as the zero word: it is
                    // initialized, not waiting for its first write.
                    null_word_static_properties.push((class_name.clone(), property.clone()));
                } else {
                    uninitialized_static_properties.push((class_name.clone(), property.clone()));
                }
            }
        }
    }
    for (class_name, property) in uninitialized_static_properties {
        emit_static_property_sentinel(ctx, &class_name, &property);
    }
    for (class_name, property) in null_word_static_properties {
        emit_static_property_null_word(ctx, &class_name, &property);
    }
    for (class_name, property, php_type, expr) in default_initializers {
        // The refusal belongs to the declaring class, not to the entry program that runs this
        // prologue: an initializer naming a class constant the compiled world lacks (a class
        // left to the runtime autoloader) hands the declaring file back to the runtime too.
        emit_static_property_default(ctx, &class_name, &property, &php_type, &expr)
            .map_err(|error| error.owned_by(static_property_owner_body(&class_name, &property)))?;
    }
    Ok(())
}

/// Names the static property a refused initializer is recorded against (`Class::$prop`).
///
/// `autoload::deferral::BackendDeferral::for_module` maps the same spelling to the declaring
/// class's file.
pub(crate) fn static_property_owner_body(class_name: &str, property: &str) -> String {
    format!("{}::${}", class_name, property)
}

/// Marks one typed static property without a default as uninitialized.
///
/// The VALUE word goes back to zero with the marker, and it must. `__rt_web_reset` releases what
/// the property held and leaves the pointer in place; this block then runs again for the next
/// request and says "uninitialized" while the dead pointer is still sitting there. The first
/// store to the property releases whatever it finds in that word before overwriting it, so the
/// leftover made it free an address the heap had already handed to someone else. Symfony's
/// `Container::$make` coalesce-assign is exactly that shape: on the second request it freed a
/// live service-id string and the worker took a segmentation fault. Zero releases as a no-op.
fn emit_static_property_sentinel(ctx: &mut FunctionContext<'_>, class_name: &str, property: &str) {
    ctx.emitter.comment(&format!(
        "mark static property {}::${} uninitialized",
        class_name, property
    ));
    let symbol = static_property_symbol(class_name, property);
    abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 0);
    let marker_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    abi::emit_store_reg_to_symbol(ctx.emitter, marker_reg, &symbol, 8);
}

/// Initializes one typed static property whose null default is the zero word.
///
/// A `ptr` slot reads address 0 as its null pointer, so `public static ptr $p = null;` needs
/// no literal: both storage words are zeroed, the value word to the null address and the
/// high word to the initialized marker. The store is explicit rather than relying on the
/// zero-filled symbol so a web request starts from the default again.
fn emit_static_property_null_word(ctx: &mut FunctionContext<'_>, class_name: &str, property: &str) {
    ctx.emitter.comment(&format!(
        "initialize static property {}::${} to the null word",
        class_name, property
    ));
    let symbol = static_property_symbol(class_name, property);
    abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 0);
    abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 8);
}

/// Writes a supported literal static-property default into its symbol storage.
fn emit_static_property_default(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    php_type: &PhpType,
    expr: &ExprKind,
) -> Result<()> {
    ensure_static_property_default_type_supported(class_name, property, php_type)?;
    if let Some(value) = crate::codegen::literal_defaults::enum_case_literal_default(
        ctx.module, class_name, php_type, expr,
    ) {
        ctx.emitter.comment(&format!(
            "initialize static property {}::${}",
            class_name, property
        ));
        return emit_static_property_default_value(ctx, class_name, property, php_type, &value);
    }
    let resolved = super::const_default_values::resolve_const_default(
        &crate::parser::ast::Expr::new(expr.clone(), crate::span::Span::synthetic()),
        &super::const_default_values::ConstDefaultContext::for_class(ctx.module, class_name),
    ).and_then(super::const_default_values::resolved_literal_default);
    let expr = resolved.as_ref().unwrap_or(expr);
    // Whatever the shared constant resolver left symbolic: class-like constants of the owner
    // and the program's global constants.
    let expr = crate::codegen::eval_class_constant_helpers::resolve_class_like_constants_in_literal(
        ctx.module,
        class_name,
        expr,
    );
    let expr = crate::codegen::literal_defaults::resolve_literal_default_global_constants(
        &expr,
        &ctx.module.global_constants,
    );
    let value = literal_default_value(
        &format!("static property {}::${}", class_name, property),
        php_type,
        &expr,
        "static property initializer",
    )?;
    ctx.emitter.comment(&format!(
        "initialize static property {}::${}",
        class_name, property
    ));
    emit_static_property_default_value(ctx, class_name, property, php_type, &value)?;
    Ok(())
}

/// Verifies the EIR static-property initializer has a direct storage representation.
fn ensure_static_property_default_type_supported(
    class_name: &str,
    property: &str,
    php_type: &PhpType,
) -> Result<()> {
    match php_type {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Void
        | PhpType::Never
        | PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Object(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Union(_) => Ok(()),
        _ => Err(CodegenIrError::unsupported(format!(
            "static property initializer for {}::${} with PHP type {:?}",
            class_name, property, php_type
        ))),
    }
}

/// Emits the target-specific literal load and symbol store for one static-property default.
fn emit_static_property_default_value(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    php_type: &PhpType,
    value: &LiteralDefaultValue,
) -> Result<()> {
    match value {
        // Only object allocation builds an initializer-thunk default.
        LiteralDefaultValue::InitThunk { .. } => {
            return Err(CodegenIrError::unsupported(format!(
                "initializer thunk default for static property {}::${}",
                class_name, property
            )));
        }
        LiteralDefaultValue::Int(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, *value);
        }
        LiteralDefaultValue::Bool(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, i64::from(*value));
        }
        LiteralDefaultValue::Float(value) => {
            let label = ctx.data.add_float(*value);
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_symbol_to_reg(ctx.emitter, float_reg, &label, 0);
        }
        LiteralDefaultValue::Str(value) => {
            emit_string_literal_default_to_result(ctx, value);
        }
        LiteralDefaultValue::Null => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        LiteralDefaultValue::NullSentinel => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
        LiteralDefaultValue::TaggedNull => {
            emit_tagged_null_literal_to_result(ctx);
        }
        LiteralDefaultValue::TaggedInt(value) => {
            emit_tagged_int_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::BoxedNull => {
            emit_boxed_null_literal_to_result(ctx);
        }
        LiteralDefaultValue::EnumCase {
            enum_name,
            case_name,
            boxed,
        } => {
            crate::codegen::literal_defaults::emit_enum_case_default_to_result(
                ctx, enum_name, case_name, *boxed,
            );
        }
        LiteralDefaultValue::BoxedStr(value) => {
            emit_boxed_string_literal_default_to_result(ctx, value);
        }
        LiteralDefaultValue::BoxedInt(value) => {
            emit_boxed_int_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::BoxedBool(value) => {
            emit_boxed_bool_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::BoxedFloat(value) => {
            emit_boxed_float_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::Array {
            elem_type,
            elements,
        } => {
            emit_array_literal_default_to_result(ctx, elem_type, elements)?;
            if matches!(php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
                crate::codegen::emit_box_current_value_as_mixed(
                    ctx.emitter,
                    &PhpType::Array(Box::new(elem_type.clone())),
                );
            }
        }
        LiteralDefaultValue::AssocArray {
            value_type,
            entries,
        } => {
            emit_assoc_array_literal_default_to_result(ctx, value_type, entries)?;
            if matches!(php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
                crate::codegen::emit_box_current_value_as_mixed(
                    ctx.emitter,
                    &PhpType::AssocArray {
                        key: Box::new(PhpType::Mixed),
                        value: Box::new(value_type.clone()),
                    },
                );
            }
        }
        LiteralDefaultValue::EmptyAssocArray { value_type } => {
            emit_empty_assoc_array_literal_to_result(ctx, value_type);
        }
        LiteralDefaultValue::BoxedArray {
            elem_type,
            elements,
        } => {
            emit_array_literal_default_to_result(ctx, elem_type, elements)?;
            // The OWNED boxer, because the literal above allocated the array and the box takes
            // its own reference: the plain one retains without releasing, which leaked one block
            // per object (`allocs=3 frees=2` where the `mixed $s = "x"` default closed at 3/3).
            crate::codegen::emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(elem_type.clone())),
            );
        }
        LiteralDefaultValue::BoxedAssocArray {
            value_type,
            entries,
        } => {
            emit_assoc_array_literal_default_to_result(ctx, value_type, entries)?;
            crate::codegen::emit_box_current_owned_value_as_mixed(
                ctx.emitter,
                &PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(value_type.clone()),
                },
            );
        }
    }
    let symbol = static_property_symbol(class_name, property);
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, php_type, false);
    if !matches!(
        php_type.codegen_repr(),
        PhpType::Str | PhpType::TaggedScalar
    ) {
        abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 8);
    }
    Ok(())
}

/// Emits every block in machine layout order, then the function's deferred cold paths.
///
/// Hot blocks keep their table order and cold (`unreachable` / `fatal` / `throw`) blocks move
/// last. Each block knows its layout successor so terminators can fall through instead of
/// jumping. The finished region then goes through the late machine peepholes.
fn emit_blocks(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let region_start = ctx.emitter.text_len();
    // Borrowed for the same reason as the instruction in `lower_instruction`:
    // `FunctionContext::function` is a `&'a Function` whose lifetime does not depend on the
    // borrow of `ctx`, so copying the reference out lets the blocks outlive the `&mut ctx`
    // below. The clone this replaces copied every block of every body -- its name, its
    // instruction-id vector and its terminator -- to end a borrow that was never in the way.
    let function = ctx.function;
    let order = block_layout::emission_order(function);
    let use_counts = block_layout::value_use_counts(function);
    let loop_headers = signal_loop_headers(ctx);
    // What the prologue established (`frame::emit_callee_saved_saves`), or `None`.
    let frame_slot_base = ctx.emitter.frame_slot_base;
    ctx.begin_cold_section();
    for (position, &index) in order.iter().enumerate() {
        let block = &function.blocks[index];
        ctx.machine_layout.next_block = order.get(position + 1).map(|&next| function.blocks[next].id);
        ctx.machine_layout.fused_compare =
            block_layout::fusible_compare(function, block, &use_counts);
        // Every block of the body runs with x28 = this frame's base (set by the prologue,
        // restored by `longjmp` at a catch). A `return` lowered inline in an earlier block
        // emits the callee-saved restores, which turn the base off for what follows them in
        // the text; the next block is live code again.
        ctx.emitter.frame_slot_base = frame_slot_base;
        emit_block(ctx, block, index == 0 || loop_headers.contains(&block.id))?;
    }
    ctx.machine_layout.next_block = None;
    ctx.machine_layout.fused_compare = None;
    ctx.flush_cold_section();
    // What follows the blocks -- the shared epilogue, the cleanup callback that borrows another
    // frame's x29, an entry stub -- addresses slots through x29.
    ctx.emitter.frame_slot_base = None;
    // The text peepholes pay off in loops and in small functions (often called from loops);
    // large loop-free bodies (generated helpers and dispatch code) skip them, keeping compile
    // time flat. Block-level layout above still removes their jumps to the next block.
    let target = ctx.emitter.target;
    if block_layout::has_retreating_edge(function)
        || machine_peephole::is_small_region(ctx.emitter.text_since(region_start), target)
    {
        ctx.emitter
            .rewrite_text_from(region_start, |text| machine_peephole::optimize_region(text, target));
    }
    // What follows the blocks -- the shared epilogue, the cleanup callback that borrows another
    // frame's x29, an entry stub -- addresses slots through x29.
    ctx.emitter.frame_slot_base = None;
    Ok(())
}

/// The blocks that start with an async-signal safe point: the loop headers, i.e. the targets of
/// a back edge found by a depth-first walk from the entry block.
///
/// php's VM checks for a pending interrupt at function entry, at a loop's back edge and after a
/// call returns; the calls are covered by `signal_safe_point_after`, the entry by the first block.
/// A block start that is neither (the join of an `if`, a `catch` body) is a bounded step between
/// two of those points, and a safe point there only cost a call at about 300 000 sites of the
/// Symfony console. Every cycle contains a back edge, so a tight loop still polls. Blocks the walk
/// cannot reach from the entry (landing pads, dead arms) are walked as roots of their own, so a loop in one still polls, but their first block does not.
fn signal_loop_headers(ctx: &FunctionContext<'_>) -> crate::fast_hash::FastSet<crate::ir::BlockId> {
    use crate::ir::{BlockId, Terminator};
    let mut headers = crate::fast_hash::FastSet::default();
    if !ctx.uses_pcntl_async_signals() {
        return headers;
    }
    let function = ctx.function;
    let index_of: crate::fast_hash::FastMap<BlockId, usize> = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect();
    let successors = |index: usize| -> Vec<usize> {
        let mut targets: Vec<BlockId> = Vec::new();
        match &function.blocks[index].terminator {
            Some(Terminator::Br { target, .. }) => targets.push(*target),
            Some(Terminator::CondBr { then_target, else_target, .. }) => {
                targets.push(*then_target);
                targets.push(*else_target);
            }
            Some(Terminator::Switch { cases, default, .. }) => {
                targets.extend(cases.iter().map(|case| case.target));
                targets.push(*default);
            }
            Some(Terminator::GeneratorSuspend { resume, .. }) => targets.push(*resume),
            _ => {}
        }
        targets.iter().filter_map(|target| index_of.get(target).copied()).collect()
    };
    // 0 = unvisited, 1 = on the DFS stack, 2 = finished.
    let mut state = vec![0u8; function.blocks.len()];
    let mut stack: Vec<(usize, Vec<usize>, usize)> = Vec::new();
    for root in 0..function.blocks.len() {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        stack.push((root, successors(root), 0));
        while let Some((node, succ, next)) = stack.last_mut() {
            if *next < succ.len() {
                let child = succ[*next];
                *next += 1;
                match state[child] {
                    0 => {
                        state[child] = 1;
                        let child_successors = successors(child);
                        stack.push((child, child_successors, 0));
                    }
                    1 => {
                        headers.insert(function.blocks[child].id);
                    }
                    _ => {}
                }
            } else {
                state[*node] = 2;
                stack.pop();
            }
        }
    }
    headers
}

/// Whether `ELEPHC_OP_STATS` asks for the per-op instruction histogram.
fn op_stats_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("ELEPHC_OP_STATS").is_some())
}

/// Per EIR op: how many were lowered and how many instructions they emitted.
type OpStats = std::collections::BTreeMap<String, (u64, u64)>;

/// The histogram key of one instruction: its op, refined by the immediate for the generic ops
/// (`runtime_call`, `core_builtin`, `call`) whose cost depends on what they call.
fn op_stats_key(inst: &crate::ir::Instruction) -> String {
    let name = inst.op.name();
    if !matches!(name, "runtime_call" | "core_builtin" | "const_str" | "call" | "method_call") {
        return name.to_string();
    }
    let detail = format!("{:?}", inst.immediate);
    let detail = detail.split(['(', '{', ' ']).next().unwrap_or("").to_string();
    let inner = format!("{:?}", inst.immediate);
    let inner = inner.trim_start_matches(&detail).chars().take(40).collect::<String>();
    format!("{name}[{detail}{inner}]")
}

fn op_stats() -> &'static std::sync::Mutex<OpStats> {
    static STATS: std::sync::OnceLock<std::sync::Mutex<OpStats>> = std::sync::OnceLock::new();
    STATS.get_or_init(Default::default)
}

fn record_op_stats(op: String, instructions: usize) {
    if let Ok(mut stats) = op_stats().lock() {
        let entry = stats.entry(op).or_default();
        entry.0 += 1;
        entry.1 += instructions as u64;
    }
}

/// Prints the per-op histogram collected under `ELEPHC_OP_STATS=1`, largest total first.
pub fn print_op_stats() {
    if !op_stats_enabled() {
        return;
    }
    let Ok(stats) = op_stats().lock() else {
        return;
    };
    let total: u64 = stats.values().map(|(_, instructions)| instructions).sum();
    let mut rows = stats.iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| right.1 .1.cmp(&left.1 .1));
    eprintln!("OPSTATS total_instructions={total}");
    for (op, (count, instructions)) in rows {
        eprintln!(
            "OPSTATS {op} count={count} instructions={instructions} average={:.1}",
            *instructions as f64 / (*count).max(1) as f64
        );
    }
}

/// Emits one EIR basic block.
fn emit_block(ctx: &mut FunctionContext<'_>, block: &BasicBlock, entry_safe_point: bool) -> Result<()> {
    ctx.emitter.comment(&format!("@block name={}", block.name));
    let block_label = ctx.block_label_for_id(block.id)?;
    ctx.emitter.label(&block_label);
    // Async signal handlers run at safe points, where php's VM checks for an interrupt: after a
    // call returns and on a backward jump. A block start covers every back edge (and forward
    // branches, which costs little); `signal_safe_point_after` covers the calls. Checking after
    // EVERY instruction, as this used to, put a million dispatch calls into the Symfony build and
    // spent 3.4% of a request in them.
    if entry_safe_point && ctx.uses_pcntl_async_signals() {
        emit_signal_safe_point(ctx);
    }
    for inst_id in &block.instructions {
        emit_instruction_source_marker(ctx, *inst_id)?;
        let stats_mark = op_stats_enabled().then(|| ctx.emitter.text_mark());
        let outline_mark = super::op_outline::enabled().then(|| ctx.emitter.checkpoint());
        lower_inst::lower_instruction(ctx, *inst_id).map_err(|error| {
            let location = instruction_location(ctx, *inst_id);
            error.at(location)
        })?;
        if let Some(mark) = outline_mark {
            super::op_outline::outline_instruction(ctx, mark);
        }
        if let Some(mark) = stats_mark {
            if let Some(inst) = ctx.function.instruction(*inst_id) {
                let instructions = ctx.emitter.instructions_since(mark);
                // The sites behind a large average: which function, how big.
                if instructions >= 4000 {
                    eprintln!("OPSITE {} {} instructions={instructions}", inst.op.name(), ctx.function.name);
                }
                record_op_stats(op_stats_key(inst), instructions);
            }
        }
        // A worker arms this watch around each body; nothing else does, so on every other path
        // the check is one `Option` read. See `CodegenIrError::deferred_body`.
        if ctx.shared.must_defer() {
            return Err(CodegenIrError::deferred_body());
        }
        if ctx.uses_pcntl_async_signals() && signal_safe_point_after(ctx, *inst_id) {
            emit_signal_safe_point(ctx);
        }
    }
    let terminator = block.terminator.as_ref().ok_or_else(|| {
        CodegenIrError::invalid_module(format!("block '{}' has no terminator", block.name))
    })?;
    lower_term::lower_terminator(ctx, terminator)
}

/// Emits one async-signal safe point: a single call whose callee tests the dispatch flag.
///
/// Every block start and call site of a program that can enable async signals is a safe point --
/// 21,000 of them per Symfony request, 817,520 in the Symfony console's code. The flag test used
/// to be inline (adrp/add/ldr/cbz plus the call) for speed, which made it 2.5% of that code. On
/// AArch64 `__rt_pcntl_safe_point` now does the test with `x16` alone and no flag change, so the
/// site is one `bl` and the fast path stays a handful of predicted instructions. The x86_64
/// helper's fast path is already flag-free, so that target calls it directly.
fn emit_signal_safe_point(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        // One call per site: `__rt_pcntl_safe_point` tests the flag itself (see its emitter).
        Arch::AArch64 => abi::emit_call_label(ctx.emitter, "__rt_pcntl_safe_point"),
        Arch::X86_64 => {
            abi::emit_call_label(ctx.emitter, "__rt_pcntl_async_dispatch_preserving");
        }
    }
}

/// Returns whether an async signal may be dispatched right after this instruction.
///
/// These are the instructions that run PHP code or a builtin, the places php's VM checks for a
/// pending interrupt once the callee returns: calls of every shape, constructions (which run a
/// constructor), eval and include entries, and generator resumption. Everything else is a
/// bounded step between two such points.
fn signal_safe_point_after(ctx: &FunctionContext<'_>, inst_id: InstId) -> bool {
    let Some(inst) = ctx.function.instruction(inst_id) else {
        return false;
    };
    matches!(
        inst.op,
        Op::Call
            | Op::FunctionVariantCall
            | Op::MethodCall
            | Op::NullsafeMethodCall
            | Op::StaticMethodCall
            | Op::IteratorMethodCall
            | Op::ClosureCall
            | Op::ExprCall
            | Op::PipeCall
            | Op::CallableDescriptorInvoke
            | Op::LanguageConstructCall
            | Op::RuntimeCall
            | Op::SplRuntimeCall
            | Op::ExternCall
            | Op::FiberRuntimeCall
            | Op::ObjectNew
            | Op::DynamicObjectNew
            | Op::DynamicObjectNewMixed
            | Op::DynamicObjectNewWithoutConstructorMixed
            | Op::DynamicPdoStatementConstructorCall
            | Op::EvalObjectNew
            | Op::EvalStaticMethodCall
            | Op::EvalLiteralCall
            | Op::EvalFunctionCall
            | Op::EvalFunctionCallArray
            | Op::GeneratorYield
            | Op::GeneratorYieldFrom
    )
}

/// Describes the source line and opcode associated with an EIR instruction.
fn instruction_location(ctx: &FunctionContext<'_>, inst_id: InstId) -> String {
    let Some(inst) = ctx.function.instruction(inst_id) else {
        return format!("op #{}", inst_id.as_raw());
    };
    // The enclosing function is named for the same reason the checker records a declaration and
    // the EIR validator names its own: an `Instruction` carries a `Span` with line and column but
    // NO file, and after include/autoload expansion one `line:col` belongs to every spliced file
    // at once. The function is what turns a backend gap into something a reader can go and open.
    match inst.span.filter(|span| span.line > 0) {
        Some(span) => format!(
            "{}:{}, op {}, in {}",
            span.line,
            span.col,
            inst.op.name(),
            ctx.function.name
        ),
        None => format!("op {}, in {}", inst.op.name(), ctx.function.name),
    }
}

/// Emits the source-map marker for an EIR instruction when it carries a real PHP span.
/// The marker carries the EIR opcode spelling so v2 maps can label mappings at
/// expression granularity.
pub(super) fn emit_instruction_source_marker(ctx: &mut FunctionContext<'_>, inst_id: InstId) -> Result<()> {
    let Some(inst) = ctx.function.instruction(inst_id) else {
        return Err(CodegenIrError::missing_entry(
            "instruction",
            inst_id.as_raw(),
        ));
    };
    let Some(span) = inst.span else {
        return Ok(());
    };
    if span.line > 0 {
        let mut marker = format!("@src line={} col={}", span.line, span.col);
        if span.has_extent() {
            let _ = write!(marker, " end={}:{}", span.end_line, span.end_column());
        }
        let _ = write!(marker, " op={}", inst.op.name());
        if let Some(origin) = inst.origin {
            let _ = write!(marker, " origin={}", origin.name());
        }
        ctx.emitter.comment(&marker);
    }
    Ok(())
}

/// Emits the source-map marker opening a function region. `name` is the PHP-level
/// function/method name and `symbol` the assembly entry label; the map generator
/// pairs it with the matching `@endfn` to derive the function's assembly range.
/// `synthetic` marks compiler-generated bodies with no user-written PHP source.
fn emit_fn_marker(emitter: &mut Emitter, name: &str, symbol: &str, synthetic: bool) {
    let mut marker = format!("@fn name={} symbol={}", name, symbol);
    if synthetic {
        marker.push_str(" synthetic=1");
    }
    emitter.comment(&marker);
}

/// Emits the source-map marker closing the function region opened by
/// `emit_fn_marker()`.
fn emit_endfn_marker(emitter: &mut Emitter, name: &str) {
    emitter.comment(&format!("@endfn name={}", name));
}

/// Reports which type-argument candidates lower to identical bodies.
///
/// The merge refuses a candidate when its EIR could observe a class name the shape erased, but the
/// EIR cannot see every backend decision: the runtime value tag is baked from the exact `PhpType`
/// at codegen, so two bodies may compare unequal while emitting the same code (a pass-through
/// `id<int>` / `id<bool>` pair), or compare equal while emitting different code (a body boxing its
/// scalar). This oracle lowers each surviving candidate into an isolated emitter and compares the
/// SHAPE-NORMALIZED ASSEMBLY, which is the actual soundness oracle: it includes the frame layout,
/// the epilogue cleanup, every baked tag and every future scalar-consulting lowering.
///
/// The result is the measured case for widening the merge — nothing here changes the emitted
/// program; it is reported under `--report-generics` only.
/// One dispatch slot's fold: the surviving candidate and the bodies folded into it.
pub(crate) struct BodyShareGroup {
    /// The slot's readable name, for the report.
    pub slot: String,
    /// How many candidates the slot carries in total (folded and kept).
    pub slot_candidates: usize,
    /// The candidate kept as the shared implementation.
    pub survivor: String,
    /// The candidates folded into the survivor, by EIR function name.
    pub victims: Vec<String>,
}

/// Groups the module's type-argument candidates whose lowered bodies are byte-identical.
///
/// Within a slot the candidates are lowered in isolation and bucketed by their normalized text;
/// each bucket keeps its first candidate as the survivor and folds the rest into it. A candidate
/// whose body did not lower is left alone.
pub(crate) fn foldable_body_groups(module: &Module, regalloc_linear: bool) -> Vec<BodyShareGroup> {
    // (slot, candidate bodies, is_method)
    let mut rows: Vec<(String, Vec<&Function>, bool)> = Vec::new();
    let mut row_of: crate::fast_hash::FastMap<String, usize> = crate::fast_hash::FastMap::default();
    let emitted_body = |function: &Function| {
        // A STATIC LOCAL's storage symbol is derived from the function's own name, and a global
        // alias names a global by symbol: both are state whose identity the shape erasure would
        // fold into one storage cell for two independent instantiations.
        let stateful = function.locals.iter().any(|slot| {
            matches!(
                slot.kind,
                crate::ir::LocalKind::StaticLocal | crate::ir::LocalKind::GlobalAlias
            )
        });
        !stateful
            && !function.flags.is_main
            && !function.flags.is_generator
            && !function.flags.is_synthetic
            && !function.flags.is_closure
            && !function.flags.is_fiber_wrapper
            && !function.flags.is_callback_wrapper
            && !function.flags.is_runtime_callable_invoker
    };
    for function in &module.functions {
        if !function.name.contains('<') || !emitted_body(function) {
            continue;
        }
        let slot = function.name.split('<').next().unwrap_or(&function.name).to_string();
        match row_of.get(&slot).copied() {
            Some(index) => rows[index].1.push(function),
            None => {
                row_of.insert(slot.clone(), rows.len());
                rows.push((slot, vec![function], false));
            }
        }
    }
    for function in &module.class_methods {
        if !function.name.contains('<') || !emitted_body(function) {
            continue;
        }
        let Some((class, method)) = function.name.rsplit_once("::") else {
            continue;
        };
        let class = class.split('<').next().unwrap_or(class).to_string();
        // The METHOD side carries type arguments too: a generic method on a non-generic class
        // instantiates as `Plain::get<int>` / `Plain::get<string>`, and those siblings are
        // exactly the pair monomorphization duplicates — strip both sides into one slot.
        let method = method.split('<').next().unwrap_or(method).to_string();
        let kind = if function.flags.is_static { "static" } else { "instance" };
        let slot = format!("{class}::{method} [{kind}]");
        match row_of.get(&slot).copied() {
            Some(index) => rows[index].1.push(function),
            None => {
                row_of.insert(slot.clone(), rows.len());
                rows.push((slot, vec![function], true));
            }
        }
    }
    rows.iter()
        .filter(|(_, candidates, _)| candidates.len() >= 2)
        .filter_map(|(slot, candidates, is_method)| {
            // Bucket by the normalized text in one pass; a linear scan against every bucket
            // would compare full bodies O(k²) per slot, which grows with the instantiation
            // count the feature monomorphizes.
            let mut buckets: crate::fast_hash::FastMap<String, Vec<&Function>> =
                crate::fast_hash::FastMap::default();
            let mut order: Vec<(String, &Function)> = Vec::new();
            for function in candidates {
                let Some(text) =
                    normalized_candidate_body(module, function, *is_method, regalloc_linear)
                else {
                    continue;
                };
                match buckets.get_mut(&text) {
                    Some(members) => members.push(function),
                    None => {
                        buckets.insert(text.clone(), vec![function]);
                        order.push((text, function));
                    }
                }
            }
            let mut groups = Vec::new();
            for (text, _) in order {
                let members = &buckets[&text];
                if members.len() >= 2 {
                    groups.push(BodyShareGroup {
                        slot: slot.clone(),
                        slot_candidates: candidates.len(),
                        survivor: members[0].name.clone(),
                        victims: members[1..].iter().map(|body| body.name.clone()).collect(),
                    });
                }
            }
            (!groups.is_empty()).then_some(groups)
        })
        .flatten()
        .collect()
}

/// The measured fold opportunity, reported under `--report-generics`.
pub fn generic_body_sharing_oracle(module: &Module, regalloc_linear: bool) -> String {
    let groups = foldable_body_groups(module, regalloc_linear);
    if groups.is_empty() {
        return "shape-oracle: no further foldable bodies".to_string();
    }
    // (slot, candidates, folded bodies) — a slot's distinct bodies are its candidates minus its
    // folded ones.
    let mut slots: Vec<(String, usize, usize)> = Vec::new();
    for group in &groups {
        match slots.iter_mut().find(|(slot, _, _)| *slot == group.slot) {
            Some((_, _, folded)) => *folded += group.victims.len(),
            None => slots.push((group.slot.clone(), group.slot_candidates, group.victims.len())),
        }
    }
    let mut measured: Vec<(String, usize, usize)> = slots
        .into_iter()
        .map(|(slot, candidates, folded)| (slot, candidates, candidates - folded))
        .collect();
    measured.sort_by(|a, b| (b.1 - b.2).cmp(&(a.1 - a.2)).then(a.0.cmp(&b.0)));
    let total_candidates: usize = measured.iter().map(|(_, candidates, _)| candidates).sum();
    let total_distinct: usize = measured.iter().map(|(_, _, distinct)| distinct).sum();
    let removable = total_candidates - total_distinct;
    let mut lines = vec![format!(
        "shape-oracle: {} slots lower to identical bodies: {} instantiations → {} distinct bodies ({} removable)",
        measured.len(),
        total_candidates,
        total_distinct,
        removable
    )];
    for (slot, candidates, distinct) in measured {
        let body_word = if distinct == 1 { "body" } else { "bodies" };
        lines.push(format!(
            "  {slot}: {candidates} instantiations → {distinct} {body_word}"
        ));
    }
    lines.join("\n")
}

/// The folded-candidate map: a victim's EIR function name to its survivor's.
///
/// The victim KEEPS its EIR function, so a caller still resolves its parameter types and picks its
/// own coercion from the original instantiation; only the machine code is shared.
pub(crate) fn generic_body_aliases(
    module: &Module,
    regalloc_linear: bool,
) -> crate::fast_hash::FastMap<String, String> {
    foldable_body_groups(module, regalloc_linear)
        .into_iter()
        .flat_map(|group| {
            group
                .victims
                .into_iter()
                .map(move |victim| (victim, group.survivor.clone()))
        })
        .collect()
}

/// Emits a folded candidate's entry as a jump to the survivor's body.
///
/// The candidate KEEPS its EIR function, so a caller still resolves its parameter types and picks
/// its own coercion from the original instantiation; only the machine code is shared. No cleanup
/// symbol is emitted: a cleanup activation is published by the body's own prologue
/// (`frame::emit_exception_activation_push`), and a folded body has no prologue — the survivor's
/// body publishes the activation for the frame that actually runs.
pub(super) fn emit_folded_body(
    function: &Function,
    survivor: &str,
    emitter: &mut Emitter,
) -> Result<()> {
    let is_static = function.flags.is_static;
    let entry = entry_symbol_for_name(&function.name, is_static);
    let survivor_entry = entry_symbol_for_name(survivor, is_static);
    // The source map discovers functions through `@fn`/`@endfn` regions; a folded body is a real
    // function whose range happens to be one jump, so it still needs its record.
    emit_fn_marker(emitter, &function.name, &entry, false);
    // `label_global` gives the thunk the same section and `%function` type the full bodies get
    // on Linux; a bare `.globl` would leave the jump in the PREVIOUS symbol's section and skip
    // the type annotation, which misdirects `--gc-sections` and debug extents.
    emitter.label_global(&entry);
    abi::emit_jump(emitter, &survivor_entry);
    emit_endfn_marker(emitter, &function.name);
    Ok(())
}

/// Returns the entry symbol of an EIR function name, method or free function.
fn entry_symbol_for_name(name: &str, is_static: bool) -> String {
    match name.rsplit_once("::") {
        Some((class, method)) => {
            let key = php_symbol_key(method);
            if is_static {
                static_method_symbol(class, &key)
            } else {
                method_symbol(class, &key)
            }
        }
        None => function_symbol(name),
    }
}

/// Lowers one candidate body into an isolated emitter and returns its shape-normalized text.
///
/// A fresh emitter, literal pool and shared state make the text depend only on the body and the
/// module's read-only context, so two structurally identical candidates lower to the same text
/// instead of inheriting whatever label counters and caches the whole-module emission had reached.
/// `None` means the body did not lower, which the oracle reports as a candidate it cannot compare.
fn normalized_candidate_body(
    module: &Module,
    function: &Function,
    is_method: bool,
    regalloc_linear: bool,
) -> Option<String> {
    let mut emitter = Emitter::new(module.target);
    let mut data = DataSection::new();
    let mut shared = SharedCodegenState::for_module(module);
    let lowered = frame::without_frame_reader(|| {
        if is_method {
            emit_class_method(
                module,
                function,
                &mut emitter,
                &mut data,
                &mut shared,
                regalloc_linear,
            )
        } else {
            emit_user_function(
                module,
                function,
                &mut emitter,
                &mut data,
                &mut shared,
                regalloc_linear,
            )
        }
    });
    lowered.ok()?;
    // The DATA the body references is part of its meaning: two isolated pools assign the same
    // label to literals of different contents (a seven-byte "integer" and a seven-byte "boolean"
    // both intern as the pool's first entry), so comparing the text alone would fold bodies whose
    // literals disagree. The pool's own emission is the missing half of the comparison.
    let mut text = emitter.output();
    let data_output = data.emit(module.target);
    if !data_output.is_empty() {
        text.push_str(&data_output);
    }
    Some(normalize_body_text(&text, function))
}

/// Erases every symbol that names the candidate and canonicalizes internal labels, so two
/// structurally identical bodies compare byte-equal.
///
/// The candidate's own mangled name reaches the text as its entry, epilogue and exceptional
/// cleanup symbols, and reaches internal labels as part of their prefix. The erasure is
/// operand-aware: a symbol is replaced only where it ENDS at a word boundary, so a sibling
/// callee whose mangled name merely has the candidate's entry as a prefix (method `foobar`
/// alongside method `foo`) keeps its own name — a blanket substring replace would fold the two
/// distinct callees into one and send the folded body to the wrong call target. Each suffix
/// symbol is erased under its own exact spelling for the same reason. The raw name is what the
/// source-map `@fn` marker quotes.
///
/// The erasure deliberately SKIPS `.ascii` payload lines: a body may embed its own instantiated
/// name as data, and that data is a real difference between two instantiations — erasing it
/// would fold bodies whose runtime output differs.
fn normalize_body_text(text: &str, function: &Function) -> String {
    let self_symbol = crate::names::mangle_fqn(&function.name);
    let entry_symbol = entry_symbol_for_name(&function.name, function.flags.is_static);
    // Order matters: the longer suffix symbols must be erased before the entry symbol, or the
    // entry erasure would consume their prefix and leave a mangled remainder behind.
    let own_symbols = [
        format!("{entry_symbol}__bt_reader"),
        format!("{entry_symbol}__cdylib_exception_cleanup"),
        format!("{entry_symbol}_epilogue"),
        entry_symbol.clone(),
        self_symbol,
        function.name.clone(),
    ];
    // A method's symbols are NOT derived from the mangled full name: `method_symbol(class, key)`
    // mangles the class and the method separately, so the entry symbol has to be erased on its
    // own. Replacing it also erases its epilogue and exceptional-cleanup symbols, which are built
    // as suffixes of it.
    let erased: String = text
        .split_inclusive('\n')
        .map(|line| {
            if line.trim_start().starts_with(".ascii") {
                return line.to_string();
            }
            let mut erased_line = line.to_string();
            for symbol in &own_symbols {
                erased_line = erase_word_bounded(&erased_line, symbol);
            }
            erased_line
        })
        .collect();
    let mut mapping: Vec<(String, String)> = Vec::new();
    for line in erased.split_inclusive('\n') {
        let trimmed = line.trim_end();
        let Some(token) = trimmed.strip_suffix(':') else {
            continue;
        };
        if !is_internal_label(token) {
            continue;
        }
        label_map_push(&mut mapping, token);
    }
    let mut out = String::with_capacity(erased.len());
    for line in erased.split_inclusive('\n') {
        let trimmed = line.trim_end();
        // A `.ascii` payload is data, not control flow: its bytes may spell a label by
        // coincidence, and rewriting them would erase a real difference between two bodies.
        // Label DEFINITIONS never sit on payload lines, so the mapping above is complete.
        if line.trim_start().starts_with(".ascii") {
            out.push_str(line);
            continue;
        }
        if let Some(token) = trimmed.strip_suffix(':') {
            if is_internal_label(token) {
                out.push_str(&format!("{}:\n", label_of(&mapping, token)));
                continue;
            }
        }
        let mut rewritten = line.to_string();
        for (raw, canonical) in &mapping {
            rewritten = replace_symbol_token(&rewritten, raw, canonical);
        }
        out.push_str(&rewritten);
    }
    out
}

/// Whether a label token is one of the body's internal labels, which carry no cross-candidate
/// meaning. The Linux assembler spells them `.L_eir_…` where Apple spells them `L_eir_…`.
fn is_internal_label(token: &str) -> bool {
    token.starts_with("L_") || token.starts_with(".L")
}

/// Replaces every occurrence of `symbol` that ends AND starts at symbol boundaries.
///
/// Both edges matter: the character AFTER the match decides whether the match is the whole of a
/// longer symbol (method `foobar` alongside method `foo`), and the character BEFORE decides
/// whether the match is the TAIL of one (callee `xf` alongside candidate `f` — the callee's
/// mangled symbol contains the candidate's as a suffix). Missing either edge rewrites a
/// DIFFERENT name's operand and fuses two distinct callees into one.
fn erase_word_bounded(line: &str, symbol: &str) -> String {
    if symbol.is_empty() {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let is_symbol_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0;
    while let Some(offset) = line[cursor..].find(symbol) {
        let start = cursor + offset;
        let end = start + symbol.len();
        let before = start == 0 || !is_symbol_byte(bytes[start - 1]);
        let after = end >= bytes.len() || !is_symbol_byte(bytes[end]);
        out.push_str(&line[cursor..start]);
        if before && after {
            out.push_str("<SELF>");
        } else {
            out.push_str(symbol);
        }
        cursor = end;
    }
    out.push_str(&line[cursor..]);
    out
}

/// Maps an internal label token to its canonical name by order of first definition.
fn label_map_push(mapping: &mut Vec<(String, String)>, token: &str) -> String {
    if let Some((_, canonical)) = mapping.iter().find(|(raw, _)| raw == token) {
        return canonical.clone();
    }
    let canonical = format!("L{}", mapping.len());
    mapping.push((token.to_string(), canonical.clone()));
    canonical
}

/// Returns the canonical name of an already-mapped label token.
fn label_of(mapping: &[(String, String)], token: &str) -> String {
    mapping
        .iter()
        .find(|(raw, _)| raw == token)
        .map(|(_, canonical)| canonical.clone())
        .unwrap_or_else(|| token.to_string())
}

/// Replaces whole-token occurrences of `raw` with `canonical`, leaving longer symbols alone.
fn replace_symbol_token(line: &str, raw: &str, canonical: &str) -> String {
    let is_symbol_byte =
        |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut out = String::with_capacity(line.len());
    let mut cursor = 0;
    while let Some(offset) = line[cursor..].find(raw) {
        let start = cursor + offset;
        let end = start + raw.len();
        let before_ok = start == 0 || !is_symbol_byte(line.as_bytes()[start - 1]);
        let after_ok = end >= line.len() || !is_symbol_byte(line.as_bytes()[end]);
        if before_ok && after_ok {
            out.push_str(&line[cursor..start]);
            out.push_str(canonical);
        } else {
            out.push_str(&line[cursor..end]);
        }
        cursor = end;
    }
    out.push_str(&line[cursor..]);
    out
}

/// Regression tests for assembly-body equivalence and literal payload preservation.
#[cfg(test)]
mod normalization_tests {
    use super::*;

    /// The single-pass key scan records what the per-line scan recorded: the trimmed rest of
    /// each line after its first marker, across CRLF endings, a final line without a newline,
    /// and two markers on one line.
    #[test]
    fn helper_keys_are_the_rest_of_each_marked_line() {
        let open = crate::codegen::context::HELPER_MARKER_OPEN;
        let text = format!(
            "    mov x0, x1\n; {open}alpha \r\n; {open}beta {open}gamma\nret\n; {open} delta"
        );
        let mut seen = HashSet::default();
        collect_helper_keys(&text, &mut seen);
        let mut keys: Vec<_> = seen.into_iter().collect();
        keys.sort();
        assert_eq!(keys, vec!["alpha".to_string(), format!("beta {open}gamma"), "delta".to_string()]);
    }

    /// Label canonicalization must leave indented string data byte-for-byte intact.
    #[test]
    fn indented_ascii_preserves_internal_label_spelling() {
        let function = Function::new(
            "payload_fixture".to_string(), crate::ir::IrType::Void, PhpType::Void,
        );
        let first = "L_payload_17:\n    b L_payload_17\n    .ascii \"L_payload_17\"\n";
        let second = "L_payload_18:\n    b L_payload_18\n    .ascii \"L_payload_18\"\n";
        let normalized = normalize_body_text(first, &function);
        assert!(normalized.contains("    .ascii \"L_payload_17\""));
        assert!(!normalized.contains("b L_payload_17"));
        assert_ne!(normalized, normalize_body_text(second, &function));
    }
}

#[cfg(test)]
mod backend_inventory_tests {
    //! Purpose:
    //! Regression tests for scan-and-report backend inventory behavior.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests use an explicit switch instead of mutating the process environment.

    use super::{BackendInventory, CodegenIrError};
    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::codegen::shared_reflection::{
        ReflectionMaterializerKey, ReflectionOwnerKind,
    };
    use crate::codegen::shared_state::SharedCodegenState;
    use crate::codegen_support::emit::Emitter;
    use crate::ir::Module;

    /// Builds a fixed-target emitter whose buffer can be inspected directly.
    fn emitter() -> Emitter {
        Emitter::new(Target::new(Platform::Linux, Arch::AArch64))
    }

    /// Verifies disabled inventory preserves abort-on-first-error behavior.
    #[test]
    fn disabled_inventory_propagates_refusals() {
        let mut inventory = BackendInventory::new(false);
        let mut emitter = emitter();
        let outcome = inventory.run(&mut emitter, "f", |_| {
            Err(CodegenIrError::unsupported("gap"))
        });
        assert!(outcome.is_err());
        assert!(inventory.finish().is_ok());
    }

    /// Verifies enabled inventory rolls back a failed body and continues scanning.
    #[test]
    fn enabled_inventory_rolls_back_and_continues() {
        let mut inventory = BackendInventory::new(true);
        let mut emitter = emitter();
        let first = inventory.run(&mut emitter, "failed", |emitter| {
            emitter.raw("discarded:");
            Err(CodegenIrError::unsupported("gap"))
        });
        let second = inventory.run(&mut emitter, "kept", |emitter| {
            emitter.raw("kept:");
            Ok(())
        });
        assert!(first.is_ok());
        assert!(second.is_ok());
        let output = emitter.output();
        assert!(!output.contains("discarded:"));
        assert!(output.contains("kept:"));
    }

    /// Verifies a refused body cannot leave a reusable Reflection label without assembly.
    #[test]
    fn enabled_inventory_rolls_back_reflection_materializer_reservations() {
        let mut inventory = BackendInventory::new(true);
        let mut emitter = emitter();
        let module = Module::new(Target::new(Platform::Linux, Arch::AArch64));
        let mut shared = SharedCodegenState::for_module(&module);
        let key = ReflectionMaterializerKey::FullClass {
            owner: ReflectionOwnerKind::Class,
            name: "RolledBack".to_string(),
        };

        let failed_key = key.clone();
        let failed = inventory.run_with_shared(
            &mut emitter,
            &mut shared,
            "failed",
            |emitter, shared| {
                emitter.raw("discarded:");
                shared
                    .reflection
                    .reserve_materializer(failed_key, "discarded_label".to_string());
                Err(CodegenIrError::unsupported("gap"))
            },
        );

        assert!(failed.is_ok());
        assert_eq!(emitter.checkpoint(), 0);
        assert!(shared.reflection.materializer_label(&key).is_none());

        let kept_key = key.clone();
        let kept = inventory.run_with_shared(
            &mut emitter,
            &mut shared,
            "kept",
            |emitter, shared| {
                emitter.raw("kept:");
                shared
                    .reflection
                    .reserve_materializer(kept_key, "kept_label".to_string());
                Ok(())
            },
        );

        assert!(kept.is_ok());
        assert!(emitter.output().contains("kept:"));
        assert_eq!(
            shared.reflection.materializer_label(&key).as_deref(),
            Some("kept_label")
        );
    }

    /// Verifies identical causes are grouped while per-body details remain present.
    #[test]
    fn enabled_inventory_groups_equivalent_causes() {
        let mut inventory = BackendInventory::new(true);
        let mut emitter = emitter();
        for body in ["a", "b"] {
            let _ = inventory.run(&mut emitter, body, |_| {
                Err(CodegenIrError::unsupported(format!("gap in '{}'", body)))
            });
        }
        let report = inventory.report();
        assert_eq!(inventory.causes().len(), 1, "{report}");
        assert!(report.contains("2 of 2 scanned bodies"), "{report}");
        assert!(report.contains("a"), "{report}");
        assert!(report.contains("b"), "{report}");
    }
}

#[cfg(test)]
mod duplicate_helper_tests {
    use super::drop_duplicate_helpers;
    use crate::fast_hash::FastSet as HashSet;

    /// A stub already emitted elsewhere goes, and the body written behind it stays: the emitter
    /// counts that body as written, so the next stub of the same content jumps to it.
    #[test]
    fn a_dropped_stub_leaves_its_shared_body_behind() {
        let text = "a\n; @helper key=stub\nb stub\n; @helper key=_eir_cbody_1\nbody\n; @endhelper\n; @endhelper\nz\n";
        let mut seen: HashSet<String> = ["stub".to_string()].into_iter().collect();
        let kept = drop_duplicate_helpers(text, &mut seen).unwrap();
        assert_eq!(kept, "a\n; @helper key=_eir_cbody_1\nbody\n; @endhelper\nz\n");
        assert!(seen.contains("_eir_cbody_1"));
    }

    /// A body some earlier region already holds goes with its stub.
    #[test]
    fn a_body_already_kept_is_not_kept_twice() {
        let text = "; @helper key=stub\n; @helper key=_eir_cbody_1\nbody\n; @endhelper\n; @endhelper\n";
        let mut seen: HashSet<String> =
            ["stub".to_string(), "_eir_cbody_1".to_string()].into_iter().collect();
        assert_eq!(drop_duplicate_helpers(text, &mut seen).unwrap(), "");
    }
}
