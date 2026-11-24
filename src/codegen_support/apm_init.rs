//! Purpose:
//! The transaction-monitoring start-up a build carries under `--with-apm`:
//! the call that maps the counters and the I/O slots that count queries and
//! network calls per transaction, shared by an executable's main and a
//! library's `elephc_init`.
//!
//! Called from:
//! - `crate::codegen::frame::emit_probe_init` (main's prologue).
//! - `crate::codegen_support::cdylib::lifecycle` (`elephc_init`).
//!
//! Key details:
//! - No sampler here: nothing a host application would not expect of a
//!   library — no signal handler, no profiling timer.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;

/// The NUL-terminated `--apm-collector` address a `--with-apm` build embeds
/// (C name, mangled per platform by `extern_symbol`).
pub(crate) const APM_COLLECTOR_SYMBOL: &str = "elephc_probe_collector_address";

/// The NUL-terminated start-up text a probe build embeds, one item per line: `+pattern` and
/// `-pattern` for `[apm] include` / `exclude`, `@<unix seconds> <label>` for the build's
/// identity (C name, mangled per platform by `extern_symbol`).
pub(crate) const APM_RULES_SYMBOL: &str = "elephc_probe_apm_rules_text";

/// Publishes the embedded rule text in the probe's `elephc_probe_apm_rules` slot, before
/// any transaction can start: the probe reads it once, on the first label it matches.
pub(crate) fn emit_apm_rules_slot(emitter: &mut Emitter) {
    let target = emitter.target;
    emitter.comment("apm: the project's include/exclude rules ([apm] in elephc.toml)");
    let scratch = abi::int_arg_reg_name(target, 0);
    abi::emit_symbol_address(emitter, scratch, &target.extern_symbol(APM_RULES_SYMBOL));
    abi::emit_store_reg_to_symbol(emitter, scratch, &target.extern_symbol("elephc_probe_apm_rules"), 0);
}

/// Publishes the address of the runtime's live-heap watermark (`_gc_peak`) in the probe's
/// `elephc_probe_heap_peak` slot: a `--web` request starts it over, so when the request is
/// recorded it holds that request's peak memory.
pub(crate) fn emit_heap_peak_slot(emitter: &mut Emitter) {
    let target = emitter.target;
    emitter.comment("apm: where the probe reads a request's peak memory");
    let scratch = abi::int_arg_reg_name(target, 0);
    abi::emit_symbol_address(emitter, scratch, "_gc_peak");
    abi::emit_store_reg_to_symbol(emitter, scratch, &target.extern_symbol("elephc_probe_heap_peak"), 0);
}

/// Emits the `--with-apm` initialization call: hands `elephc_probe_apm_init`
/// the build key and the embedded collector address (or null), which maps
/// the counters and starts sending them when there is a collector. `enable`
/// starts counting at once: a library's, whose build option is its only
/// switch; an executable's counting starts with its `--apm` or collector.
pub(crate) fn emit_apm_init_call(emitter: &mut Emitter, collector: bool, enable: bool) {
    let target = emitter.target;
    emitter.comment("apm: map the transaction counters (--with-apm)");
    let key_arg = abi::int_arg_reg_name(target, 0);
    let collector_arg = abi::int_arg_reg_name(target, 1);
    let enable_arg = abi::int_arg_reg_name(target, 2);
    abi::emit_load_int_immediate(emitter, enable_arg, i64::from(enable));
    abi::emit_symbol_address(emitter, key_arg, &target.extern_symbol("elephc_probe_build_key"));
    if collector {
        abi::emit_symbol_address(emitter, collector_arg, &target.extern_symbol(APM_COLLECTOR_SYMBOL));
    } else {
        abi::emit_load_int_immediate(emitter, collector_arg, 0);
    }
    abi::emit_call_label(emitter, &target.extern_symbol("elephc_probe_apm_init"));
}

/// Publishes whether the bridges time their database and network calls:
/// yes from boot under `--apm`, so a transaction's queries are counted
/// whether or not a profile is being taken.
pub(crate) fn emit_event_active_slot(emitter: &mut Emitter) {
    let target = emitter.target;
    let scratch = abi::int_arg_reg_name(target, 0);
    abi::emit_symbol_address(emitter, scratch, &target.extern_symbol("elephc_probe_event_active"));
    abi::emit_store_reg_to_symbol(emitter, scratch, &target.extern_symbol("elephc_monitor_event_active_fn"), 0);
}

/// Claims the I/O event slots the PDO and network bridges call through, so
/// a probe build counts exact database queries and waits, and network calls
/// and waits, per route or transaction. Those events are not sampled — a
/// driver call fires exactly one — and they cost an atomic increment paid
/// only when one happens, so unlike per-call instrumentation this is
/// affordable in production.
///
/// Not under `--with-monitoring`'s instrumentation, which wins when both are
/// linked: it attributes the same events per FUNCTION through its shadow
/// stack, which strictly refines per-route, and forwards each one to the
/// probe itself. Two writers to one slot would otherwise leave the winner to
/// emission order.
pub(crate) fn emit_probe_io_slots(emitter: &mut Emitter) {
    let target = emitter.target;
    let scratch = abi::int_arg_reg_name(target, 0);
    for (entry, slot) in [
        ("elephc_probe_note_io", "elephc_instr_io_fn"),
        ("elephc_probe_note_wait", "elephc_instr_wait_fn"),
        ("elephc_probe_note_network", "elephc_instr_network_fn"),
        ("elephc_probe_note_network_wait", "elephc_instr_network_wait_fn"),
        // A web request's W3C trace context: read from its `traceparent`, or started,
        // and handed to curl for the next hop (`elephc_probe_trace_begin`).
        ("elephc_probe_trace_begin", "elephc_instr_trace_fn"),
    ] {
        abi::emit_symbol_address(emitter, scratch, &target.extern_symbol(entry));
        abi::emit_store_reg_to_symbol(emitter, scratch, &target.extern_symbol(slot), 0);
    }
    // Where the probe says a trace context is published: the word curl checks before it
    // sends a `traceparent`, without turning on the exact profiler's capture.
    abi::emit_symbol_address(emitter, scratch, &target.extern_symbol("elephc_trace_active"));
    abi::emit_store_reg_to_symbol(emitter, scratch, &target.extern_symbol("elephc_probe_trace_active"), 0);
}
