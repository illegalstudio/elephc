//! Purpose:
//! Build-memory diagnostics behind `ELEPHC_MEM_STATS=1` (`stop` also ends `--emit-ir` before the
//! textual EIR is printed): the process footprint at each phase end,
//! and the byte size of the structures that make up the footprint (AST, checker tables, EIR,
//! class tables, data pool). Everything is inert unless the variable is set.
//!
//! Called from:
//! - `crate::timings::CompileTimings::record_since` (a footprint line per phase).
//! - `crate::pipeline` (the structure reports after the check, after lowering, after EIR
//!   optimization).
//!
//! Key details:
//! - Footprint is `phys_footprint` (macOS `task_info` word 18) with its high-water mark (word 21),
//!   `VmRSS`/`VmHWM` on Linux. macOS keeps freed heap in the footprint, so the peak is the
//!   high-water mark of live heap plus allocator slack.
//! - EIR sizes are exact: a traversal of the function-local tables counting `capacity *
//!   size_of::<T>()` plus every owned heap allocation (operand vectors, `PhpType` strings and
//!   boxes, local and block names).
//! - Sizes of types that are not walked structurally (AST, checker tables, class tables) are the
//!   net bytes a `clone()` allocates, which needs the counting allocator: build with
//!   `--features mem-stats`. Without it those rows read `n/a` and only the footprint and the
//!   exact EIR rows are printed. The counting allocator also tracks the live heap and its
//!   per-phase high-water mark, so `footprint - live` is the allocator slack (freed memory the
//!   process still holds).
//! - The structure rows are measured by cloning, so they raise the peak: read the per-phase
//!   footprint lines, which are printed before any clone, for the true figures.

use crate::ir::{Function, Instruction, Module, Value};
use crate::types::PhpType;
use std::sync::OnceLock;

/// Whether `ELEPHC_MEM_STATS` asks for the diagnostic output.
pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("ELEPHC_MEM_STATS").map(|value| !value.is_empty() && value != "0").unwrap_or(false)
    })
}

/// Whether `ELEPHC_MEM_STATS=stop` asks `--emit-ir` to end after the EIR optimization report,
/// before the textual EIR (itself a large allocation) is built.
pub(crate) fn stop_before_print() -> bool {
    std::env::var("ELEPHC_MEM_STATS").map(|value| value == "stop").unwrap_or(false)
}

/// The counting allocator that gives the diagnostic its exact live-heap figures; only compiled
/// with `--features mem-stats`.
#[cfg(feature = "mem-stats")]
pub mod counting {
    use std::alloc::{GlobalAlloc, Layout};
    use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};

    /// Bytes currently allocated through the global allocator.
    static LIVE: AtomicIsize = AtomicIsize::new(0);
    /// Highest `LIVE` since the last [`reset_peak`].
    static PEAK: AtomicIsize = AtomicIsize::new(0);

    /// Global allocator wrapper that counts live bytes around the real allocator.
    pub struct CountingAlloc(pub mimalloc::MiMalloc);

    /// Records `delta` bytes more (or fewer) live and raises the high-water mark.
    #[inline]
    fn adjust(delta: isize) {
        let now = LIVE.fetch_add(delta, Relaxed) + delta;
        if delta > 0 {
            PEAK.fetch_max(now, Relaxed);
        }
    }

    unsafe impl GlobalAlloc for CountingAlloc {
        /// Allocates through mimalloc and counts the bytes.
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { self.0.alloc(layout) };
            if !ptr.is_null() {
                adjust(layout.size() as isize);
            }
            ptr
        }

        /// Allocates zeroed memory through mimalloc and counts the bytes.
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { self.0.alloc_zeroed(layout) };
            if !ptr.is_null() {
                adjust(layout.size() as isize);
            }
            ptr
        }

        /// Frees through mimalloc and uncounts the bytes.
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            adjust(-(layout.size() as isize));
            unsafe { self.0.dealloc(ptr, layout) }
        }

        /// Resizes through mimalloc and counts the difference.
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let new = unsafe { self.0.realloc(ptr, layout, new_size) };
            if !new.is_null() {
                adjust(new_size as isize - layout.size() as isize);
            }
            new
        }
    }

    /// Live heap bytes right now.
    pub fn live() -> usize {
        LIVE.load(Relaxed).max(0) as usize
    }

    /// Live-heap high-water mark since the last reset.
    pub fn peak() -> usize {
        PEAK.load(Relaxed).max(0) as usize
    }

    /// Restarts the high-water mark at the current live size.
    pub fn reset_peak() {
        PEAK.store(LIVE.load(Relaxed), Relaxed);
    }
}

/// The process's `(current, peak)` physical footprint in bytes, when the platform reports one.
pub(crate) fn footprint() -> Option<(u64, u64)> {
    #[cfg(target_os = "macos")]
    {
        const TASK_VM_INFO: libc::task_flavor_t = 22;
        const CURRENT_WORD: usize = 18;
        const PEAK_WORD: usize = 21;
        let mut info = [0u64; 64];
        let mut count = (info.len() * 2) as libc::mach_msg_type_number_t;
        #[allow(deprecated)]
        let status = unsafe {
            libc::task_info(libc::mach_task_self(), TASK_VM_INFO, info.as_mut_ptr().cast(), &mut count)
        };
        if status != 0 || (count as usize) < (PEAK_WORD + 1) * 2 {
            return None;
        }
        Some((info[CURRENT_WORD], info[PEAK_WORD]))
    }
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let read = |key: &str| -> Option<u64> {
            let line = status.lines().find(|line| line.starts_with(key))?;
            Some(line.split_whitespace().nth(1)?.parse::<u64>().ok()? * 1024)
        };
        Some((read("VmRSS:")?, read("VmHWM:")?))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Formats a byte count as MiB with one decimal.
fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1u64 << 20) as f64)
}

/// Prints the footprint line for one phase end (or any named point) when the diagnostic is on.
///
/// With the counting allocator the line also carries the live heap, its high-water mark since the
/// previous line, and the slack (footprint minus live), then restarts the high-water mark.
pub(crate) fn mark(label: &str) {
    if !enabled() {
        return;
    }
    let (current, peak) = footprint().unwrap_or((0, 0));
    #[cfg(feature = "mem-stats")]
    {
        let live = counting::live() as u64;
        eprintln!(
            "[mem] {label:<34} footprint {:>12} peak {:>12} | live {:>12} live-peak {:>12} slack {:>12}",
            mib(current),
            mib(peak),
            mib(live),
            mib(counting::peak() as u64),
            mib(current.saturating_sub(live)),
        );
        counting::reset_peak();
    }
    #[cfg(not(feature = "mem-stats"))]
    eprintln!("[mem] {label:<34} footprint {:>12} peak {:>12}", mib(current), mib(peak));
}

/// Net heap bytes a `clone()` of `value` allocates, or `None` without the counting allocator.
///
/// The clone is dropped before returning. Vector capacity slack is not counted (a clone is
/// tight), so this is a lower bound on the original's heap size.
#[allow(unused_variables)]
pub(crate) fn clone_bytes<T: Clone>(value: &T) -> Option<u64> {
    #[cfg(feature = "mem-stats")]
    {
        let before = counting::live();
        let copy = value.clone();
        let after = counting::live();
        drop(copy);
        Some(after.saturating_sub(before) as u64)
    }
    #[cfg(not(feature = "mem-stats"))]
    {
        None
    }
}

/// Footprint and live heap at the start of one lowered body.
pub(crate) struct BodyMark {
    name: String,
    footprint: u64,
    live: usize,
}

/// Live heap bytes (counting allocator) or 0.
fn live_bytes() -> usize {
    #[cfg(feature = "mem-stats")]
    {
        counting::live()
    }
    #[cfg(not(feature = "mem-stats"))]
    {
        0
    }
}

/// Starts the per-body memory delta for `name` when the diagnostic is on.
pub(crate) fn body_start(name: &str) -> Option<BodyMark> {
    if !enabled() {
        return None;
    }
    Some(BodyMark { name: name.to_string(), footprint: footprint().map_or(0, |f| f.0), live: live_bytes() })
}

/// Prints one `[mem-body]` line for a body whose footprint or live heap grew by at least
/// `ELEPHC_MEM_STATS_BODY_KIB` (default 4096) KiB. A closure counts in its enclosing body.
pub(crate) fn body_end(mark: BodyMark) {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    let limit = *LIMIT.get_or_init(|| {
        std::env::var("ELEPHC_MEM_STATS_BODY_KIB").ok().and_then(|v| v.parse().ok()).unwrap_or(4096)
    }) * 1024;
    let footprint = footprint().map_or(0, |f| f.0).saturating_sub(mark.footprint);
    let live = live_bytes().saturating_sub(mark.live) as u64;
    if footprint >= limit || live >= limit {
        eprintln!("[mem-body] footprint +{:>9} live +{:>9} {}", mib(footprint), mib(live), mark.name);
    }
}

/// Formats an optional byte count for a report row.
fn fmt_opt(bytes: Option<u64>) -> String {
    bytes.map(mib).unwrap_or_else(|| "n/a".to_string())
}

/// Prints one structure row: its label, entry count and clone-measured heap size.
fn row(label: &str, count: usize, bytes: Option<u64>) {
    let per = match bytes {
        Some(bytes) if count > 0 => format!("{:>9} B/entry", bytes / count as u64),
        _ => String::new(),
    };
    eprintln!("[mem]   {label:<40} {count:>10} entries {:>14} {per}", fmt_opt(bytes));
}

/// Measures and prints one named table.
macro_rules! table {
    ($label:expr, $value:expr, $count:expr) => {
        row($label, $count, clone_bytes(&$value))
    };
}

/// Heap bytes a `PhpType` owns beyond its own inline `size_of::<PhpType>()`.
pub(crate) fn php_type_heap(ty: &PhpType) -> usize {
    let node = std::mem::size_of::<PhpType>();
    match ty {
        PhpType::Array(inner) | PhpType::Buffer(inner) => node + php_type_heap(inner),
        PhpType::AssocArray { key, value } => 2 * node + php_type_heap(key) + php_type_heap(value),
        PhpType::Object(name) | PhpType::Packed(name) => name.capacity(),
        PhpType::Pointer(name) | PhpType::Resource(name) => name.as_ref().map_or(0, String::capacity),
        PhpType::Union(members) => {
            members.capacity() * node + members.iter().map(php_type_heap).sum::<usize>()
        }
        _ => 0,
    }
}

/// Byte tallies of the exact EIR traversal.
#[derive(Default)]
struct EirTally {
    functions: usize,
    instructions: usize,
    instruction_inline: usize,
    operand_vecs: usize,
    operand_slots: usize,
    operand_heap: usize,
    immediate_heap: usize,
    result_type_heap: usize,
    result_type_heap_count: usize,
    values: usize,
    value_inline: usize,
    value_type_heap: usize,
    locals: usize,
    local_inline: usize,
    local_heap: usize,
    blocks: usize,
    block_inline: usize,
    block_heap: usize,
    function_inline: usize,
    function_heap: usize,
    spare_instruction_capacity: usize,
    spare_value_capacity: usize,
    max_function_bytes: (usize, String),
    dense_dominator_bytes: u128,
    max_blocks: (usize, String),
    signature_clone: u64,
    signature_count: usize,
    other_decl_clone: u64,
}

impl EirTally {
    /// Adds one function's tables to the tally.
    fn add(&mut self, function: &Function) {
        use crate::ir::Immediate;
        let before = self.total();
        self.functions += 1;
        self.function_inline += std::mem::size_of::<Function>();
        self.function_heap += function.name.capacity()
            + function.source_name.as_ref().map_or(0, String::capacity)
            + function.display_name.as_ref().map_or(0, String::capacity)
            + function.source_signature.as_ref().map_or(0, String::capacity)
            + function.lexical_class.as_ref().map_or(0, String::capacity)
            + function.params.capacity() * std::mem::size_of::<crate::ir::FunctionParam>()
            + function.params.iter().map(|p| p.name.capacity() + php_type_heap(&p.php_type)).sum::<usize>()
            + php_type_heap(&function.return_php_type)
            + function.inlined_scopes.values().map(|scope| scope.capacity() + 24).sum::<usize>();
        self.instructions += function.instructions.len();
        self.instruction_inline += function.instructions.capacity() * std::mem::size_of::<Instruction>();
        self.spare_instruction_capacity +=
            (function.instructions.capacity() - function.instructions.len()) * std::mem::size_of::<Instruction>();
        for inst in &function.instructions {
            if inst.operands.capacity() > 0 {
                self.operand_vecs += 1;
                self.operand_slots += inst.operands.len();
                self.operand_heap += inst.operands.capacity() * 4;
            }
            let heap = php_type_heap(&inst.result_php_type);
            if heap > 0 {
                self.result_type_heap_count += 1;
            }
            self.result_type_heap += heap;
            self.immediate_heap += match &inst.immediate {
                Some(Immediate::IncludeFrame(b)) => std::mem::size_of_val(&**b),
                Some(Immediate::ClassLikeActivation(b)) => std::mem::size_of_val(&**b),
                Some(Immediate::UnresolvedSource(b)) => std::mem::size_of_val(&**b),
                _ => 0,
            };
        }
        self.values += function.values.len();
        self.value_inline += function.values.capacity() * std::mem::size_of::<Value>();
        self.spare_value_capacity +=
            (function.values.capacity() - function.values.len()) * std::mem::size_of::<Value>();
        self.value_type_heap += function.values.iter().map(|v| php_type_heap(&v.php_type)).sum::<usize>();
        self.locals += function.locals.len();
        self.local_inline += function.locals.capacity() * std::mem::size_of::<crate::ir::LocalSlot>();
        self.local_heap += function
            .locals
            .iter()
            .map(|l| l.name.as_ref().map_or(0, String::capacity) + php_type_heap(&l.php_type))
            .sum::<usize>();
        self.blocks += function.blocks.len();
        self.dense_dominator_bytes += (function.blocks.len() as u128).pow(2) / 8;
        if function.blocks.len() > self.max_blocks.0 {
            self.max_blocks = (function.blocks.len(), function.name.clone());
        }
        self.block_inline += function.blocks.capacity() * std::mem::size_of::<crate::ir::BasicBlock>();
        self.block_heap += function
            .blocks
            .iter()
            .map(|b| {
                b.name.capacity()
                    + b.params.capacity() * 4
                    + b.instructions.capacity() * 4
                    + b.terminator.as_ref().map_or(0, |t| t.used_values().len() * 4 + 16)
            })
            .sum::<usize>();
        if let Some(bytes) = clone_bytes(&function.signature) {
            self.signature_clone += bytes;
            self.signature_count += usize::from(function.signature.is_some());
        }
        if let Some(bytes) = clone_bytes(&(&function.source_defaults, &function.attribute_names, &function.attribute_args, &function.callable_default, &function.generator_source)) {
            self.other_decl_clone += bytes;
        }
        let function_bytes = self.total() - before;
        if function_bytes > self.max_function_bytes.0 {
            self.max_function_bytes = (function_bytes, function.name.clone());
        }
    }

    /// Sum of every counted byte.
    fn total(&self) -> usize {
        self.instruction_inline
            + self.operand_heap
            + self.immediate_heap
            + self.result_type_heap
            + self.value_inline
            + self.value_type_heap
            + self.local_inline
            + self.local_heap
            + self.block_inline
            + self.block_heap
            + self.function_inline
            + self.function_heap
    }

    /// Prints the tally as report rows.
    fn print(&self, label: &str) {
        let line = |name: &str, bytes: usize, note: String| {
            eprintln!("[mem]   {name:<40} {:>14} {note}", mib(bytes as u64));
        };
        eprintln!(
            "[mem] EIR {label}: {} functions, {} instructions, {} values, {} locals, {} blocks, total {}",
            self.functions,
            self.instructions,
            self.values,
            self.locals,
            self.blocks,
            mib(self.total() as u64)
        );
        let per_inst = |bytes: usize| format!("{:.1} B/instruction", bytes as f64 / self.instructions.max(1) as f64);
        line("instructions (inline)", self.instruction_inline, per_inst(self.instruction_inline));
        line(
            "  of which spare Vec capacity",
            self.spare_instruction_capacity,
            per_inst(self.spare_instruction_capacity),
        );
        line(
            "operand Vecs (heap)",
            self.operand_heap,
            format!("{} vecs, {} operands, {}", self.operand_vecs, self.operand_slots, per_inst(self.operand_heap)),
        );
        line("immediates (boxed)", self.immediate_heap, per_inst(self.immediate_heap));
        line(
            "instruction result PhpType (heap)",
            self.result_type_heap,
            format!("{} types with heap, {}", self.result_type_heap_count, per_inst(self.result_type_heap)),
        );
        line("values (inline)", self.value_inline, format!("{:.1} B/value", self.value_inline as f64 / self.values.max(1) as f64));
        line("  of which spare Vec capacity", self.spare_value_capacity, String::new());
        line("value PhpType (heap)", self.value_type_heap, String::new());
        line("locals (inline)", self.local_inline, String::new());
        line("local names + PhpType (heap)", self.local_heap, String::new());
        line("blocks (inline)", self.block_inline, String::new());
        line("block names, ids, terminators (heap)", self.block_heap, String::new());
        line("function headers (inline)", self.function_inline, String::new());
        line("function names, params, scopes (heap)", self.function_heap, String::new());
        line(
            "function signatures (clone-measured)",
            self.signature_clone as usize,
            format!("{} signatures", self.signature_count),
        );
        line("defaults/attributes/generator (clone)", self.other_decl_clone as usize, String::new());
        eprintln!(
            "[mem]   most blocks: {} in {}; per-block dominator bitsets would need {} for the whole module, {} for that body alone",
            self.max_blocks.1,
            self.max_blocks.0,
            mib((self.dense_dominator_bytes.min(u64::MAX as u128)) as u64),
            mib(((self.max_blocks.0 as u128).pow(2) / 8) as u64),
        );
        eprintln!(
            "[mem]   largest function {} ({})",
            self.max_function_bytes.1,
            mib(self.max_function_bytes.0 as u64)
        );
        eprintln!(
            "[mem]   inline sizes: Instruction {} Value {} PhpType {} Span {} Op {} Immediate {} LocalSlot {} BasicBlock {}",
            std::mem::size_of::<Instruction>(),
            std::mem::size_of::<Value>(),
            std::mem::size_of::<PhpType>(),
            std::mem::size_of::<crate::span::Span>(),
            std::mem::size_of::<crate::ir::Op>(),
            std::mem::size_of::<crate::ir::Immediate>(),
            std::mem::size_of::<crate::ir::LocalSlot>(),
            std::mem::size_of::<crate::ir::BasicBlock>(),
        );
    }
}

/// Reports the exact size of an EIR module's function tables and the clone-measured size of the
/// rest of the module (class tables, data pool, declaration tables).
pub(crate) fn report_module(label: &str, module: &Module) {
    if !enabled() {
        return;
    }
    mark(&format!("before module report: {label}"));
    let mut tally = EirTally::default();
    let groups: [(&str, &Vec<Function>); 8] = [
        ("functions", &module.functions),
        ("class_methods", &module.class_methods),
        ("closures", &module.closures),
        ("fiber_wrappers", &module.fiber_wrappers),
        ("callback_wrappers", &module.callback_wrappers),
        ("extern_callback_trampolines", &module.extern_callback_trampolines),
        ("runtime_callable_invokers", &module.runtime_callable_invokers),
        ("(none)", &Vec::new()),
    ];
    for (_, group) in &groups {
        for function in group.iter() {
            tally.add(function);
        }
    }
    tally.print(label);
    for (name, group) in &groups[..7] {
        let instructions: usize = group.iter().map(|f| f.instructions.len()).sum();
        eprintln!("[mem]   group {name:<28} {:>7} functions {:>10} instructions", group.len(), instructions);
    }
    eprintln!("[mem] module tables (clone-measured) at {label}:");
    table!("class_infos", module.class_infos, module.class_infos.len());
    table!("interface_infos", module.interface_infos, module.interface_infos.len());
    table!("enum_infos", module.enum_infos, module.enum_infos.len());
    table!("class_table", module.class_table, 1);
    table!("enum_table", module.enum_table, 1);
    table!("interface_table", module.interface_table, 1);
    table!("trait_table", module.trait_table, 1);
    table!("data pool", module.data, module.data.strings.len());
    table!("callable_param_sigs", module.callable_param_sigs, module.callable_param_sigs.len());
    table!("global_constants", module.global_constants, module.global_constants.len());
    table!("declared_trait_methods", module.declared_trait_methods, module.declared_trait_methods.len());
    table!("declared_trait_properties", module.declared_trait_properties, module.declared_trait_properties.len());
    table!("declared_trait_constants", module.declared_trait_constants, module.declared_trait_constants.len());
    table!("declared_class_source_files", module.declared_class_source_files, module.declared_class_source_files.len());
    table!("source_paths + source_files", (&module.source_paths, &module.source_files), module.source_paths.len());
    if module.source_catalog().is_some() {
        eprintln!("[mem]   source catalog present (texts counted by the AST/source rows, not walked)");
    }
    mark(&format!("after module report: {label}"));
}

/// Reports the clone-measured size of the AST at `label`.
pub(crate) fn report_ast(label: &str, program: &[crate::parser::ast::Stmt]) {
    if !enabled() {
        return;
    }
    let bytes = clone_bytes(&program.to_vec());
    eprintln!("[mem] AST {label}: {} top-level statements, {}", program.len(), fmt_opt(bytes));
}

/// Reports the clone-measured size of each checker table at `label`.
pub(crate) fn report_check(label: &str, check: &crate::types::CheckResult) {
    if !enabled() {
        return;
    }
    eprintln!("[mem] checker tables (clone-measured) at {label}:");
    table!("classes", check.classes, check.classes.len());
    table!("interfaces", check.interfaces, check.interfaces.len());
    table!("enums", check.enums, check.enums.len());
    table!("packed_classes", check.packed_classes, check.packed_classes.len());
    table!("functions (FunctionSig)", check.functions, check.functions.len());
    table!("callable_param_sigs", check.callable_param_sigs, check.callable_param_sigs.len());
    table!("callable_return_sigs", check.callable_return_sigs, check.callable_return_sigs.len());
    table!("callable_array_return_sigs", check.callable_array_return_sigs, check.callable_array_return_sigs.len());
    table!("global_env", check.global_env, 1);
    table!("function_attribute_names", check.function_attribute_names, check.function_attribute_names.len());
    table!("function_attribute_args", check.function_attribute_args, check.function_attribute_args.len());
    table!("builtin_call_types", check.builtin_call_types, check.builtin_call_types.len());
    table!("throw_access_sites", check.throw_access_sites, check.throw_access_sites.len());
    table!("closure_param_hints", check.closure_param_hints, check.closure_param_hints.len());
    table!("array_push_property_plans", check.array_push_property_plans, check.array_push_property_plans.len());
    table!("member_receiver_types", check.member_receiver_types, check.member_receiver_types.len());
    table!("flow_typed_property_accesses", check.flow_typed_property_accesses, check.flow_typed_property_accesses.len());
    table!("array_append_element_types", check.array_append_element_types, check.array_append_element_types.len());
    table!("local_value_read_types", check.local_value_read_types, 1);
    table!("loop_storage_types", check.loop_storage_types, 1);
    table!("boxed_reference_promotion_sites", check.boxed_reference_promotion_sites, check.boxed_reference_promotion_sites.len());
    table!("local_bind_kill_sites", check.local_bind_kill_sites, check.local_bind_kill_sites.len());
    table!("local_retype_sites", check.local_retype_sites, check.local_retype_sites.len());
    table!("mixed_storage_store_sites", check.mixed_storage_store_sites, check.mixed_storage_store_sites.len());
    table!("sparse_key_write_sites", check.sparse_key_write_sites, check.sparse_key_write_sites.len());
    table!("generic_call_sites", check.generic_call_sites, check.generic_call_sites.len());
    table!("generic_method_sites", check.generic_method_sites, check.generic_method_sites.len());
    table!("doc_return_sites", check.doc_return_sites, check.doc_return_sites.len());
    table!("promotable_arrays", check.promotable_arrays, 1);
    table!("return_alias_summaries", check.return_alias_summaries, 1);
    table!("class_binding", check.class_binding, 1);
    table!("trait_collision_defaults", check.trait_collision_defaults, check.trait_collision_defaults.len());
    table!("runtime_define_values", check.runtime_define_values, check.runtime_define_values.len());
    table!("buffer_read_sites", check.buffer_read_sites, check.buffer_read_sites.len());
    table!("warnings", check.warnings, check.warnings.len());
}
