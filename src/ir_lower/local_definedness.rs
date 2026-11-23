//! Purpose:
//! Gives the locals a read may find UNASSIGNED a run-time "is defined" flag, so a read of one
//! answers what php answers: a plain read warns `Undefined variable $x` and yields NULL,
//! `isset()`/`empty()`/`??` see null without a warning, and `compact()` leaves the name out with
//! `compact(): Undefined variable $x`.
//!
//! Called from:
//! - `crate::ir_lower::numeric_storage::lower_body`, which lowers a body through
//!   [`begin_body`] and replays it once when [`take_replay_request`] says so.
//! - `crate::ir_lower::expr` for every `$x` read, a by-value closure capture, `++`/`--`, and the
//!   array literal `compact()` becomes.
//! - `crate::ir_lower::stmt` after every array element write (which defines its root variable),
//!   around `for`/`foreach` loops, and in the `try` lowering.
//! - `crate::ir_lower::context::LoweringContext::note_slot_initialized` for every initialization.
//!
//! Key details:
//! - Every php variable has a static "definitely defined here" MARKER in `initialized_slots`,
//!   set where lowering initializes its slot. Every branch merge already intersects that set,
//!   so the fact needs no merge code of its own. Two constructs keep slot facts their body
//!   established although the body may not run, which is sound for storage but not for
//!   definedness: `for`/`foreach` (their markers are restored after the loop) and `try` (a catch
//!   starts from the markers of the try's entry, and the join keeps what every edge defined).
//! - Pay-for-use. A body whose reads all find the marker set gets no flag and emits exactly the
//!   code it did before. Only a name some read finds possibly undefined is tracked: its marker
//!   becomes a run-time I64 flag slot, and only the reads whose marker is not set test it.
//! - Two passes, only for such bodies. The flag must be set by every write of the name,
//!   including the writes lowered BEFORE the read that discovers the need, so the first lowering
//!   records the names and the body is lowered again with their flags installed. The names
//!   survive rollback, so a later replay of the same body installs them up front.
//! - The flag is set by the builder (`Builder::set_definedness_flags`) after EVERY instruction that
//!   writes a slot of the name, not by the lowering paths one by one: a store the lowering does
//!   not report as an initialization still defines the variable at run time. A write that only
//!   mutates the storage in place (`$a[] = 1`, a by-reference fill) is covered by
//!   `note_slot_initialized` and by [`define_element_write_root`].
//! - A variable a body may pass to a by-reference parameter of a function it cannot resolve, a
//!   closure or a constructor is never tracked: the call writes its storage without any store
//!   lowering sees. A method call is not in that set: binding the argument by reference is a
//!   write of its slot the builder already marks as a definition.
//! - A local `$a[] = …` creates keeps its entry-block seed (`seed_vivified_array_locals`): the
//!   seed only pre-allocates storage, the flag says whether php's variable exists yet. Such a
//!   local is tracked up front whenever the body reads it, which needs no replay.
//! - A read that may find the variable unassigned is a boxed `mixed` whatever type the variable
//!   holds elsewhere ([`load_holds_null`]): an int, float, bool, string, array or object slot has
//!   no representation of NULL, and the undefined path read it as uninitialized storage (a
//!   garbage int). The checker widens the same reads with `null`
//!   (`Checker::binding_read_type`) and expression merges predict them as `mixed`
//!   ([`read_lowers_as_boxed_mixed`]). A variable every read of which finds it defined keeps its
//!   unboxed slot and its code unchanged.
//! - `unset($x)` ends the variable ([`note_unset`]): the flag is cleared at run time and the
//!   static "defined" fact dropped, so the next read warns again.
//! - Bodies whose scope other code can write (`eval`, a runtime `include`, `extract()`) keep the
//!   old behaviour: their writes are not all visible here.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};
use std::collections::BTreeSet;

use crate::ir::{Immediate, IrType, LocalKind, LocalSlotId, Op, RuntimeCallTarget, Terminator};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::names::php_symbol_key;
use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::PhpType;

/// Per-body state of the run-time definedness flags.
#[derive(Default)]
pub(crate) struct DefinednessState {
    /// Whether the body may track definedness at all; decided once, by [`begin_body`].
    eligible: Option<bool>,
    /// Names the body tracks. Grows when a lowering discovers a possibly unassigned read.
    tracked: BTreeSet<String>,
    /// Names whose entry seed pre-allocates storage without defining the variable.
    seeded: HashSet<String>,
    /// The installed flag slot of every tracked name, for the current lowering attempt.
    flags: HashMap<String, LocalSlotId>,
    /// Spans of the variables `isset`, `empty` and `??` probe: read without a warning.
    quiet_reads: HashSet<Span>,
    /// The subset of `quiet_reads` whose probe operand is the variable itself.
    whole_probe_reads: HashSet<Span>,
    /// Names the body may pass to a by-reference parameter of a call this module cannot follow
    /// (an unresolved function, a closure, a constructor). The call binds the argument's own
    /// storage and defines it without a store this module could see, so they are never tracked.
    by_ref_arguments: HashSet<String>,
    /// Names a read found possibly unassigned while they had no flag.
    discovered: BTreeSet<String>,
    /// Static "definitely defined" marker of every untracked name, an id outside the frame's
    /// slot range that only ever lives in `initialized_slots` (a tracked name's marker is its
    /// flag slot). Stable across attempts, so a rollback's restored set keeps meaning the same.
    markers: HashMap<String, LocalSlotId>,
    /// Open `try` statements: the markers known at entry and those at each exit to the join.
    try_frames: Vec<(HashSet<LocalSlotId>, Vec<HashSet<LocalSlotId>>)>,
    /// Names a loop both reads and assigns: the read may run before the store lowering sees
    /// (the second iteration finds what the first stored), so such a read cannot be answered
    /// from the slot-less "undefined, therefore null" fact of the first lowered iteration.
    loop_carried: HashSet<String>,
    /// Set while a `compact()` literal is lowered through its ordinary path.
    in_compact: bool,
    /// Counter for the generated `compact()` result locals.
    compact_temps: usize,
}

impl DefinednessState {
    /// True when the body tracks no definedness: every hook returns at once.
    pub(crate) fn is_inactive(&self) -> bool {
        self.eligible != Some(true)
    }

    /// Returns the installed flag of a tracked name.
    pub(crate) fn flag(&self, name: &str) -> Option<LocalSlotId> {
        self.flags.get(name).copied()
    }

    /// Whether a name's slot holds an entry seed before the variable exists.
    pub(crate) fn is_seeded(&self, name: &str) -> bool {
        self.seeded.contains(name)
    }

    /// Returns the marker that says `name` is definitely defined, allocating an untracked
    /// name's on first use.
    pub(crate) fn marker(&mut self, name: &str) -> LocalSlotId {
        if let Some(flag) = self.flags.get(name) {
            return *flag;
        }
        if let Some(marker) = self.markers.get(name) {
            return *marker;
        }
        let marker = LocalSlotId::from_raw(u32::MAX - self.markers.len() as u32);
        self.markers.insert(name.to_string(), marker);
        marker
    }

    /// Every marker this body has handed out.
    fn all_markers(&self) -> Vec<LocalSlotId> {
        self.flags.values().chain(self.markers.values()).copied().collect()
    }

    /// Whether the body may track definedness.
    fn is_eligible(&self) -> bool {
        self.eligible == Some(true)
    }
}

/// Returns the markers known set on the current path.
fn known_markers(ctx: &LoweringContext<'_, '_>) -> HashSet<LocalSlotId> {
    ctx.definedness
        .all_markers()
        .into_iter()
        .filter(|marker| ctx.definedness_flag_is_known_set(*marker))
        .collect()
}

/// Makes exactly `known` the markers set on the current path. A marker set this way emits no
/// flag store: every path into here already set the flag it stands for.
fn set_known_markers(ctx: &mut LoweringContext<'_, '_>, known: &HashSet<LocalSlotId>) {
    for marker in ctx.definedness.all_markers() {
        if known.contains(&marker) {
            ctx.assume_definedness_flag_set(marker);
        } else {
            ctx.forget_definedness_flag(marker);
        }
    }
}

/// Installs the flags of every tracked name before a body is lowered.
///
/// Called at the start of each lowering attempt of the body, whose earlier attempts' slots a
/// rollback has discarded. The first call decides eligibility and tracks, up front, the seeded
/// array locals the body reads. Each flag starts as the variable is now: set for a name already
/// stored (a parameter), clear otherwise and for a seeded name.
pub(crate) fn begin_body(ctx: &mut LoweringContext<'_, '_>, body: &[Stmt]) {
    ctx.definedness.flags.clear();
    ctx.definedness.discovered.clear();
    ctx.builder.set_definedness_flags(HashMap::default());
    if ctx.definedness.eligible.is_none() {
        let eligible = body_is_eligible(ctx, body);
        ctx.definedness.eligible = Some(eligible);
        if eligible {
            let (quiet, whole) = quiet_read_spans(body);
            ctx.definedness.quiet_reads = quiet;
            ctx.definedness.whole_probe_reads = whole;
            ctx.definedness.by_ref_arguments = by_ref_argument_names(ctx, body);
            ctx.definedness.loop_carried = loop_carried_names(body);
            let params: Vec<String> = ctx
                .builder
                .function()
                .params
                .iter()
                .map(|param| param.name.clone())
                .collect();
            let seeded = crate::append_vivify::vivified_array_locals(
                body,
                params.iter().map(String::as_str),
            );
            let read = read_variable_names(body);
            for name in seeded {
                if read.contains(&name) && is_trackable_name(ctx, &name) {
                    ctx.definedness.tracked.insert(name.clone());
                }
                ctx.definedness.seeded.insert(name);
            }
        }
    }
    if !ctx.definedness.is_eligible() {
        return;
    }
    let names: Vec<String> = ctx.definedness.tracked.iter().cloned().collect();
    let mut flags = HashMap::default();
    for name in names {
        let flag = ctx.builder.add_local(
            Some(crate::names::generated_local_name(&format!("{name}#defined"))),
            IrType::I64,
            PhpType::Int,
            LocalKind::HiddenTemp,
        );
        flags.insert(name, flag);
    }
    ctx.builder.set_definedness_flags(flags.clone());
    ctx.definedness.flags = flags;
    // Names stored before the body (parameters, captures) are defined from the start; a
    // seeded local is not, whatever its slot holds.
    let mut stored: Vec<String> = ctx
        .local_slots
        .keys()
        .filter(|name| !ctx.definedness.seeded.contains(*name) && ctx.local_slot_is_initialized_here(name))
        .cloned()
        .collect();
    stored.sort();
    let tracked: Vec<String> = ctx.definedness.tracked.iter().cloned().collect();
    for name in tracked {
        let flag = ctx.definedness.flags[&name];
        if stored.contains(&name) {
            ctx.mark_definedness_flag(flag);
        } else {
            ctx.builder.emit_definedness_flag_store(flag, false);
        }
    }
    for name in stored {
        let marker = ctx.definedness.marker(&name);
        ctx.assume_definedness_flag_set(marker);
    }
}

/// Returns true once when the attempt just lowered found a possibly unassigned read of a name
/// it does not track yet: the name is now tracked and the caller lowers the body again.
pub(crate) fn take_replay_request(ctx: &mut LoweringContext<'_, '_>) -> bool {
    let discovered = std::mem::take(&mut ctx.definedness.discovered);
    let mut grew = false;
    for name in discovered {
        grew |= ctx.definedness.tracked.insert(name);
    }
    grew
}

/// Whether a read of `name` here lowers to a boxed `mixed` because the variable may still be
/// unassigned: expression merges (`?:`, `match`) must predict that value type for the arm.
pub(crate) fn read_lowers_as_boxed_mixed(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    if !ctx.definedness.is_eligible()
        || !is_trackable_name(ctx, name)
        || !ctx.variable_read_is_frame_local(name)
    {
        return false;
    }
    ctx.definedness
        .flag(name)
        .is_some_and(|flag| !ctx.definedness_flag_is_known_set(flag))
        && load_holds_null(ctx, name)
}

/// Lowers a read of `$name` in value position.
pub(crate) fn lower_variable_read(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    span: Span,
) -> LoweredValue {
    let Some(flag) = read_guard(ctx, name) else {
        return crate::ir_lower::expr::lower_local_value(ctx, name, span);
    };
    let quiet = ctx.definedness.quiet_reads.contains(&span);
    let whole_probe = ctx.definedness.whole_probe_reads.contains(&span);
    // The root of a probed chain (`$box['k'] ?? $d`) hands its value to the chain, which asks a
    // boxed ArrayAccess receiver `offsetExists()` first: it keeps its typed load, as before.
    let chain_root = quiet && !whole_probe;
    let holds_null = if chain_root {
        load_is_mixed(ctx, name)
    } else {
        load_holds_null(ctx, name)
    };
    if holds_null || whole_probe || (quiet && ctx.definedness.seeded.contains(name)) {
        return lower_guarded_read(ctx, name, flag, quiet, span);
    }
    if !quiet {
        warn_unless_defined(ctx, name, flag, span);
    }
    crate::ir_lower::expr::lower_local_value(ctx, name, span)
}

/// Whether a read of `name` is lowered as a boxed `mixed`, which can carry php's NULL, when the
/// variable may be unassigned.
///
/// A slot that holds a scalar, a string, an array or an object has no representation of NULL: the
/// undefined path used to read it as it stood, uninitialized storage that printed as a garbage
/// int (`elephc-08wi`), so those reads are boxed too, in the cases the flag says undefined (a
/// variable every read of which finds it defined never reaches here and keeps its unboxed slot).
/// The checker widens such reads with `null` in step (`Checker::binding_read_type`), so a
/// consumer sees the nullable type it was typed against.
///
/// The root of a probed chain (`isset($box['k'])`, `$box['k'] ?? $d`) keeps its typed load: the
/// chain consumes it, and an `empty($box['k'])` on a boxed ArrayAccess receiver would no longer
/// ask `offsetExists()` first. A probe whose whole operand is the variable (`isset($x)`,
/// `$x ?? $d`) is boxed as well, as is any probe of a seeded array local: they consume the value
/// themselves.
fn load_holds_null(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    load_is_mixed(ctx, name)
        || ctx.local_load_php_type(name).is_some_and(|ty| ty.is_plain_binding_storage())
}

/// Whether the load of `name` is already a boxed `mixed` (or the `null` type) in its storage.
fn load_is_mixed(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    ctx.local_load_php_type(name)
        .is_some_and(|ty| matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Void))
}

/// Reads a by-value closure capture of `$name`.
///
/// An undefined variable is captured as NULL. `use ($x)` warns like any read, as php does when
/// the closure is created; an arrow function captures only what exists and never warns.
pub(crate) fn lower_capture_read(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    arrow: bool,
    span: Span,
) -> LoweredValue {
    let Some(flag) = read_guard(ctx, name) else {
        return crate::ir_lower::expr::lower_local_value(ctx, name, span);
    };
    if load_holds_null(ctx, name) {
        return lower_guarded_read(ctx, name, flag, arrow, span);
    }
    if !arrow {
        warn_unless_defined(ctx, name, flag, span);
    }
    crate::ir_lower::expr::lower_local_value(ctx, name, span)
}

/// Prepares `++`/`--` on `$name`, which reads the variable before writing it: an undefined one
/// warns like a read and, when its storage can hold it, is set to NULL first, so the operator
/// sees php's NULL operand.
pub(crate) fn define_before_increment(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) {
    let Some(flag) = read_guard(ctx, name) else {
        return;
    };
    if !load_holds_null(ctx, name) {
        warn_unless_defined(ctx, name, flag, span);
        return;
    }
    let defined = load_flag(ctx, flag, span);
    let split_initialized = ctx.initialized_slots_snapshot();
    let split_types = ctx.local_types_snapshot();
    let undefined_block = ctx.builder.create_named_block("var.undefined", Vec::new());
    let merge = ctx.builder.create_named_block("var.defined", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: defined.value,
        then_target: merge,
        then_args: Vec::new(),
        else_target: undefined_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(undefined_block);
    emit_warning(ctx, &format!("Warning: Undefined variable ${name}\n"), span);
    let assign = Stmt::new(
        StmtKind::Assign {
            name: name.to_string(),
            value: Expr::new(ExprKind::Null, span),
        },
        span,
    );
    crate::ir_lower::stmt::lower_stmt(ctx, &assign);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(split_initialized);
    ctx.restore_local_types(split_types);
    // Either path left a value php can increment; the storage now also holds NULL.
    ctx.set_local_type(name, PhpType::Mixed);
    note_defined(ctx, name);
}

/// Raises php's `Undefined variable $x` when the flag says the variable does not exist.
fn warn_unless_defined(ctx: &mut LoweringContext<'_, '_>, name: &str, flag: LocalSlotId, span: Span) {
    let defined = load_flag(ctx, flag, span);
    let undefined_block = ctx.builder.create_named_block("var.undefined", Vec::new());
    let read = ctx.builder.create_named_block("var.read", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: defined.value,
        then_target: read,
        then_args: Vec::new(),
        else_target: undefined_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(undefined_block);
    emit_warning(ctx, &format!("Warning: Undefined variable ${name}\n"), span);
    branch_to(ctx, read);
    ctx.builder.position_at_end(read);
}

/// Returns the flag a read of `name` must test here, recording an untracked possibly unassigned
/// read for the replay.
fn read_guard(ctx: &mut LoweringContext<'_, '_>, name: &str) -> Option<LocalSlotId> {
    if !ctx.definedness.is_eligible()
        || !is_trackable_name(ctx, name)
        || !ctx.variable_read_is_frame_local(name)
    {
        return None;
    }
    match ctx.definedness.flag(name) {
        Some(flag) => (!ctx.definedness_flag_is_known_set(flag)).then_some(flag),
        None => {
            let marker = ctx.definedness.marker(name);
            if !ctx.definedness_flag_is_known_set(marker) {
                ctx.definedness.discovered.insert(name.to_string());
            }
            None
        }
    }
}

/// Records that the current path defined `name`: sets its flag when it has one, otherwise its
/// marker.
pub(crate) fn note_defined(ctx: &mut LoweringContext<'_, '_>, name: &str) {
    if let Some(flag) = ctx.definedness.flag(name) {
        ctx.mark_definedness_flag(flag);
    } else {
        let marker = ctx.definedness.marker(name);
        ctx.assume_definedness_flag_set(marker);
    }
}

/// Records that the current path un-defined `name` with `unset($name)`: a later read warns
/// `Undefined variable` and yields NULL like a variable that was never assigned.
///
/// A tracked name's flag is cleared at run time; an untracked name loses its static "defined"
/// marker, so the next read finds it possibly undefined and the body is lowered again with the
/// name tracked.
pub(crate) fn note_unset(ctx: &mut LoweringContext<'_, '_>, name: &str) {
    if ctx.definedness.is_inactive() || ctx.builder.insertion_block_is_terminated() {
        return;
    }
    if let Some(flag) = ctx.definedness.flag(name) {
        ctx.builder.emit_definedness_flag_store(flag, false);
        ctx.forget_definedness_flag(flag);
    } else {
        let marker = ctx.definedness.marker(name);
        ctx.forget_definedness_flag(marker);
    }
}

/// Reads a tracked local that may be unassigned: its value when the flag is set, otherwise NULL
/// after php's warning (none for a probe operand). The result is a boxed `mixed`, the only
/// representation that holds both.
fn lower_guarded_read(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    flag: LocalSlotId,
    quiet: bool,
    span: Span,
) -> LoweredValue {
    let temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let defined = load_flag(ctx, flag, span);
    let split_initialized = ctx.initialized_slots_snapshot();
    let defined_block = ctx.builder.create_named_block("var.defined", Vec::new());
    let undefined_block = ctx.builder.create_named_block("var.undefined", Vec::new());
    let merge = ctx.builder.create_named_block("var.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: defined.value,
        then_target: defined_block,
        then_args: Vec::new(),
        else_target: undefined_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(defined_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    // The slot a later store of an enclosing loop creates does not exist yet where this read is
    // lowered: reserve it as a boxed `mixed` for the read, so the flag-guarded load reads it
    // instead of the constant NULL an absent slot answers.
    let reserved = !ctx.has_local_slot(name);
    if reserved {
        ctx.declare_local(name, PhpType::Mixed);
        ctx.local_types.insert(name.to_string(), PhpType::Mixed);
    }
    // An `unset()` on one branch leaves the flow type `null` while the frame slot keeps the
    // representation the other branch stored: the defined path reads the slot as it is.
    let flow_type = ctx.local_types.get(name).cloned();
    let slot_type = ctx.local_storage_type(name);
    let reads_slot_representation = !reserved
        && matches!(flow_type, Some(PhpType::Void))
        && slot_type.is_plain_binding_storage();
    if reads_slot_representation {
        ctx.local_types.insert(name.to_string(), slot_type);
    }
    let value = crate::ir_lower::expr::lower_local_value(ctx, name, span);
    if reserved {
        ctx.local_types.remove(name);
    } else if reads_slot_representation {
        ctx.local_types.insert(name.to_string(), PhpType::Void);
    }
    crate::ir_lower::expr::store_value_into_temp(ctx, &temp, PhpType::Mixed, value, span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(undefined_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    if !quiet {
        emit_warning(ctx, &format!("Warning: Undefined variable ${name}\n"), span);
    }
    let null = ctx.emit_value(
        Op::ConstNull,
        Vec::new(),
        None,
        PhpType::Void,
        Op::ConstNull.default_effects(),
        Some(span),
    );
    crate::ir_lower::expr::store_value_into_temp(ctx, &temp, PhpType::Mixed, null, span);
    branch_to(ctx, merge);

    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(split_initialized);
    crate::ir_lower::expr::take_owned_temp(ctx, &temp, span)
}

/// Marks the root variable of an array element write defined: php creates it with the array.
pub(crate) fn define_element_write_root(ctx: &mut LoweringContext<'_, '_>, root: &str) {
    if ctx.definedness.is_inactive() || ctx.builder.insertion_block_is_terminated() {
        return;
    }
    note_defined(ctx, root);
}

/// Defines the root of a statement-level array element write once the statement has run.
pub(crate) fn define_after_statement(ctx: &mut LoweringContext<'_, '_>, stmt: &Stmt) {
    if ctx.definedness.is_inactive() {
        return;
    }
    let root = match &stmt.kind {
        StmtKind::ArrayPush { array, .. } | StmtKind::ArrayAssign { array, .. } => Some(array.as_str()),
        StmtKind::NestedArrayAssign { target, .. } => element_write_root(target),
        _ => None,
    };
    if let Some(root) = root {
        define_element_write_root(ctx, root);
    }
}

/// Captures which markers are known set before a `for` or `foreach`, whose body may run zero
/// times. `None` for any other statement, or when the body tracks no definedness.
///
/// Those loops leave the facts their body established in place after the loop, which is sound
/// for storage (a slot the body may have stored is released like a stored one) but not for
/// definedness, so [`restore_loop_markers`] takes the body's markers back afterwards.
pub(crate) fn loop_entry_markers(
    ctx: &LoweringContext<'_, '_>,
    stmt: &Stmt,
) -> Option<HashSet<LocalSlotId>> {
    if ctx.definedness.is_inactive()
        || !matches!(stmt.kind, StmtKind::For { .. } | StmtKind::Foreach { .. })
    {
        return None;
    }
    Some(known_markers(ctx))
}

/// Forgets every definedness fact a loop body established (see [`loop_entry_markers`]), keeping
/// the variable a `for` initializer always assigns.
pub(crate) fn restore_loop_markers(
    ctx: &mut LoweringContext<'_, '_>,
    stmt: &Stmt,
    known: HashSet<LocalSlotId>,
) {
    for marker in ctx.definedness.all_markers() {
        if !known.contains(&marker) {
            ctx.forget_definedness_flag(marker);
        }
    }
    if let StmtKind::For { init: Some(init), .. } = &stmt.kind {
        if let StmtKind::Assign { name, .. } = &init.kind {
            if !ctx.builder.insertion_block_is_terminated() {
                note_defined(ctx, name);
            }
        }
    }
}

/// Opens a `try`: its handler starts from the markers known here, whichever statement of the
/// body threw.
pub(crate) fn begin_try(ctx: &mut LoweringContext<'_, '_>) {
    if ctx.definedness.is_inactive() {
        return;
    }
    let entry = known_markers(ctx);
    ctx.definedness.try_frames.push((entry, Vec::new()));
}

/// Records the markers of one edge into the open `try`'s join (the body's or a catch's end).
pub(crate) fn note_try_exit(ctx: &mut LoweringContext<'_, '_>) {
    if ctx.definedness.is_inactive() || ctx.builder.insertion_block_is_terminated() {
        return;
    }
    let known = known_markers(ctx);
    if let Some((_, exits)) = ctx.definedness.try_frames.last_mut() {
        exits.push(known);
    }
}

/// Resets the markers to the open `try`'s entry, where a catch (or its dispatch) starts.
pub(crate) fn enter_try_handler(ctx: &mut LoweringContext<'_, '_>) {
    if ctx.definedness.is_inactive() {
        return;
    }
    if let Some(entry) = ctx.definedness.try_frames.last().map(|(entry, _)| entry.clone()) {
        set_known_markers(ctx, &entry);
    }
}

/// Closes a `try`: after it, a name is defined when every edge into the join defined it.
pub(crate) fn end_try(ctx: &mut LoweringContext<'_, '_>) {
    if ctx.definedness.is_inactive() {
        return;
    }
    let Some((_, exits)) = ctx.definedness.try_frames.pop() else {
        return;
    };
    let mut exits = exits.into_iter();
    let Some(first) = exits.next() else {
        return;
    };
    let joined = exits.fold(first, |joined, exit| joined.intersection(&exit).copied().collect());
    set_known_markers(ctx, &joined);
}

/// Returns the variable an element-write target (`$a[..]`, `$a[..][..]`) is rooted at.
pub(crate) fn element_write_root(target: &Expr) -> Option<&str> {
    match &target.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::ArrayAccess { array, .. } => element_write_root(array),
        _ => None,
    }
}

/// Lowers the array literal a `compact()` call became, leaving out each name that is
/// undefined at run time with php's warning. `None` keeps the ordinary literal lowering, which
/// is exact when every name is defined here.
pub(crate) fn lower_compact_literal(
    ctx: &mut LoweringContext<'_, '_>,
    pairs: &[(Expr, Expr)],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !ctx.definedness.is_eligible() || ctx.definedness.in_compact || pairs.is_empty() {
        return None;
    }
    let names = pairs
        .iter()
        .map(|(key, value)| crate::name_resolver::compact_entry_variable(key, value))
        .collect::<Option<Vec<&str>>>()?;
    let uncertain: Vec<(&str, LocalSlotId)> = names
        .iter()
        .filter_map(|name| {
            let flag = ctx.definedness.flag(name)?;
            (!ctx.definedness_flag_is_known_set(flag)).then_some((*name, flag))
        })
        .collect();
    if uncertain.is_empty() {
        return None;
    }
    let span = expr.span;
    ctx.definedness.in_compact = true;
    let built = crate::ir_lower::expr::lower_expr(ctx, expr);
    ctx.definedness.in_compact = false;
    let temp = crate::names::generated_local_name(&format!(
        "__elephc_compact{}",
        ctx.definedness.compact_temps
    ));
    ctx.definedness.compact_temps += 1;
    let built_type = ctx.builder.value_php_type(built.value);
    let stored = crate::ir_lower::ownership::acquire_if_refcounted(ctx, built, Some(span));
    ctx.store_local(&temp, stored, built_type, Some(span));
    if stored.value != built.value && ctx.value_needs_release_after_retaining_store(built) {
        crate::ir_lower::ownership::release_if_owned(ctx, built, Some(span));
    }
    for (name, flag) in uncertain {
        let defined = load_flag(ctx, flag, span);
        let split_initialized = ctx.initialized_slots_snapshot();
        let split_types = ctx.local_types_snapshot();
        let undefined_block = ctx.builder.create_named_block("compact.undefined", Vec::new());
        let merge = ctx.builder.create_named_block("compact.next", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: defined.value,
            then_target: merge,
            then_args: Vec::new(),
            else_target: undefined_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(undefined_block);
        emit_warning(ctx, &format!("Warning: compact(): Undefined variable ${name}\n"), span);
        let unset = Stmt::new(
            StmtKind::ExprStmt(Expr::new(
                ExprKind::FunctionCall {
                    name: crate::names::Name::unqualified("unset"),
                    args: vec![Expr::new(
                        ExprKind::ArrayAccess {
                            array: Box::new(Expr::new(ExprKind::Variable(temp.clone()), span)),
                            index: Box::new(Expr::new(ExprKind::StringLiteral(name.to_string()), span)),
                        },
                        span,
                    )],
                },
                span,
            )),
            span,
        );
        crate::ir_lower::stmt::lower_stmt(ctx, &unset);
        branch_to(ctx, merge);
        ctx.builder.position_at_end(merge);
        ctx.restore_initialized_slots(split_initialized);
        ctx.restore_local_types(split_types);
    }
    Some(crate::ir_lower::expr::lower_expr(
        ctx,
        &Expr::new(ExprKind::Variable(temp), span),
    ))
}

/// Loads a definedness flag for a branch.
fn load_flag(ctx: &mut LoweringContext<'_, '_>, flag: LocalSlotId, span: Span) -> LoweredValue {
    ctx.emit_value(
        Op::LoadLocal,
        Vec::new(),
        Some(Immediate::LocalSlot(flag)),
        PhpType::Int,
        Op::LoadLocal.default_effects(),
        Some(span),
    )
}

/// Ends the current block with a jump, unless it already ended.
fn branch_to(ctx: &mut LoweringContext<'_, '_>, target: crate::ir::BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br {
            target,
            args: Vec::new(),
        });
    }
}

/// Raises one complete php diagnostic line through the runtime's error-reporting path.
fn emit_warning(ctx: &mut LoweringContext<'_, '_>, text: &str, span: Span) {
    let message = crate::ir_lower::expr::lower_expr(
        ctx,
        &Expr::new(ExprKind::StringLiteral(text.to_string()), span),
    );
    ctx.emit_void(
        Op::RuntimeCall,
        vec![message.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::WriteDiagnostic)),
        crate::ir_lower::effects_lookup::runtime_effects(),
        Some(span),
    );
}

/// Whether a slot name is a php variable of the source rather than a compiler temporary.
pub(crate) fn names_a_php_variable(name: &str) -> bool {
    name != "this" && !name.contains('#') && !name.starts_with("__eir_")
}

/// Whether `name` is a php variable this frame alone can leave unassigned.
fn is_trackable_name(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    names_a_php_variable(name)
        && !crate::names::is_generated_local_name(name)
        && !crate::superglobals::is_superglobal(name)
        && !crate::globals_array::is_alias(name)
        && !(ctx.in_main && matches!(name, "argc" | "argv"))
        && !ctx.definedness.by_ref_arguments.contains(name)
}

/// Names every bare variable the body passes where the parameter may be by reference.
///
/// A function the lowering can resolve answers per parameter; any other call (a method, a
/// closure, a `new`, an unknown function, a named or spread argument list) is assumed to take
/// every bare variable argument by reference.
fn by_ref_argument_names(ctx: &LoweringContext<'_, '_>, body: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::default();
    let add_all = |args: &[Expr], names: &mut HashSet<String>| {
        for arg in args {
            let arg = match &arg.kind {
                ExprKind::NamedArg { value, .. } => value.as_ref(),
                _ => arg,
            };
            if let ExprKind::Variable(name) = &arg.kind {
                names.insert(name.clone());
            }
        }
    };
    walk_nodes(body, &mut |node| {
        let Node::Expr(expr) = node else {
            return;
        };
        match &expr.kind {
            ExprKind::FunctionCall { name, args } => {
                let key = php_symbol_key(name.as_str().trim_start_matches('\\'));
                if matches!(key.as_str(), "isset" | "empty" | "unset") {
                    return;
                }
                let positional = args
                    .iter()
                    .all(|arg| !matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)));
                match crate::ir_lower::expr::call_signature(ctx, name.as_str(), false) {
                    Some(sig) if positional => {
                        for (index, arg) in args.iter().enumerate() {
                            let by_ref = sig.ref_params.get(index).copied().unwrap_or_else(|| {
                                sig.variadic.is_some() && sig.ref_params.last().copied().unwrap_or(false)
                            });
                            if let (true, ExprKind::Variable(name)) = (by_ref, &arg.kind) {
                                names.insert(name.clone());
                            }
                        }
                    }
                    _ => add_all(args, &mut names),
                }
            }
            ExprKind::ClosureCall { args, .. }
            | ExprKind::ExprCall { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::NewGeneric { args, .. }
            | ExprKind::NewDynamic { args, .. }
            | ExprKind::NewDynamicObject { args, .. }
            | ExprKind::NewScopedObject { args, .. } => add_all(args, &mut names),
            _ => {}
        }
    });
    names
}

/// Whether a body's variables are all written by code lowering sees.
fn body_is_eligible(ctx: &LoweringContext<'_, '_>, body: &[Stmt]) -> bool {
    if ctx.body_contains_eval {
        return false;
    }
    let mut eligible = true;
    walk_nodes(body, &mut |node| {
        if let Node::Expr(expr) = node {
            if let ExprKind::FunctionCall { name, .. } = &expr.kind {
                let key = php_symbol_key(name.as_str().trim_start_matches('\\'));
                if matches!(key.as_str(), "extract" | "parse_str" | "mb_parse_str" | "eval") {
                    eligible = false;
                }
            }
        }
    });
    eligible
}

/// Collects the variables at the root of every `isset()`, `empty()`, `unset()` and `??` operand
/// spine.
///
/// php fetches them in its quiet "is" mode, so an unassigned one warns about nothing. Only the
/// spine is quiet: `isset($a[$b])` still warns for `$b` (see `types::checker::null_probe`).
///
/// The second set holds the probes whose WHOLE operand is the variable (`isset($x)`, `$x ?? d`):
/// those consume the value themselves, where a chain root (`$box['k'] ?? d`) hands it on.
fn quiet_read_spans(body: &[Stmt]) -> (HashSet<Span>, HashSet<Span>) {
    let mut spans = HashSet::default();
    let mut whole = HashSet::default();
    let mut record = |operand: &Expr| {
        if matches!(operand.kind, ExprKind::Variable(_)) {
            whole.insert(operand.span);
        }
        let mut current = operand;
        loop {
            match &current.kind {
                ExprKind::Variable(_) => {
                    spans.insert(current.span);
                    return;
                }
                ExprKind::ArrayAccess { array, .. } => current = array,
                ExprKind::PropertyAccess { object, .. }
                | ExprKind::NullsafePropertyAccess { object, .. } => current = object,
                _ => return,
            }
        }
    };
    walk_nodes(body, &mut |node| {
        let Node::Expr(expr) = node else {
            return;
        };
        match &expr.kind {
            ExprKind::NullCoalesce { value, .. } => record(value),
            ExprKind::FunctionCall { name, args } => {
                let key = php_symbol_key(name.as_str().trim_start_matches('\\'));
                if matches!(key.as_str(), "isset" | "empty" | "unset") {
                    for arg in args {
                        record(arg);
                    }
                }
            }
            _ => {}
        }
    });
    (spans, whole)
}

/// Names every variable the body reads (or writes as a whole) outside a nested scope.
fn read_variable_names(body: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::default();
    walk_nodes(body, &mut |node| {
        if let Node::Expr(expr) = node {
            if let ExprKind::Variable(name) = &expr.kind {
                names.insert(name.clone());
            }
        }
    });
    names
}

/// Names a loop both assigns (`$x = …`) and reads: the read may run, on a later iteration,
/// after a store that lowering reaches only further down the body.
fn loop_carried_names(body: &[Stmt]) -> HashSet<String> {
    let mut carried = HashSet::default();
    walk_nodes(body, &mut |node| {
        let Node::Stmt(stmt) = node else {
            return;
        };
        if !matches!(
            stmt.kind,
            StmtKind::While { .. } | StmtKind::DoWhile { .. } | StmtKind::For { .. } | StmtKind::Foreach { .. }
        ) {
            return;
        }
        let mut assigned: HashSet<String> = HashSet::default();
        let mut read: HashSet<String> = HashSet::default();
        walk_nodes(std::slice::from_ref(stmt), &mut |inner| match inner {
            Node::Stmt(inner) => {
                if let StmtKind::Assign { name, .. } | StmtKind::TypedAssign { name, .. } = &inner.kind {
                    assigned.insert(name.clone());
                }
            }
            Node::Expr(expr) => {
                if let ExprKind::Variable(name) = &expr.kind {
                    read.insert(name.clone());
                }
            }
        });
        carried.extend(assigned.intersection(&read).cloned());
    });
    carried
}

/// Whether a read of `name` that finds no storage yet may still see a value a later store of an
/// enclosing loop leaves for the next iteration.
pub(crate) fn read_may_see_a_later_loop_store(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    ctx.definedness.is_eligible()
        && ctx.definedness.loop_carried.contains(name)
        && is_trackable_name(ctx, name)
        && !ctx.has_local_slot(name)
}
