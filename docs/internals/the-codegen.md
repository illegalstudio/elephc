---
title: "The Code Generator"
description: "How EIR becomes target assembly and links against the shared runtime."
sidebar:
  order: 7
---

**Source:** assembly emitter `src/codegen/`; EIR lowering `src/ir_lower/`;
literal eval planning `src/eval_aot.rs`; IR model and validation `src/ir/`;
shared runtime/ABI support `src/codegen_support/`; optional bridge crates under
`crates/`.

Codegen is a single EIR pipeline. The checked and optimized AST is always
lowered into EIR, IR passes run over that module, and `src/codegen/` emits the
user assembly for the selected target.

## Pipeline Position

Internal value classes can declare operator adapters in their shared
`ClassContract::operators` metadata. Type inference resolves their result types,
and EIR lowering emits ordinary typed calls after evaluating and capturing
operands in source order. Boxed operands branch on the actual runtime class
before scalar fallback. Magician consumes the same metadata. Decimal work stays
in `elephc-bcmath`; the existing target-aware BCMath ABI shapes marshal class
operations on every supported target. Identity comparisons bypass this dispatch.
AST effect and exception summaries retain potentially dispatched arithmetic,
including unused results and calls through mixed parameters. Literal scalar
arithmetic keeps its ordinary folding rules.

```text
PHP source
  -> Lexer
  -> Parser
  -> Magic constants
  -> Conditional compilation
  -> Resolver / autoload
  -> NameResolver
  -> AST constant folding
  -> Type checker / warnings
  -> AST optimizer passes
  -> AST -> EIR lowering
  -> EIR validation
  -> EIR optimization passes
  -> EIR -> target assembly
  -> runtime cache
  -> assembler / linker
  -> binary or cdylib
```

`--emit-ir` stops after lowering and IR optimization, printing the textual EIR.
Normal builds continue through `codegen::generate_user_asm_from_ir_with_options`
and link the resulting user object against the cached runtime object.

## Module Layout

| Path | Responsibility |
|---|---|
| `src/codegen/mod.rs` | Public codegen facade, EIR backend entry points, runtime metadata finalization |
| `src/codegen/block_emit.rs` | Function/block traversal, prologues, top-level entry and deferred EIR wrappers |
| `src/codegen/lower_inst.rs`, `src/codegen/lower_inst/` | Instruction lowering, including typed runtime-target dispatch with no PHP-name lookup |
| `src/codegen/lower_inst/runtime_calls.rs`, `runtime_functions/` | Validates and lowers typed `RuntimeCallTarget` / `RuntimeFnId` operations into target-aware backend implementations |
| `src/codegen/lower_inst/builtins/eval.rs` | Literal-eval AOT bodies, scope synchronization, bridge calls, and dynamic post-barrier dispatch |
| `src/codegen/eval_inventory_helper.rs` | The one module-level helper that seeds a fresh eval context with the AOT metadata inventory |
| `src/codegen/eval_status_helper.rs` | The one module-level helper that turns an eval bridge status into its throw/fatal handler |
| `src/codegen/shared_helper.rs` | Synthetic EIR scaffolding and frames for sequences emitted once per module instead of at every call site |
| `src/codegen/lower_term.rs` | Terminator lowering for returns, branches, switches, and unreachable paths |
| `src/codegen/block_layout.rs` | Machine block order (cold blocks last) and compare-branch fusion eligibility |
| `src/codegen/parallel_moves.rs` | Direct scalar block-argument copies ordered as a parallel move |
| `src/codegen/machine_peephole.rs` | Late per-function machine peepholes over emitted assembly text |
| `src/codegen/context.rs` | EIR function emission state and value materialization helpers |
| `src/codegen/frame.rs` | Stack-frame sizing, local slots, register allocation integration |
| `src/codegen/stack_numeric.rs` | Escape and lifetime proofs for frame-local checked numeric results |
| `src/codegen/value_placement.rs` | Stack/register placement for EIR values |
| `src/codegen/runtime_callable_invoker.rs` | Generated uniform `(descriptor, argument array) -> Mixed` invoker trampolines |
| `src/codegen_support/abi/` | Target ABI helpers for registers, calls, stack slots, symbols, and frame mechanics |
| `src/codegen_support/arrays.rs` | Shared array metadata helpers used by EIR and runtime support |
| `src/codegen_support/callable_descriptor.rs` | Callable descriptor layout, kind constants, and entry-slot loading |
| `src/codegen_support/callable_invoker_args.rs` | Shared descriptor-invoker argument cloning and boxing helpers |
| `src/codegen_support/sentinels.rs` | Null/uninitialized sentinel constants, tagged-scalar helpers, x86_64 heap-header magic |
| `src/codegen_support/value_boxing.rs` | Shared scalar/string/array/object/iterable boxing into runtime `Mixed` cells |
| `src/codegen_support/wrappers/` | Shared callback and fiber wrapper emitters used by deferred EIR wrapper emission |
| `src/codegen_support/runtime/` | Shared `__rt_*` routines and runtime data emission |
| `src/codegen_support/runtime/eval_bridge.rs`, `eval_scope.rs` | C-ABI value hooks and core materialized-scope helpers for eval |
| `src/codegen_support/platform/` | Target descriptions and assembler/linker naming conventions |

The active backend must remain target-aware. New lowering paths should use the
ABI helpers instead of hardcoding AArch64 or x86_64 register and stack details.

## Scalar Constants and Immediate Operands

With the default `--regalloc=linear`, integer, boolean, raw null, and floating-point
constants have reconstruction recipes instead of spill slots. A constant used only
as an immediate emits no standalone definition. Other consumers reconstruct it in
their requested register. Float recipes preserve the exact IEEE bits, including
negative zero and NaN payloads. Strings, heap values, and two-word tagged scalars
retain their existing storage and ownership rules.

`src/ir_passes/rematerialize.rs` estimates the cost of keeping a constant in a
register versus reconstructing it. Encodable immediate uses contribute no register
benefit. Other uses receive a loop-depth weight, capped at three nesting levels;
definition cost, callee-save overhead, and a live-range surcharge offset that
benefit. Expensive constants reused in a loop can therefore stay in registers
after LICM places their definitions outside the loop. Register pressure falls back
to reconstruction rather than a constant spill slot. Functions with implicit
exception edges or generator suspension use recipes without allocating registers.
The explicit `--regalloc=stack` mode retains backing slots as a diagnostic baseline.

Allocation and emission share the encoding rules in
`src/codegen_support/abi/immediate.rs`:

| Operation | AArch64 | x86_64 |
|---|---|---|
| Integer add/subtract and compare | Unsigned 12-bit immediate, optionally shifted by 12; negative constants use the inverse add/subtract or `cmn` | Signed 32-bit immediate extended to 64 bits |
| Integer AND/OR/XOR | Repeated, rotated runs of ones in a power-of-two bit width | Signed 32-bit immediate extended to 64 bits |
| Integer multiply | Register operand | Signed 32-bit immediate extended to 64 bits |

These constraints follow the [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab)
and [Intel instruction reference](https://www.intel.com/content/www/us/en/content-details/825760/intel-64-and-ia-32-architectures-software-developer-s-manual-combined-volumes-2a-2b-2c-and-2d-instruction-set-reference-a-z.html).
Values outside an encoding's range use full-width registers. Commutative operations
can exchange operands; comparisons also reverse their predicate. Loop bounds use
the same comparison selection. Checked integer-to-int arithmetic enters the cold
overflow path with one not-taken conditional branch (`b.vs`, `b.ne` for AArch64
multiply, or `jo` on x86_64); the in-range result falls through. The overflow
path reconstructs an immediate operand only when conversion needs it. Boxed
checked arithmetic follows its runtime call ABI unless its entire lifetime is
proven eligible for a frame-local cell.

Loop-carried numeric locals must preserve an overflow-promoted float on later
iterations. Lowering discovers integer reads of a final Mixed slot inside a
cyclic region, then replays the body with Mixed storage from its first store.
Constraints belong to individual storage bindings, so a name reused after
`unset` retains its new representation. This decision is separate from the
sealed per-call-site closure proof.
`src/ir_passes/numeric_domain.rs` can prove an integer/float domain from a
dominating numeric seed and every writer of a private local. A nonescaping
add/subtract/multiply or numeric increment can reuse a 24-byte frame cell: two integer tags take
checked integer arithmetic, while an existing float remains a float. Both input
payloads are read before the output cell changes. Proven numeric relational
comparisons avoid temporary boxes, compare integer pairs exactly and reject
unordered NaN predicates. Unknown tags, calls, references
and escaping pointers keep ordinary heap-backed operations.

`examples/constant-rematerialization/main.php` demonstrates bounded sample
accumulation with immediate masks, scaling, increments, and a fixed loop bound.

## Frame-local checked numeric cells

Integer destinations expose checked arithmetic conversion in EIR before assigning
call-operand owners. Direct, method, static and statically known callable arguments,
including named and static spread arguments, can therefore use the existing checked
integer sinks. Explicit `int` local declarations apply the same conversion to their
initializer and later arithmetic assignments. Inferred locals retain PHP's dynamic
integer or float result. Only add/subtract/multiply trees with concrete integer
leaves take this path; unknown Mixed sources keep the ordinary boundary conversion.
Names declared inside a branch, loop, exception region or include-once guard keep
their existing dynamic reassignment handling throughout the body. This conservative
boundary prevents a skipped declaration from narrowing a sibling arm or a later
write. Explicit declaration initializers still use their own integer destination.

The conversion consumes the final result of the entire tree. Intermediate overflow
still promotes at the original operation, and later operations use its float value.
Compatible left-associated trees and scalar add/multiply operands around right-hand
chains fuse into scalar checked chains. Other closed numeric trees use the frame
cells described below, without allocating heap cells.

`src/codegen/stack_numeric.rs::plan()` runs before frame sizing and selects owned
add/subtract/multiply results from checked integer or proven numeric Mixed
operations, plus numeric increments or decrements, whose complete pointer lifetime
remains in the current activation. Numeric observations and one ordinary local
are permitted, including a declared integer return check whose result copies the
payload and preserves its TypeError guard. A checked result may also feed another
closed numeric add/subtract/multiply or increment; every input must be proven
integer or float, including all local writers. A float tag remains floating point
on subsequent operations.
Private slots in the executable main frame use the same lifetime proof; globals
retain heap storage. Delayed reads, escaping consumers, aliases, references,
`eval` and suspension fail the proof.

Independent frame cells do not invalidate each other's borrowed reads. A same-block
read must be consumed before its owning slot is stored or released, or before any
producer that can back that slot executes again, even if its next store comes
later. The per-slot invalidation index is bound to the immutable EIR function.
An `OwnedTemp` or `HiddenTemp` call-operand slot may retain one selected cell
through transparent borrows only with a single same-block store, balanced push/pop
cleanup records and final retirement. Other temporary contracts stay excluded.
Property access may still throw while such an index is staged. The
ordinary unwinding dispatcher ignores frame addresses and retains heap-seed
cleanup; neither the catch boundary nor the cleanup records are removed.

Selected values use 24-byte `mixed` cells in the frame, without heap headers.
`stack_numeric::placement` shares storage only between same-block temporaries
whose payload observations do not overlap, following transparent retains/borrows.
The last numeric consumer reads both inputs before writing, so its output may
reuse an input cell at that boundary. Any value stored in a local, including a
scoped operand owner, keeps dedicated storage. Groups never share across blocks.
This bounds transient chain storage by live values instead of expression length,
preserving room for recursion on the fixed Fiber stack.
`src/codegen/lower_inst/arithmetic/stack_cell.rs` writes its runtime
tag and payload directly, using the original operands for overflow-to-float
conversion. Selected local stores retain the cell address, scalar observations
copy its payload, and ownership operations skip heap retirement for these cells.
A cell must never escape, acquire heap
children or reach the heap free path. AArch64 and x86_64 use the same layout
through target-aware frame helpers. This storage plan also applies with
`--no-ir-opt`; it is separate from the EIR bounded-counter pass.

## Short-Circuit Branch Conditions

EIR lowering sends `&&`, `||`, and nested `!` directly to their true and false
blocks when an `if`, `elseif`, loop header, or full ternary consumes the condition.
Each operand is evaluated in source order, and a short-circuit edge skips the
remaining operands. Truthiness conversion also releases any owned operand before
branching. These conditions need no boolean merge slot or associated stores and
reloads, even with `--no-ir-opt`, on every supported target.
Whole logical trees allocate operand blocks in source order, including mixed
operators and negation. Continuations between operands without internal control
flow can fall through instead of jumping backward between operands.

When the logical result is itself returned, assigned, or passed to a function,
ordinary expression lowering still materializes its boolean value. The short
ternary `?:` also keeps its value-producing path because it forwards its left value.

## Late Instruction Selection and Machine Peepholes

After EIR passes and register allocation, the backend applies a final set of
target-aware choices while emitting each function. None of them changes
observable behavior, so they run in every optimizer mode.

**Constant-multiply strength reduction**
(`src/codegen/lower_inst/strength_reduction.rs`). An unchecked `imul` by a
constant (one whose overflow the integer range analysis ruled out, so the result
is defined modulo 2^64) is factored as `±m * 2^k` with `m` equal to `1`,
`2^j + 1`, or `2^j - 1`. Each candidate sequence is priced with a per-target
latency and instruction-count model and is kept only when it beats the multiplier:

| Constant | AArch64 | x86_64 |
|---|---|---|
| `x * 8` | `lsl x0, x0, #3` | `shl rax, 3` |
| `x * 9` | `add x0, x0, x0, lsl #3` | `lea rax, [rax + rax*8]` |
| `x * 31` | `lsl x10, x0, #5` + `sub x0, x10, x0` | `mov r10, rax` + `shl rax, 5` + `sub rax, r10` |
| `x * -8` | `neg x0, x0, lsl #3` | `shl rax, 3` + `neg rax` |
| `x * 11` | `mul` (no cheaper sequence) | `imul rax, 11` |

AArch64 accepts at most two ALU instructions within `mul`'s latency, because
`mul` also needs the constant materialized; x86_64 requires strictly lower
latency than its single 3-cycle `imul r, r, imm32`. Checked multiplies keep the
hardware overflow flag and are not rewritten.

**Operand selection.** Integer compares, AArch64 three-operand arithmetic, and
right-hand x86_64 operands read register-allocated values in place instead of
copying them into the result register first.

**Block layout** (`src/codegen/block_layout.rs`). The first block stays first
(the prologue falls into it). Blocks that can only end abnormally
(`unreachable`, `fatal`, `throw`) move after every other block, so a loop's hot
blocks are contiguous. Each block knows its layout successor: an unconditional
`br` to it emits no jump, and a `cond_br` whose then-block is next branches only
on the inverted condition.

**Compare-and-branch fusion.** When a block ends in `icmp` whose only use is the
block's `cond_br`, and only `nop`s follow it, the compare is emitted by the
terminator and the branch consumes its flags directly (`cmp` + `b.ge` /
`jge`) instead of materializing a boolean with `cset` / `setcc` and testing it.

**Cold paths.** Overflow paths of checked integer arithmetic and fused checked
chains are written to a per-function cold section
(`src/codegen/context/machine_layout.rs`) and appended after the function's last
block, which no block can fall into. The hot path only carries the not-taken
overflow branch, and the cold path jumps back after promoting or reporting.

**Block-argument moves** (`src/codegen/parallel_moves.rs`). Edges whose
arguments and parameters are all `int`, `bool`, or `float` copy values directly
into the parameter homes, ordered as a parallel move; a cyclic dependency (such
as a swap) falls back to the general stack-based copy.

**Machine peepholes** (`src/codegen/machine_peephole.rs`) run over each finished
small or loop-carrying function's text. They delete full-width self-moves and the second move of a
`mov a, b` / `mov b, a` pair, forward a value stored to a frame slot into an
adjacent reload of the same slot, and remove unconditional jumps to the label
that immediately follows. Patterns only look through comments and blank lines:
a label is a join point and a directive is opaque. Conditional branches on
AArch64 reach only ±1 MiB, so far branches are emitted as an inverted short
branch over `b` (`cbz x0, 1f; b L; 1:`). The peephole folds that into one
`cbnz x0, L` / `b.cond L` when `L` is a label of the same function and the
function is provably smaller than the conditional reach. The text peepholes run
on functions that are small (up to 8 KiB of assembly text, typically the callees
of hot loops) or contain a retreating CFG edge (an edge to the same or an earlier block
in the block table), up to 256 KiB. Larger bodies without such an edge and very
large bodies (generated helpers and dispatch code such as an eval-heavy
program's top level) skip them to keep
compile time flat; block layout still removes their jumps to the next block. Overflow code
moved to the cold tail repeats its instruction's `@src` marker, so debug line
info and source maps keep pointing at the originating PHP statement.

`examples/machine-peepholes/main.php` shows a fused loop test, an out-of-line
overflow path, and strength-reduced multiplies.

## Compound Array Operations

For a nested write into an associative array, `ArrayFetchForWrite` ensures the
parent entry exists and diagnoses a float key while normalizing it. The
following `HashGetForWrite` carries a boolean EIR marker indicating that the
same key was already diagnosed. Its target lowering rebuilds the normalized
key for the read without emitting the deprecation again. Missing ordinary
hash reads likewise avoid repeating the float diagnostic while formatting
their undefined-key warning.
Compound hash updates carry the same diagnosed-key marker from the read half
to `HashSet`. The marker also reaches the property and static-property write
paths and the boxed `Mixed` array writer, whose `RuntimeCall` then rebuilds the
key with `__rt_php_float_to_int`, so a compound update on a local, a property,
a static property, or a `mixed` local reports one float-key diagnostic.
Increment and decrement expressions capture the old element once, then
calculate and write the new value. These updates also capture a mutable
dimension after eager right-hand-side evaluation and before the read, so an
error handler cannot redirect the write by changing the source index variable.

A null-coalescing assignment (`$a[$k] ??= $v`) is not a compound update: it
probes the element with `??` semantics and writes only when the probe produced
null, so a present element is never written back. The insert converts a float
key again, and reports that second conversion just as PHP does.

Packed indexed arrays apply the same float-key conversion before `ArrayGet`,
`ArraySet`, and existence probes. Compound writes reuse the read's diagnosis,
including when a key becomes boxed `Mixed` or the array is promoted to hash
storage at runtime. Integral float keys convert without a deprecation.

String indexing returns a null sentinel for a missing offset when lowered for
`isset()`, `empty()`, or `??`, allowing null coalescing to select its fallback
without an out-of-bounds warning. Ordinary reads still return an empty string
and warn. A float string offset is converted to an integer by a warning-marked
`FToI` instruction before `StrCharAt`. A boxed float uses a marked integer cast
that checks its runtime tag and issues the same warning. A boxed variable is
fetched again after its warning handler returns, matching PHP when the handler
changes that variable.

## Runtime Split

Codegen always produces two compiler-owned artifacts:

1. **User assembly** from `src/codegen/`, containing lowered PHP functions,
   methods, top-level entry code, user metadata, and literal data.
2. **Runtime object** from `src/codegen_support/runtime/`, cached by compiler
   version, target, heap size, runtime features, and PIC mode.

The linker may also add optional bridge staticlibs as a third class of input.
For example, dynamic eval links `libelephc_magician.a`; fully AOT literal eval
does not. Bridge archives are discovered and linked by `src/linker/` rather
than emitted by the assembly backend.

Runtime feature selection is derived from the EIR module plus CLI-owned modes
such as `--web`. This keeps ordinary binaries from carrying unused helper
families while preserving deterministic linking.

For `substr()` and `strpos()`, simple string subjects and exact scalar arguments can be read
without acquiring a full copy of the subject. The shared argument planner still controls named
parameter order and defaults. Property reads keep the receiver rooted through the call, and a
substring result is persisted before that receiver is retired. Calls with side effects,
coercions, or spreads use ordinary operand ownership. Final local storage is checked
after lowering; widening a participating string local retries the body with ordinary argument roots.
Persistence adopts a transient concat allocation only when the pointer is the exact base of a
live heap block; substring views and other interior pointers are always duplicated.

## Builtin Boundary

Each PHP builtin surface has one dependency-neutral contract in
`crates/elephc-builtin-contract`. The AOT home file in
`src/builtins/<area>/<name>.rs` joins its `BuiltinSemantics` by stable `BuiltinId`.
Checker validation/result typing, optimizer effects, ownership/aliasing,
runtime/link requirements, callable policy, argument lowering, and EIR lowering
consume that assembled view without repeating signature metadata.

Lowering emits reusable EIR primitives/graphs or an `Op::RuntimeCall` carrying a
typed `RuntimeCallTarget`. `src/codegen/lower_inst/runtime_calls.rs` and the bounded
`runtime_functions/` groups select the concrete target-aware implementation. PHP
builtin names are absent from backend dispatch. Only compiler-resident language
constructs such as `eval`, `isset`, `unset`, `empty`, `exit`, and `die` retain the
separate `LanguageConstructCall` path.

## Reflection Reachability

Synthetic builtin Reflection classes are lowered from EIR reachability rather than emitted as a
fixed full surface in every native program. The EIR lowerer collects Reflection classes found in
value types, object construction, static calls, and method-dispatch candidates, then adds their
parents and implementation owners before lowering concrete methods and property-initializer
thunks. An eval whose source is known only at runtime keeps the full Reflection surface because
its class and method names resolve only then; a literal eval fragment contributes exactly the
Reflection classes its parsed source names (`ir_lower::program::eval_demand`), the same rule the
DateTime alias surface and the AST reachability pass apply to it.

Some Reflection getters materialize a new Reflection object into a `mixed` property slot instead
of emitting an EIR object-construction operation. For those slots, the lowerer also includes the
materialized class when the holder is reachable and the getter name is present in the module's
string data. It repeats this companion scan to a fixed point because a returned Reflection object
can itself hold another materialized Reflection object. This keeps getter-returned objects' method
and `__toString` tables available without pulling the entire Reflection hierarchy into programs
that never call those getters. A getter name assembled at runtime never reaches the string data,
but it can only be called through a runtime-resolved callable — `$holder->$name()`,
`call_user_func([$holder, $name])` and `[$holder, $name]()` all lower to a
`CallableDescriptorInvoke` — so a module containing one treats every getter as named. That cost
falls only on programs that build a holder and make such a call.

## Reflection `__toString()` Dumps

`ReflectionMethod`, `ReflectionFunction` and `ReflectionParameter` answer `__toString()` from a
`__string` slot filled when the owner object is materialized. The dump is rendered at compile time
by `src/codegen/lower_inst/objects/reflection/callable_to_string.rs`, so a compiled program never
walks metadata to print it. Eval materialization also fills the slot so casts work both inside
and after the eval fragment:

- The dumps omit the `@@ <file> <line> - <line>` header: a compiled binary cannot honestly name
  a source that need not exist where it runs. Supported default forms and remaining source
  spelling limits are listed in [the Reflection API documentation](../php/classes.md).
- `elephc_builtin_contract::reflection_format` shares float, string and union formatting with
  eval. Float defaults use 14 significant digits. Value strings escape control and non-ASCII bytes,
  and `iterable` renders as `Traversable|array`. Its `union_member_rank` is also the order both
  engines sort `ReflectionUnionType` metadata with, so `getType()` and `getTypes()` answer in the
  same order as the dump (#1118).
- Method headers distinguish the overwritten parent declaration from the root prototype.
  `ClassInfo::declared_interfaces` retains the direct clauses separately from flattened interfaces:
  the last newly implemented contract supplies the interface prototype, while an inherited method
  keeps its parent's prototype when no new contract applies.
  Deprecated declarations carry the `deprecated` marker, and internal method dumps label
  tentative return types using the shared `reflection_methods` catalog.
- A function or method is marked internal by the same answer `isInternal()` gives, and its module
  comes from the builtin contract.
- `getDeclaringFunction()` reflectors are built without parameters, to avoid an infinite
  `ReflectionParameter` → declaring function → `ReflectionParameter` emission. Their dump is
  rebuilt from the member's name and stored as a string, which carries no such cycle.

A parameter default whose dump PHP prints from its written AST, rather than from its value
(`new Foo(...)`, `Foo::BAR`), goes through
`src/codegen/lower_inst/objects/reflection/default_export.rs`. It exports only the subset PHP
8.5.11 was measured on; anything else falls back to the evaluated value.
AST string exports preserve raw PHP bytes through the compiler's literal-byte representation.
The dedicated dump property emitter decodes it at the final storage boundary; genuine private-use
Unicode names are encoded first so they cannot be mistaken for escaped bytes.
Array defaults are rendered from their source expression to distinguish folded values from ASTs.
Folded values apply last-wins keys and compact lists recursively; arrays inside an AST export
retain all folded keys. Null or fractional keys keep the array's written form instead.
String key identity uses PHP bytes, so equivalent Unicode and byte-escape spellings overwrite
the same key. Constant references left in the AST retain their names without being evaluated.
Global and scoped array-valued defaults retain their written constant name independently of
their resolved value. The constant metadata evaluator also retains nested arrays, including
class constants exposed by declaring-class Reflection objects.
Relative class-name defaults resolve in the method's declaring scope. Trait-relative class names
remain written even through composition and aliases; the EIR module retains imported trait method
keys separately so display scope does not change `getDefaultValue()` binding.
An implicit item after the maximum integer key also retains its written form. The parser keeps
that item as an append entry, and mixed-literal effects preserve its catchable runtime error.

## Eval Lowering Boundary

Literal `eval()` calls reach EIR as `EvalLiteralCall`. The shared planner in
`src/eval_aot.rs` classifies the fragment before target assembly is chosen:

1. Direct-local and fully static plans become native instructions or internal
   EIR functions and require no eval runtime state.
2. Scope-backed AOT plans use `EvalScopeGet`/`EvalScopeSet` and enable only the
   core `eval_scope` runtime feature.
3. Dynamic or unsupported plans materialize `EvalContext`, `EvalScope`, and
   optional `EvalGlobalScope` slots, call the Magician C ABI, and enable
   `eval_bridge`.

The lowerer preserves PHP source evaluation order before ABI materialization,
boxes values into the shared `Mixed` cell representation, and uses the normal
target-aware call helpers on the three executable hosts: macOS ARM64, Linux
ARM64, and Linux x86_64. See [Eval Runtime Architecture](eval-runtime.md) for the
complete boundary.

### Sequences that are identical at every call site are emitted once

Two eval sequences are the same at every site that needs them, and are therefore
module-level helpers rather than inline copies:

- seeding a fresh eval context with the AOT metadata inventory
  (`src/codegen/eval_inventory_helper.rs`), and
- turning a bridge status into its throw/fatal handler
  (`src/codegen/eval_status_helper.rs`).

Both were inlined at first, which made assembly size quadratic in the number of
eval call sites. On `examples/eval/main.php` (169 sites) the inlined form emitted
32.4M lines / 770 MB of assembly and took 477 s to compile on the reference
machine; the hoisted form emits 5.6M lines / 156 MB and takes 81 s, with the same
program output. Their runtime guards never made the duplicate copies execute;
only their presence cost anything. Emit such a sequence once, measure the
assembly, and only then decide it needs its own helper.

A helper built this way has three hard constraints.

Its label must be GLOBAL (`emit_global_shared_helper`). This is not about
reachability — a forward `bl` to a local label resolves inside one object file —
it is about section and atom ownership: `label_global` opens a per-symbol
`.text.<name>` section on Linux so `--gc-sections` can drop the helper
independently and cannot attribute it to whatever function happened to be open,
and it marks the helper `.globl` on macOS so `-dead_strip` treats it as its own
atom.

Its state must not live in a frame-POINTER local: the shared-helper prologue
builds a fixed frame (32 bytes on AArch64, 24 on x86_64) with no room for one, so
a local addressed from the frame pointer would land in the caller's stack. An
`sp`-relative slot is no substitute here, because these bodies displace `sp` while
staging constant values. The eval catalog instead parks its context handle in a
module-level symbol declared in `src/codegen_support/runtime/data/fixed.rs`, which
is deliberately process-global: no registration call executes user PHP or
suspends a fiber, and `--web` workers are separate processes, so the transaction
cannot re-enter itself. A registration path that ever gains a user callback would
have to make that slot per-thread.

A helper CANNOT read the caller's eval scratch frame, because inside it `sp`
belongs to the helper: any `EVAL_*_OFFSET` access would pick up unrelated bytes.
Caller-frame values must be passed as arguments — the status dispatch receives
the pending Throwable in argument register 1 (`x1` / `rsi`) for exactly this
reason. Getting it wrong produced a program that ran, but raised an empty-class
`Uncaught` instead of the real exception.

## Emit Modes

`--emit executable` is the default and emits a process entry point. `--emit
cdylib` emits a PIC user object with `#[Export]` trampolines, ABI/error/memory
helpers, and lifecycle symbols for embedding hosts. Every export uses a native
exception boundary; scalar exports preserve their C signatures and publish
status through `elephc_last_status()`, while string-return exports preserve
their fixed scalar/string inputs, append output pointer/length parameters,
return status directly, and copy the PHP byte string into caller-owned heap
storage. Boundary nesting is explicit and concat scratch state is saved and
restored around each entry. Internal symbols are hidden on ELF (including
driver-supplied `_init`/`_fini`) and private externs on Mach-O, so separate
loaded Elephc modules do not preempt each other's runtime state and Mach-O
`-dead_strip` has real roots to collect against.

## Key Mechanisms

### Mixed boxing

`src/codegen_support/value_boxing.rs` owns the shared emitters that box PHP
values into runtime `Mixed` cells. A boxed cell pairs a runtime tag byte
(0 int, 1 str, 2 float, 3 bool/false, 4 array, 5 assoc array, 6 object,
7 mixed/union/iterable, 8 null) with the payload; tag values and payload
register conventions must match `__rt_mixed_from_value`. Owned boxing paths
transfer or release references so payloads are never double-freed. `Union(...)`
and `Iterable` values reuse the same boxed representation.

### Staging a builtin's integer arguments

A builtin whose integer argument may arrive BOXED cannot load its arguments in
ABI order. Unboxing calls `__rt_mixed_cast_int`, which clobbers every
caller-saved argument register, so any argument already loaded would be
destroyed by the unboxing of a later one.

The rule is: resolve every boxed integer argument FIRST, park each on the stack,
load the remaining arguments into their ABI registers, then pop the parked ones
back — in reverse staging order, since the stack is LIFO. An argument that is not
boxed is loaded in place, which keeps the common path identical to a builtin that
never needed staging at all.

`array_fill()` is the worked example
(`src/codegen/lower_inst/builtins/arrays/fill_helpers.rs`): `$start` and `$count`
both go through `stage_boxed_fill_integer` / `settle_fill_integer`, and the
string fill helper's `(count, ptr, len)` ABI — which puts the count in the FIRST
argument register rather than the second — is settled the same way.

Which representations need staging:

- `Int` / `Bool` — no. One register, loaded directly.
- `TaggedScalar` — no. The payload lives in the value's own slot and the null tag
  in the adjacent one, so loading the slot into an ABI register already yields the
  integer. A `?int` that is genuinely `null` still reaches the builtin as its
  sentinel payload rather than PHP's "passing null is deprecated, treated as 0",
  which is a gap in the tagged-null surface and not in the staging.
- `Mixed` / `Union` — yes. `resolve_int_operand_to_result` unboxes them through
  `__rt_mixed_cast_int`, which is the call that forces the ordering.

A local whose value came from checked integer arithmetic is `Mixed`, because the
product or sum may overflow to float: `$n = $this->w * $this->h` is boxed while a
literal, a parameter, or the same expression folded at compile time is not. That
asymmetry is why a missing stage looks like a defect that only happens inside a
method (issue #502).

### Callable descriptors and invokers

`PhpType::Callable` stays one pointer wide, but the pointer targets a
descriptor (`src/codegen_support/callable_descriptor.rs`) whose entry slot is
loaded before invoking native code. Descriptors record the callable kind
(closure, first-class callable, callback adapter, object invoke, plain
function, builtin, extern, static method, instance method) plus
signature/default/by-ref/variadic metadata and capture/receiver environment,
without changing the one-word callable ABI. The optional invoker slot points at
a generated uniform adapter (`src/codegen/runtime_callable_invoker.rs`) with
the ABI `(descriptor, argument array) -> Mixed`. The invoker saves and restores
the caller's callee-saved registers it scratches — `x19`-`x26` on AArch64 and
`r12`, `rbx`, `r13`-`r15` on x86_64 — in a dedicated frame save area.

The EIR module fixed point can specialize an already-direct numeric closure
call when every actual argument, including captures, is exactly `int` or
`float`. `src/ir_passes/numeric_closure_calls.rs` recognizes bounded
straight-line numeric bodies and emits private scalar-argument variants with
the original owned Mixed return ABI. This avoids argument boxing and the
callee's private Mixed shadow. It does not establish closure identity or admit
new call sites: lowering's existing per-site proofs remain authoritative.
Bool, null, references, variadics, attributes, unknown calls and nonnumeric
storage reject specialization. The original Mixed closure and descriptor stay
available, and every specialized signature uses the shared target-aware ABI.
Each expression node is emitted once, and every independent owned intermediate
is released after the final result is ready. A private variant retains its PHP
source name, lexical scope and instruction spans. Monitoring entries share the
source identity: instrumentation ids and counter slots aggregate the original
and specialized entries, and probe/source/backtrace metadata keeps the original
PHP name with the physical native symbol. The initial scalar family excludes the
generated Array argument collector used for func-args/backtrace support.

### The reserved nested-call register

Method dispatch (direct, virtual, interface and nullsafe), closure and callable
descriptor invocation, dynamic construction, `__set` dispatch, the boxed-Mixed
`__toString` ladder and builtin callback dispatch hold a receiver or descriptor
in one reserved register across their argument-lowering calls: `x19` on AArch64,
`r12` on x86_64 (`abi::nested_call_reg`). It is callee-saved and outside the
register allocator's pools, so `frame::layout_for_function` gives a frame a save
slot for it, and the prologue and every epilogue save and restore it with the
allocator's own callee-saved registers.

The slot is the default. A frame skips it only when every local is a one-word
scalar and every instruction is one of the register-neutral opcodes in
`frame::op_is_nested_call_reg_neutral` (scalar constants, integer and float
arithmetic, comparisons, int-to-float promotion, moves, `Nop` and
`ConcatReset`), whose lowerings stay inline and make no call that returns to the
frame. Calls into runtime helpers do not qualify: some x86_64 helpers use `r12`
as scratch. Deciding it the other way round, by
listing the users, missed a direct method call on a typed receiver: a destructor
calling `$this->close()` while an exception unwound returned into
`__rt_exception_cleanup_frames` and the frame cleanup callback with the register
they keep their own state in overwritten, and the process died with SIGSEGV. The
generator entry wrapper and native hosts keep state in the same register across
calls into PHP code.

### Inline wrappers and branch reach

Some lowerings emit a wrapper or invoker in the middle of the enclosing
function and jump over it: the `array_map()` and `preg_replace_callback()`
descriptor callback wrappers, the direct-callback argument adapter, and the
runtime-callable invokers. Their `label_global()` opens the wrapper's own
`.text.<symbol>` section on ELF, so the lowering captures
`Emitter::current_text_section()` before the wrapper and calls
`reopen_text_section()` after it. Without that, the rest of the enclosing
function continues inside the wrapper's section, and a conditional branch from
the function into that tail becomes a cross-section relocation whose distance
depends on the linker's section layout.

On AArch64, `b.cond` and `cbz`/`cbnz` encode only a ±1 MiB displacement
(`tbz`/`tbnz` only ±32 KiB). An edge that jumps over code whose size grows with
the program, such as the runtime-name case tables of callable dispatch, uses
`abi::emit_branch_if_equal_wide()`: an inverted `b.ne 1f` over an unconditional
`b`, which reaches ±128 MiB. x86_64 `jcc` relaxes to a rel32 displacement and
needs no widening. `tests/codegen/callables/branch_reach.rs` pins both rules on
the runtime-selected callable paths.

### Static and global storage

Function `static` locals are `.comm` symbols (`_static_<fn>_<name>`, 16 bytes)
paired with a one-time init-marker symbol (`<symbol>_init`); the `--web` reset
generator walks these records to release and re-arm persistent statics between
requests. Static properties live behind per-class user-data symbols
(`crate::names::static_property_symbol`), with late-bound `static::` receivers
resolved through native class-id branches. `global` variables load and store
through `_eir_global_<mangled_fqn>` symbols emitted for the EIR `LoadGlobal` /
`StoreGlobal` instructions.

### Call-stack overflow guard

`src/codegen/stack_guard.rs` emits a stack-depth check into every compiled
function, method, closure, and generator body prologue, immediately after the
frame has been reserved (so the compare already accounts for that frame) and
before the incoming parameters are spilled. It is an unsigned comparison of the
stack pointer against the runtime `_stack_limit` word, branching to
`__rt_stack_overflow` when the pointer is below it:

- **AArch64** (macOS and Linux): `adrp` / `ldr` / `cmp sp, x9` / `b.hs <local>` /
  `b __rt_stack_overflow` — five instructions, clobbering only `x9`. The
  conditional branch stays local because `b.cond` only encodes a ±1 MiB
  displacement, which large programs exceed; the unconditional `b` reaches
  ±128 MiB and gets linker veneers beyond that.
- **x86_64**: `cmp rsp, QWORD PTR [rip + _stack_limit]` / `jb __rt_stack_overflow`
  — two instructions, clobbering nothing.

The check never writes memory, so it is safe to run when the remaining stack is a
single page. A zero `_stack_limit` makes it always pass, which is the state
before `__rt_stack_limit_init` runs and whenever the stack bounds could not be
determined — the guard is inert rather than wrong. `main` is not guarded: it is
the root of every process-entry call chain and runs before the floor exists.
Cdylibs publish the floor from `elephc_init()` because they have no process
entry point. See [The runtime](the-runtime.md) for the floor measurement and
the fiber handoff.

### Sentinels and null representation

`src/codegen_support/sentinels.rs` is the canonical home for the in-band
sentinel constants and tagged-scalar helpers. Under the default
`NullRepr::Tagged` mode, null-capable scalar slots use the inline two-word
`{payload, tag}` `TaggedScalar` representation, making the full i64 range
representable; the legacy `--null-repr=sentinel` opt-out stores the in-band
`NULL_SENTINEL` (`PHP_INT_MAX - 1`), which collides with that real integer.
Uninitialized typed properties use a separate sentinel (`PHP_INT_MAX - 2`)
stored in the property's metadata word, never the value word, so it cannot
collide with property values. On x86_64, heap headers carry the `ELPH` magic
in the high 32 bits of the kind word: every stamp goes through the shared
`x86_64_heap_kind_word` helper and every check compares against
`X86_64_HEAP_MAGIC_HI32` — local copies of either constant are forbidden.

### Nullable optional string lengths

`substr()`, `substr_replace()`, and `substr_count()` share one PHP contract for
their optional `$length`: omission and `null` mean “through the end”, while `0`
means an empty window (or pure insertion for `substr_replace()`) and a negative
integer keeps its PHP end-relative meaning. The backend must therefore inspect
null before applying the ordinary weak integer conversion, because PHP's
general `(int) null` result of `0` is not the argument contract for these
builtins.

Named-argument planning can materialize an omitted nullable default as a null
EIR operand, so operand count alone is not authoritative. String builtin
lowering classifies the operand as absent/static null, concrete, inline
`TaggedScalar`, or boxed `Mixed`/union. Runtime-null tagged and boxed values are
mapped to the through-end path; concrete zero and negative payloads continue
through numeric length normalization unchanged.

The three builtins use different final encodings:

- `substr()` is lowered inline and maps runtime null to a saturating positive
  length before clamping it to the remaining string.
- `substr_replace()` passes `i64::MAX` to `__rt_substr_replace` for omitted or
  null length. `-1` is never an omission sentinel because it is a real PHP
  negative length.
- `substr_count()` cannot use the saturating value because its explicit-length
  bounds check would raise `ValueError`. It preserves null status separately
  and selects `subject_length - resolved_offset` before validating concrete
  lengths.

For boxed values, the weak-integer helper keeps the original `Mixed` cell
pointer in an aligned temporary stack slot across `__rt_mixed_unbox` and
`__rt_mixed_cast_int`, preserving existing float-coercion diagnostics. The
normalized payload and null/int tag use `x0`/`x1` on AArch64 and `rax`/`rdx` on
x86_64. `substr_count()` copies the tag to `x7` or `r11` only after conversion
calls have finished, then restores the subject, needle, and offset ABI
registers before window validation.

## Property Writes Through a Runtime-Typed Receiver

A property write resolves its slot at compile time from the receiver's class. When the receiver is
statically `mixed` or an object union there is no class to resolve, so the class is read at runtime
and the slot chosen from it.

Both spellings of the write dispatch the same way, on the payload's **class id** — the object
header's first word:

| write | lowering | chain |
|---|---|---|
| `$o->name = $v` | `lower_mixed_static_prop_set` | one compare per class in the build that declares `name` and can hold this value |
| `$o->{$expr} = $v` | `lower_runtime_mixed_prop_set` | the same compare, then a name comparison per candidate |

A match stores straight into that class's declared slot. A **stdClass** payload and a **non-object**
payload both fall through to `__rt_mixed_property_set`, which writes a dynamic property for the
first and drops the write for the second — PHP's *attempt to assign property on non-object*.

Until issue #1094 only the runtime-name spelling dispatched. The static-name spelling went straight
to `__rt_mixed_property_set`, whose class-id check only admits stdClass, so `$o->n = 9` through a
`mixed` receiver was discarded in silence for every other class while `$o->{$p} = 9` on the same
object wrote. The receiver does not have to be spelled `mixed` to reach it: a local the checker
widened across a loop back-edge (`$o = null; for (...) { $o = new T(); $o->n = 9; }`) is boxed too.

### Ownership: the release belongs to lowering

`emit_property_store` **retains** what it stores, on both of these paths — the class-id dispatch
and `materialize_dynamic_property_mixed_value`'s fresh Mixed cell — so `PropSet` never consumes its
source. A declared-receiver write is therefore followed by an explicit EIR release
(`release_property_assignment_source_after_retaining_store`), gated on the source being an owning
temporary.

A runtime-typed receiver never got one: `object_property_type` answers only for a statically known
class, so the gate returned `None` and an owning temporary assigned through a `mixed` receiver
leaked once per write — three blocks for an array literal and its two boxed elements.
`release_runtime_typed_property_assignment_source` closes that, with the same ownership gate.

The property's declared type is the one fact lowering does not have, because the payload's class is
not known until runtime. It matters only for the transfer `property_store_keeps_independent_ref`
excludes — a `Mixed` value into a `Mixed` slot, where the backend hands the source's own cell to the
property instead of retaining — so a `Mixed`-typed source is left alone.

Doing this in the backend instead does not work, and the reason is worth keeping: whether the SSA
source owns a reference is not a property of its type. A backend release gated on types alone
destroyed the property's own reference for a *borrowed* source, which reads back as a pointer and
trips `heap debug detected bad refcount`.

### Reflection constant values

Static Reflection metadata folds constant expressions at compile time in
`reflection_constant_value` (`src/codegen/lower_inst/objects/reflection/names_constants.rs`),
which is distinct from the pure `resolve_const_default` resolver in
`src/codegen/const_default_values.rs`: the reflection folder returns the recursive
`ReflectionConstantValue` metadata enum, while the pure resolver returns the
target-neutral `ConstDefaultValue` consumed by eval-bridge default registration
and descriptor invokers. The two are not interchangeable.

Supported forms: scalar literals, `self::`/`parent::`/`ClassName::` scoped
access with ancestry, global `const` references, `::class`, negation, the
binary arithmetic and concatenation operators, `null`, unit/backed enum cases,
indexed and associative array literals, and mixed array literals. A mixed
literal (`ArrayLiteralMixed`) folds each entry in PHP evaluation order: bare
elements take the next free integer slot, explicit keys are kept, and a
spread's integer keys are renumbered into a contiguous block starting after the
destination's highest integer key (`0` when it has none) while its string keys
are kept in place. The implicit key after a negative integer key starts at `0`
for PHP 8.0–8.2 and advances from that key for PHP 8.3+, following the selected
compile profile. The parser preserves those bare entries, and AST folding normalizes
spread-free mixed literals through `src/types/array_literals.rs` before their consumers.
Unpacked pure arrays share the resolver in `src/codegen/const_default_values/arrays.rs`
for native inventories, eval registration and callable defaults. Array keys
normalize with PHP's rules (integer strings, `false`, truncated floats, `null`
to `""`). Float keys use PHP's modulo integer conversion, including non-finite and
out-of-range values. Unsupported class-constant metadata reports the unsupported form;
parameter defaults retain the existing lazy-helper and unavailable-value fallbacks.

Literal string keys and values decode to PHP bytes at the storage or wire boundary;
metadata names keep their ordinary UTF-8 encoding. Native array-default records retain
binary string payloads and keys without requiring valid UTF-8, while class and parameter
names still use the checked UTF-8 codec. AOT AST literals retain lexer markers until
this boundary; Magician `EvalConst::String` and `EvalConst::Bytes` already hold decoded
PHP data and must not be encoded again. Synthesized `::class` literals protect genuine
private-use Unicode characters from the byte-escape marker representation.

The folded values emit through `emit_reflection_constant_value_as_mixed`
(`default_emit.rs`), which materializes a fresh boxed `Mixed` array (recursive,
mixed payload slots) for array forms. Consumers: `ReflectionParameter`
defaults, `ReflectionClass::getConstant()` / `getConstants()`,
`ReflectionClassConstant::getValue()`, and enum backing values.

## Backend Contract

- PHP-visible behavior belongs in `src/ir_lower/` and `src/codegen/`.
- Shared runtime, ABI, platform, and metadata helpers belong in
  `src/codegen_support/`.
