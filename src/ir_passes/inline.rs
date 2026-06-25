//! Purpose:
//! Small-function inliner (module-level EIR pass). Inlines `Call` and
//! `FunctionVariantCall` sites targeting user functions whose body is <=24
//! non-Nop instructions, is non-recursive (directly or mutually), has no
//! exception handler ops, is not a generator or fiber wrapper, and exposes a
//! provably ownership-safe destructor-free boundary/body.
//!
//! Called from:
//! - `crate::ir_passes::optimize_module` (gated by --ir-opt at call sites).
//!
//! Key details:
//! - Pure transform on &mut Module returning a change flag; clones only small
//!   callees; splices blocks at the call site using a continuation block param
//!   for the return join.
//! - Candidate callees are `module.functions` entries only (no methods,
//!   closures, builtins or externs); hosts also include the other body tables.
//! - Recursion (direct or mutual) is excluded up front via a call-graph cycle
//!   analysis, and a per-function fuel cap backstops termination. Call-site name
//!   resolution uses snapshots taken before mutation, so the rewrite loop never
//!   aliases `&Module` with the `&mut Function` it mutates.
//! - Callees are restricted to a destructor-free boundary/body (scalars, strings,
//!   and arrays/unions of destructor-free types; no by-ref/variadic params and no
//!   ref-cell/static/global/capture locals). The splice replaces `Return` with
//!   `Br`, bypassing the callee's implicit epilogue cleanup, so correctness is
//!   preserved two ways: (1) `transplant_callee_body` reproduces the callee's
//!   per-slot cleanup decisions — parameter and directly-returned slots become
//!   `HiddenTemp` (epilogue-excluded, as the callee excludes them), ordinary
//!   refcounted internal locals stay `PhpLocal` so the host epilogue still frees
//!   them; (2) the destructor-free restriction makes the only residual difference
//!   — deferring those frees to the host epilogue — unobservable (no `__destruct`,
//!   no object identity). Objects/closures/resources/`mixed`/`iterable` and by-ref
//!   params are excluded because their cleanup timing or aliasing cannot be
//!   reproduced by a value-copy splice.
//! - String arguments add one more call-site condition: PHP concatenation builds
//!   intermediates in a frame-relative scratch buffer that every function rewinds
//!   at statement boundaries (`ConcatReset`). A real call's separate frame protects
//!   the caller's in-flight scratch values, but the spliced body runs the callee's
//!   `ConcatReset` in the host frame, which would free an in-flight scratch string
//!   argument before the body reads it. So a site is only inlined when every `Str`
//!   argument comes from a provably non-scratch source (`const_str`/`load_local`);
//!   see `call_string_args_are_stable`.
//! - Returns of the callee become `Br` to a continuation block carrying the
//!   result via a param when the call produced a value; the original call is
//!   neutralized and parked in an unreachable block to keep value-def records
//!   consistent. IR stays validator-clean (the driver re-validates in debug).

use std::collections::{HashMap, HashSet};

use crate::ir::{
    collect_dispatch_groups, BasicBlock, BlockId, Function, FunctionVariantLabel, Immediate,
    InstId, Instruction, IrType, LocalKind, LocalSlotId, Module, Op, Ownership, Terminator, Value,
    ValueDef, ValueId,
};
use crate::ir_passes::cfg::has_exception_handlers;
use crate::ir_passes::rewrite::neutralize_to_nop;
use crate::types::PhpType; // used for void stores and plain-scalar checks

/// Backstop cap on inlines performed into a single host. Recursive-cycle
/// exclusion already guarantees termination over the acyclic candidate graph;
/// this only bounds pathological code-size blowup and protects against bugs.
const MAX_INLINES_PER_FUNCTION: usize = 10_000;

/// Returns true if `n` is a direct user-function call opcode we consider for inlining.
fn is_user_call_op(op: Op) -> bool {
    matches!(op, Op::Call | Op::FunctionVariantCall)
}

/// Count non-Nop instructions for the size threshold.
fn count_non_nop_instructions(func: &Function) -> usize {
    func.instructions.iter().filter(|i| i.op != Op::Nop).count()
}

/// Resolves a call-site instruction's PHP callee name without borrowing the whole
/// `Module`, using snapshots taken before any host mutation. This keeps the inlining
/// loop free of `&Module`/`&mut Function` aliasing.
struct CallTargetResolver {
    /// Snapshot of `module.data.function_names`, indexed by `Immediate::Data`.
    function_names: Vec<String>,
    /// Snapshot of the include-variant dispatch groups, indexed by `FunctionVariantRef`.
    groups: Vec<FunctionVariantLabel>,
}

impl CallTargetResolver {
    /// Builds the resolver from an immutable borrow of the module, taken once up front.
    fn new(module: &Module) -> Self {
        CallTargetResolver {
            function_names: module.data.function_names.clone(),
            groups: collect_dispatch_groups(module),
        }
    }

    /// Resolves the callee PHP name for a `Call`/`FunctionVariantCall` instruction,
    /// mirroring `ir::function_variants::resolve_variant_callee_name` over the snapshots.
    fn resolve(&self, inst: &Instruction) -> Option<String> {
        if !is_user_call_op(inst.op) {
            return None;
        }
        match &inst.immediate {
            Some(Immediate::Data(did)) => self.function_names.get(did.as_raw() as usize).cloned(),
            Some(Immediate::FunctionVariantRef { group, variant }) => {
                let group = self.groups.get(*group as usize)?;
                group
                    .variants
                    .get(*variant as usize)
                    .cloned()
                    .or_else(|| (group.variants.len() == 1).then(|| group.variants[0].clone()))
            }
            _ => None,
        }
    }
}

/// Returns true when releasing a value of this PHP type can never run user-observable
/// code (it has no `__destruct` and transitively contains no object/closure/resource).
/// Freeing such a value is pure memory management, so the inliner may let the host
/// epilogue free an inlined callee's owned locals at a (possibly later) point without
/// changing observable behavior. Objects, packed classes, closures, resources, generic
/// `iterable`/`mixed`, and buffers are conservatively excluded; arrays/unions are safe
/// only when their element/member types are themselves destructor-free.
fn is_destructor_free(php_type: &PhpType) -> bool {
    match php_type {
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::Void
        | PhpType::Never
        | PhpType::Pointer(_)
        | PhpType::TaggedScalar => true,
        PhpType::Array(element) => is_destructor_free(element),
        PhpType::AssocArray { value, .. } => is_destructor_free(value),
        PhpType::Union(members) => members.iter().all(is_destructor_free),
        PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Resource(_) => false,
    }
}

/// Returns true when the callee is safe to splice into a caller. The splice replaces the
/// callee's `return` with a `br`, bypassing the callee's implicit function-epilogue
/// cleanup; correctness is preserved by two things together:
///
/// 1. Every parameter, the return, and every local slot is **destructor-free**, so the
///    only behavioural difference — the *timing* of freeing the callee's owned internal
///    locals, which is deferred to the host epilogue — is unobservable (pure memory
///    management, no `__destruct`, no object identity).
/// 2. `transplant_callee_body` replicates the callee's per-slot cleanup *decisions*:
///    parameter slots and directly-returned slots (which the callee epilogue excludes,
///    because the argument is borrowed and the return value's ownership is moved out) are
///    transplanted as `HiddenTemp` so the host epilogue ignores them; ordinary refcounted
///    internal locals stay `PhpLocal` so the host epilogue still frees them.
///
/// By-ref/variadic parameters and special-kind locals (ref-cells, statics, globals,
/// captures, iterator/generator state) are excluded because they need aliasing/persistence
/// machinery the value-copy splice does not reproduce.
fn callee_is_inline_safe(callee: &Function) -> bool {
    if !is_destructor_free(&callee.return_php_type) {
        return false;
    }
    for param in &callee.params {
        if param.by_ref || param.variadic {
            return false;
        }
        if !is_destructor_free(&param.php_type) {
            return false;
        }
    }
    for local in &callee.locals {
        if !matches!(
            local.kind,
            LocalKind::PhpLocal | LocalKind::HiddenTemp | LocalKind::NamedArgTemp
        ) {
            return false;
        }
        if !is_destructor_free(&local.php_type) {
            return false;
        }
    }
    true
}

/// Builds the set of candidate function names that are recursive — directly or
/// mutually — over the `module.functions` call graph (edges via resolvable
/// `Call`/`FunctionVariantCall` sites). A function is recursive when it can reach
/// itself; such functions are never inlined, which (with the per-function fuel cap)
/// guarantees the inliner terminates instead of expanding a cycle forever.
fn compute_recursive_functions(
    functions: &[Function],
    name_to_idx: &HashMap<String, usize>,
    resolver: &CallTargetResolver,
) -> HashSet<String> {
    let n = functions.len();
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, func) in functions.iter().enumerate() {
        for inst in &func.instructions {
            if !is_user_call_op(inst.op) {
                continue;
            }
            if let Some(target) = resolver.resolve(inst) {
                if let Some(&j) = name_to_idx.get(&target) {
                    adjacency[i].push(j);
                }
            }
        }
    }
    let mut recursive = HashSet::new();
    for start in 0..n {
        // Depth-first reachability from `start`; if we re-reach it, it lies on a cycle.
        let mut visited = vec![false; n];
        let mut stack: Vec<usize> = adjacency[start].clone();
        let mut reaches_self = false;
        while let Some(node) = stack.pop() {
            if node == start {
                reaches_self = true;
                break;
            }
            if visited[node] {
                continue;
            }
            visited[node] = true;
            stack.extend(adjacency[node].iter().copied());
        }
        if reaches_self {
            recursive.insert(functions[start].name.clone());
        }
    }
    recursive
}

/// Returns `(saw_value, saw_void)` over the callee's `Return` terminators.
fn callee_return_shape(callee: &Function) -> (bool, bool) {
    let mut saw_value = false;
    let mut saw_void = false;
    for block in &callee.blocks {
        if let Some(Terminator::Return { value }) = &block.terminator {
            if value.is_some() {
                saw_value = true;
            } else {
                saw_void = true;
            }
        }
    }
    (saw_value, saw_void)
}

/// Eligibility predicate per acceptance: size threshold, non-recursive (direct or
/// mutual), no try/catch, no generators/fibers, a 0-parameter entry block (EIR
/// convention), and a provably ownership-safe plain-scalar boundary/body so the
/// splice cannot leak refcounted state by skipping the callee epilogue.
fn is_eligible_callee(callee: &Function, recursive: &HashSet<String>) -> bool {
    if callee.flags.is_generator || callee.flags.is_fiber_wrapper {
        return false;
    }
    if has_exception_handlers(callee) {
        return false;
    }
    if count_non_nop_instructions(callee) > 24 {
        return false;
    }
    if recursive.contains(&callee.name) {
        return false;
    }
    if let Some(entry_block) = callee.blocks.get(callee.entry.as_raw() as usize) {
        if !entry_block.params.is_empty() {
            return false;
        }
    }
    if !callee_is_inline_safe(callee) {
        return false;
    }
    true
}

/// Traces a returned value back to the local slot it directly loads from (through
/// `load_local`, or an array/hash→mixed reboxing of one), mirroring codegen's
/// `direct_return_local_slot` so the inliner can identify slots the callee epilogue
/// would have excluded from cleanup (their ownership is moved into the return value).
fn trace_returned_slot(callee: &Function, value: ValueId) -> Option<LocalSlotId> {
    let value = callee.value(value)?;
    let ValueDef::Instruction { inst, .. } = value.def else {
        return None;
    };
    let inst = callee.instruction(inst)?;
    match inst.op {
        Op::LoadLocal => match &inst.immediate {
            Some(Immediate::LocalSlot(slot)) => Some(*slot),
            _ => None,
        },
        Op::ArrayToMixed | Op::HashToMixed => {
            trace_returned_slot(callee, *inst.operands.first()?)
        }
        _ => None,
    }
}

/// Collects the callee local slots that are directly returned by some `Return`
/// terminator. These must not be cleaned up after inlining (the value's ownership is
/// transferred to the caller via the continuation parameter), matching the callee
/// epilogue's directly-returned-slot exclusion.
fn callee_directly_returned_slots(callee: &Function) -> HashSet<LocalSlotId> {
    let mut slots = HashSet::new();
    for block in &callee.blocks {
        if let Some(Terminator::Return { value: Some(value) }) = &block.terminator {
            if let Some(slot) = trace_returned_slot(callee, *value) {
                slots.insert(slot);
            }
        }
    }
    slots
}

/// Returns the callee local slots that hold parameters (by name), which the callee
/// epilogue excludes from cleanup (arguments are borrowed, not owned by the callee).
fn callee_param_slots(callee: &Function) -> HashSet<LocalSlotId> {
    let param_names: HashSet<&str> = callee.params.iter().map(|p| p.name.as_str()).collect();
    callee
        .locals
        .iter()
        .filter(|local| {
            local
                .name
                .as_deref()
                .is_some_and(|name| param_names.contains(name))
        })
        .map(|local| local.id)
        .collect()
}

/// Returns whether the call site's argument operands bind directly to the callee's
/// parameter slots with no coercion: one operand per parameter, each operand's storage
/// type matching its parameter's. This rejects calls whose arguments the callee prologue
/// would coerce (e.g. spread/named arguments materialized as boxed `mixed` values that a
/// typed parameter unboxes) — the inliner's plain `store_local` binding cannot reproduce
/// that coercion, so such sites are left as ordinary calls.
fn call_args_bind_directly(host: &Function, call_inst: &Instruction, callee: &Function) -> bool {
    if call_inst.operands.len() != callee.params.len() {
        return false;
    }
    call_inst
        .operands
        .iter()
        .zip(&callee.params)
        .all(|(operand, param)| {
            host.value(*operand)
                .is_some_and(|value| value.ir_type == param.ir_type)
        })
}

/// Returns whether every `Str`-typed argument is a value that provably lives outside the
/// global string-concat scratch buffer.
///
/// PHP string concatenation builds intermediate results in a frame-relative scratch
/// buffer that every function rewinds at its statement boundaries (`Op::ConcatReset`).
/// In a real call the callee's own frame protects the caller's in-flight scratch values;
/// but the inliner binds the argument with `store_local` and the spliced body then runs
/// the callee's `concat_reset` in the *host* frame, freeing an in-flight scratch string
/// argument before the body reads it back (a miscompile). A `const_str` literal
/// (persistent) and a `load_local` (already persisted into a slot) are the only string
/// sources guaranteed not to be scratch-resident; any other source (e.g. a `str_concat`
/// result passed directly) is conservatively treated as in-flight and the site is left as
/// an ordinary call.
fn call_string_args_are_stable(host: &Function, call_inst: &Instruction, callee: &Function) -> bool {
    for (operand, param) in call_inst.operands.iter().zip(&callee.params) {
        if param.ir_type != IrType::Str {
            continue;
        }
        let stable = host
            .value(*operand)
            .and_then(|value| match value.def {
                ValueDef::Instruction { inst, .. } => host.instruction(inst),
                _ => None,
            })
            .is_some_and(|def| matches!(def.op, Op::ConstStr | Op::LoadLocal));
        if !stable {
            return false;
        }
    }
    true
}

/// Returns whether a specific call site can be inlined for `callee`: its return shape
/// must be uniform (all value or all void, never mixed and never absent) and, when the
/// site consumes a result, the callee must actually return a value. Selecting only such
/// sites lets `apply_inline_at_site` run infallibly.
fn site_is_inlinable(callee: &Function, has_result: bool) -> bool {
    let (saw_value, saw_void) = callee_return_shape(callee);
    if saw_value && saw_void {
        return false; // mixed value/void returns
    }
    if !saw_value && !saw_void {
        return false; // no normal return (always throws / never returns)
    }
    if has_result && !saw_value {
        return false; // result consumed but callee returns void
    }
    true
}

/// Build name -> index map over the user function table (module.functions only).
fn build_name_to_index(module: &Module) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for (i, f) in module.functions.iter().enumerate() {
        m.insert(f.name.clone(), i);
    }
    m
}

/// Remap a single terminator's block targets and value operands according to maps.
fn remap_terminator(
    mut term: Terminator,
    block_map: &HashMap<BlockId, BlockId>,
    value_map: &HashMap<ValueId, ValueId>,
) -> Terminator {
    match &mut term {
        Terminator::Br { target, args } => {
            if let Some(&nb) = block_map.get(target) {
                *target = nb;
            }
            for a in args.iter_mut() {
                if let Some(&nv) = value_map.get(a) {
                    *a = nv;
                }
            }
        }
        Terminator::CondBr {
            cond,
            then_target,
            then_args,
            else_target,
            else_args,
        } => {
            if let Some(&nv) = value_map.get(cond) {
                *cond = nv;
            }
            if let Some(&nb) = block_map.get(then_target) {
                *then_target = nb;
            }
            if let Some(&nb) = block_map.get(else_target) {
                *else_target = nb;
            }
            for a in then_args.iter_mut() {
                if let Some(&nv) = value_map.get(a) {
                    *a = nv;
                }
            }
            for a in else_args.iter_mut() {
                if let Some(&nv) = value_map.get(a) {
                    *a = nv;
                }
            }
        }
        Terminator::Switch {
            scrutinee,
            cases,
            default,
            default_args,
        } => {
            if let Some(&nv) = value_map.get(scrutinee) {
                *scrutinee = nv;
            }
            if let Some(&nb) = block_map.get(default) {
                *default = nb;
            }
            for c in cases.iter_mut() {
                if let Some(&nb) = block_map.get(&c.target) {
                    c.target = nb;
                }
                for a in c.args.iter_mut() {
                    if let Some(&nv) = value_map.get(a) {
                        *a = nv;
                    }
                }
            }
            for a in default_args.iter_mut() {
                if let Some(&nv) = value_map.get(a) {
                    *a = nv;
                }
            }
        }
        Terminator::Return { value } => {
            if let Some(v) = value.as_mut() {
                if let Some(&nv) = value_map.get(v) {
                    *v = nv;
                }
            }
        }
        Terminator::Throw { value } => {
            if let Some(&nv) = value_map.get(value) {
                *value = nv;
            }
        }
        Terminator::GeneratorSuspend {
            key,
            value,
            resume,
            resume_args,
        } => {
            if let Some(k) = key.as_mut() {
                if let Some(&nv) = value_map.get(k) {
                    *k = nv;
                }
            }
            if let Some(v) = value.as_mut() {
                if let Some(&nv) = value_map.get(v) {
                    *v = nv;
                }
            }
            if let Some(&nb) = block_map.get(resume) {
                *resume = nb;
            }
            for a in resume_args.iter_mut() {
                if let Some(&nv) = value_map.get(a) {
                    *a = nv;
                }
            }
        }
        Terminator::Fatal { .. } | Terminator::Unreachable => {}
    }
    term
}

/// Remap LocalSlot references inside an immediate (for cloned callee instrs).
fn remap_immediate_local(imm: &mut Immediate, local_map: &HashMap<LocalSlotId, LocalSlotId>) {
    match imm {
        Immediate::LocalSlot(ls) => {
            if let Some(&nl) = local_map.get(ls) {
                *ls = nl;
            }
        }
        Immediate::LocalSlotPair { first, second } => {
            if let Some(&nl) = local_map.get(first) {
                *first = nl;
            }
            if let Some(&nl) = local_map.get(second) {
                *second = nl;
            }
        }
        _ => {}
    }
}

/// Clone callee body (blocks, values, instructions, locals) into host with fresh ids.
/// Returns maps (old->new) and the remapped entry block id.
fn transplant_callee_body(
    host: &mut Function,
    callee: &Function,
) -> (HashMap<BlockId, BlockId>, HashMap<ValueId, ValueId>, HashMap<InstId, InstId>, HashMap<LocalSlotId, LocalSlotId>, BlockId) {
    let mut block_map: HashMap<BlockId, BlockId> = HashMap::new();
    let mut value_map: HashMap<ValueId, ValueId> = HashMap::new();
    let mut inst_map: HashMap<InstId, InstId> = HashMap::new();
    let mut local_map: HashMap<LocalSlotId, LocalSlotId> = HashMap::new();

    // Parameter slots and directly-returned slots are excluded from the callee's
    // epilogue cleanup (borrowed argument / ownership moved into the return value). The
    // host epilogue keys cleanup off `LocalKind::PhpLocal`, so transplant these slots as
    // `HiddenTemp` to reproduce that exclusion; ordinary refcounted internal locals keep
    // their `PhpLocal` kind so the host epilogue still frees them (the only difference is
    // deferred timing, which is unobservable for the destructor-free types we inline).
    let excluded_from_cleanup: HashSet<LocalSlotId> = callee_param_slots(callee)
        .into_iter()
        .chain(callee_directly_returned_slots(callee))
        .collect();

    // Clone locals first.
    for local in &callee.locals {
        let kind = if excluded_from_cleanup.contains(&local.id) {
            LocalKind::HiddenTemp
        } else {
            local.kind
        };
        let new_id = host.add_local(
            local.name.clone(),
            local.ir_type,
            local.php_type.clone(),
            kind,
        );
        local_map.insert(local.id, new_id);
    }

    // Create blocks + their param values (defs use final block ids).
    for block in &callee.blocks {
        let new_bid = BlockId::from_raw(host.blocks.len() as u32);
        block_map.insert(block.id, new_bid);

        let mut new_params: Vec<ValueId> = Vec::with_capacity(block.params.len());
        for (pidx, &old_pid) in block.params.iter().enumerate() {
            let old_v = callee.value(old_pid).expect("callee param value exists");
            let new_vid = ValueId::from_raw(host.values.len() as u32);
            value_map.insert(old_pid, new_vid);
            host.values.push(Value {
                ir_type: old_v.ir_type,
                php_type: old_v.php_type.clone(),
                def: ValueDef::BlockParam {
                    block: new_bid,
                    index: pidx as u16,
                },
                ownership: old_v.ownership,
            });
            new_params.push(new_vid);
        }
        host.blocks
            .push(BasicBlock::new(new_bid, block.name.clone(), new_params));
    }

    // Create instructions + their result values.
    for block in &callee.blocks {
        let new_bid = block_map[&block.id];
        for &old_iid in &block.instructions {
            let old_inst = callee.instruction(old_iid).expect("callee inst exists");
            let new_iid = InstId::from_raw(host.instructions.len() as u32);
            inst_map.insert(old_iid, new_iid);

            // Compute the instruction index inside block while we can.
            let inst_idx_in_block = host
                .block(new_bid)
                .map(|b| b.instructions.len() as u32)
                .unwrap_or(0);

            let new_res = if let Some(old_rid) = old_inst.result {
                let old_v = callee.value(old_rid).expect("callee result value");
                let new_vid = ValueId::from_raw(host.values.len() as u32);
                value_map.insert(old_rid, new_vid);
                host.values.push(Value {
                    ir_type: old_v.ir_type,
                    php_type: old_v.php_type.clone(),
                    def: ValueDef::Instruction {
                        block: new_bid,
                        index: inst_idx_in_block,
                        inst: new_iid,
                    },
                    ownership: old_v.ownership,
                });
                Some(new_vid)
            } else {
                None
            };

            let mut new_inst = old_inst.clone();
            new_inst.result = new_res;
            host.instructions.push(new_inst);
            // Re-borrow block only for append.
            host.block_mut(new_bid)
                .expect("block exists")
                .instructions
                .push(new_iid);
        }
    }

    // Patch operands and immediates inside transplanted instructions.
    // We scan from the first new instruction onward.
    let first_new_inst = host.instructions.len() - callee.instructions.len();
    for inst in host.instructions[first_new_inst..].iter_mut() {
        for op in &mut inst.operands {
            if let Some(&nv) = value_map.get(op) {
                *op = nv;
            }
        }
        if let Some(imm) = &mut inst.immediate {
            remap_immediate_local(imm, &local_map);
        }
    }

    // Set (remapped) terminators on the new blocks.
    let first_new_block = host.blocks.len() - callee.blocks.len();
    for (i, block) in callee.blocks.iter().enumerate() {
        let _new_bid = block_map[&block.id];
        let old_term = block.terminator.clone().expect("callee block terminated");
        let new_term = remap_terminator(old_term, &block_map, &value_map);
        host.blocks[first_new_block + i].terminator = Some(new_term);
    }

    let remapped_entry = block_map[&callee.entry];
    (block_map, value_map, inst_map, local_map, remapped_entry)
}

/// Perform the splice + return translation + result rewrite + neutralization for one site.
/// Returns true on success (always for eligible sites).
fn apply_inline_at_site(
    host: &mut Function,
    call_block_idx: usize,
    call_inst_id: InstId,
    callee: &Function,
    _target_name: &str,
) -> bool {
    // Snapshot state before heavy mutation for the call site.
    let call_inst = host
        .instruction(call_inst_id)
        .expect("call inst present")
        .clone();
    // `site_is_inlinable` (checked before selection) guarantees the callee returns
    // uniformly and that `has_result` implies a value-bearing return, so the splice
    // below is infallible: when `has_result` we join through a continuation param,
    // otherwise return values (if any) are dropped (they are plain scalars).
    let has_result = call_inst.result.is_some() && !call_inst.result_type.is_void();
    let orig_result_vid = call_inst.result;

    // Split: keep pre, separate call + post tail.
    let call_block = &mut host.blocks[call_block_idx];
    let call_pos = call_block
        .instructions
        .iter()
        .position(|&iid| iid == call_inst_id)
        .expect("call in its block");
    let mut tail = call_block.instructions.split_off(call_pos);
    let _call_from_tail = tail.remove(0); // the call itself; we will not place it back in reachable

    // Neutralize but KEEP the original result claim on the inst so we can legally
    // place it into the unreachable dead block and satisfy the value's def record.
    if let Some(inst) = host.instruction_mut(call_inst_id) {
        neutralize_to_nop(inst);
        // do not clear inst.result here; the dead block will host the Nop+result
    }

    // Transplant callee body (adds blocks, values, instrs, locals, patches internal refs).
    let (_block_map, _value_map, _inst_map, local_map, remap_entry) =
        transplant_callee_body(host, callee);

    let transplanted_start = host.blocks.len() - callee.blocks.len();

    // Create continuation block. Uses a fresh param vid when call produced a result.
    let cont_id = BlockId::from_raw(host.blocks.len() as u32);
    let cont_name = format!("inline_cont_{}", call_block_idx);
    let mut cont_params: Vec<ValueId> = Vec::new();
    let cont_result_param: Option<ValueId> = if has_result {
        // fresh vid for join; we will RAUW the orig call result to this
        let vid = ValueId::from_raw(host.values.len() as u32);
        // php/ownership from the call site result value (still valid before we may have overwritten)
        let (ir_t, php_t, own) = if let Some(rid) = orig_result_vid {
            if let Some(v) = host.value(rid) {
                (v.ir_type, v.php_type.clone(), v.ownership)
            } else {
                (call_inst.result_type, call_inst.result_php_type.clone(), call_inst.result_ownership)
            }
        } else {
            (call_inst.result_type, call_inst.result_php_type.clone(), call_inst.result_ownership)
        };
        host.values.push(Value {
            ir_type: ir_t,
            php_type: php_t.clone(),
            def: ValueDef::BlockParam {
                block: cont_id,
                index: 0,
            },
            ownership: own,
        });
        cont_params.push(vid);
        Some(vid)
    } else {
        None
    };
    host.blocks
        .push(BasicBlock::new(cont_id, cont_name, cont_params));

    // Install post-call tail into continuation; fix value defs for moved result producers.
    {
        let contb = host.block_mut(cont_id).unwrap();
        contb.instructions = tail;
        let cont_instrs: Vec<(usize, InstId)> = contb
            .instructions
            .iter()
            .enumerate()
            .map(|(k, &iid)| (k, iid))
            .collect();
        for (local_idx, iid) in cont_instrs {
            if let Some(inst) = host.instruction(iid) {
                if let Some(rid) = inst.result {
                    if let Some(v) = host.values.get_mut(rid.as_raw() as usize) {
                        if let ValueDef::Instruction { block, index, .. } = &mut v.def {
                            *block = cont_id;
                            *index = local_idx as u32;
                        }
                    }
                }
            }
        }
    }

    // Move original terminator of call block onto the cont.
    let old_term = host.blocks[call_block_idx]
        .terminator
        .take()
        .expect("call block had terminator");
    host.blocks[call_block_idx].terminator = None;
    host.block_mut(cont_id).unwrap().terminator = Some(old_term);

    // Bind the call arguments (already evaluated into pre values) into the
    // remapped param local slots. The transplanted body uses load_local on those
    // slots (as seen in real lowered EIR); the stores make the values visible.
    {
        let call_operands = call_inst.operands.clone();
        for (i, arg_vid) in call_operands.into_iter().enumerate() {
            if i >= callee.params.len() {
                break;
            }
            let old_slot = LocalSlotId::from_raw(i as u32);
            if let Some(&new_slot) = local_map.get(&old_slot) {
                let store_iid = InstId::from_raw(host.instructions.len() as u32);
                let store_inst = Instruction::new(
                    Op::StoreLocal,
                    vec![arg_vid],
                    Some(Immediate::LocalSlot(new_slot)),
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                    Op::StoreLocal.default_effects(),
                    None,
                );
                host.instructions.push(store_inst);
                host.blocks[call_block_idx].instructions.push(store_iid);
            }
        }
    }

    // Wire pre (call block) to jump into transplanted callee entry (always 0 args;
    // entry blocks never carry params in this EIR).
    host.blocks[call_block_idx].terminator = Some(Terminator::Br {
        target: remap_entry,
        args: Vec::new(),
    });

    // Translate ONLY Returns inside the just-transplanted callee blocks (not
    // original host blocks or prior inlines) into Br to our cont.
    let cont_idx = host.blocks.len() - 1;
    for bidx in transplanted_start..cont_idx {
        let blk = &mut host.blocks[bidx];
        if let Some(term) = blk.terminator.as_mut() {
            if let Terminator::Return { value } = term {
                let args = if has_result {
                    if let Some(rv) = value.take() {
                        vec![rv]
                    } else {
                        vec![]
                    }
                } else {
                    let _ = value.take();
                    vec![]
                };
                *term = Terminator::Br {
                    target: cont_id,
                    args,
                };
            }
        }
    }

    // Rewrite uses of the original call result to the cont param (if any).
    if let (Some(cr), Some(cp)) = (orig_result_vid, cont_result_param) {
        let mut m = HashMap::new();
        m.insert(cr, cp);
        crate::ir_passes::rewrite::replace_all_uses(host, &m);
    }

    // Attach the neutralized call instruction to a fresh unreachable block so its
    // result value definition record remains consistent with placement (even if
    // result uses were rewritten away).
    let dead_id = BlockId::from_raw(host.blocks.len() as u32);
    let mut dead = BasicBlock::new(dead_id, "inline_dead_call".to_string(), vec![]);
    dead.instructions.push(call_inst_id);
    dead.terminator = Some(Terminator::Unreachable);
    host.blocks.push(dead);

    // Always repair the def for the original call result vid (if any) to point into dead.
    if let Some(rid) = orig_result_vid {
        if let Some(v) = host.values.get_mut(rid.as_raw() as usize) {
            if let ValueDef::Instruction { block, index, .. } = &mut v.def {
                *block = dead_id;
                *index = 0;
            }
        }
    }

    // No local re-validation here: the driver re-validates the whole module after
    // `inline_small_functions` (and every function after each pass) in debug builds,
    // so we avoid paying for `validate_function` — or panicking — in `--release`.
    true
}

/// Scan one host and inline all eligible call sites, re-scanning after each splice.
///
/// Resolution uses the pre-built `resolver` and `recursive` snapshots, so the loop
/// never borrows the surrounding `Module` and can hold `&mut Function` soundly. Site
/// selection only picks sites for which `apply_inline_at_site` is infallible. The
/// `fuel` cap backstops termination; recursive callees are already excluded.
fn inline_into_function(
    host: &mut Function,
    name_to_idx: &HashMap<String, usize>,
    all_functions: &[Function],
    resolver: &CallTargetResolver,
    recursive: &HashSet<String>,
) -> bool {
    let mut any = false;
    let mut fuel = MAX_INLINES_PER_FUNCTION;
    loop {
        let mut site: Option<(usize, InstId, String, usize)> = None; // (block_idx, inst_id, name, callee_idx)
        'search: for (bidx, block) in host.blocks.iter().enumerate() {
            for &iid in &block.instructions {
                if let Some(inst) = host.instruction(iid) {
                    if is_user_call_op(inst.op) {
                        let has_result = inst.result.is_some() && !inst.result_type.is_void();
                        if let Some(tname) = resolver.resolve(inst) {
                            if let Some(&cidx) = name_to_idx.get(&tname) {
                                let callee = &all_functions[cidx];
                                if is_eligible_callee(callee, recursive)
                                    && site_is_inlinable(callee, has_result)
                                    && call_args_bind_directly(host, inst, callee)
                                    && call_string_args_are_stable(host, inst, callee)
                                {
                                    site = Some((bidx, iid, tname, cidx));
                                    break 'search;
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some((bidx, iid, tname, cidx)) = site {
            if fuel == 0 {
                // Backstop only: cycle exclusion already guarantees termination, so a
                // real program never exhausts this fuel before running out of sites.
                break;
            }
            fuel -= 1;
            let callee = all_functions[cidx].clone();
            apply_inline_at_site(host, bidx, iid, &callee, &tname);
            any = true;
            // Re-scan to find subsequent sites in the updated structure.
            continue;
        }
        break;
    }
    any
}

/// Entry point: inline eligible small functions at user call sites in the module.
/// Processes user functions, methods, closures etc. as hosts; only `module.functions`
/// are candidates. Resolution data and the recursive-function set are snapshotted up
/// front, so the mutating loops use plain disjoint borrows (no `unsafe`).
pub(crate) fn inline_small_functions(module: &mut Module) -> bool {
    if module.functions.is_empty() {
        return false;
    }
    let name_to_idx = build_name_to_index(module);
    let resolver = CallTargetResolver::new(module);

    // Snapshot callee bodies once (small); we only inline from the pre-mutation bodies,
    // and repeated inlining inside one host re-scans the updated host only.
    let callee_snapshots: Vec<Function> = module.functions.clone();
    let recursive = compute_recursive_functions(&callee_snapshots, &name_to_idx, &resolver);
    let mut changed = false;

    // Hosts in `module.functions`: indexed access yields a disjoint `&mut` each
    // iteration, while `resolver`/`callee_snapshots`/`recursive` are owned and borrow
    // nothing from `module`.
    let host_count = module.functions.len();
    for h in 0..host_count {
        if inline_into_function(
            &mut module.functions[h],
            &name_to_idx,
            &callee_snapshots,
            &resolver,
            &recursive,
        ) {
            changed = true;
        }
    }

    // Other body tables can host call sites to small helpers too. They are distinct
    // module fields, so iterating them mutably is sound without raw pointers.
    for host in module
        .class_methods
        .iter_mut()
        .chain(module.closures.iter_mut())
        .chain(module.fiber_wrappers.iter_mut())
        .chain(module.callback_wrappers.iter_mut())
        .chain(module.extern_callback_trampolines.iter_mut())
        .chain(module.runtime_callable_invokers.iter_mut())
    {
        if inline_into_function(host, &name_to_idx, &callee_snapshots, &resolver, &recursive) {
            changed = true;
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    // Real tests are in src/ir_passes/tests/inline_test.rs (Builder-driven, per repo policy).
}
