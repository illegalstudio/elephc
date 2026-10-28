---
title: "Optimization and codegen controls"
description: "Controls that affect generated code quality and shape: the EIR optimization passes (--ir-opt), the register allocator (--regalloc), and the null representation (--null-repr)."
sidebar:
  order: 5
---

elephc optimizes in two places: over the AST before lowering, and over EIR after
lowering. The AST optimizer is always on. The EIR-level controls below let you
turn passes off for benchmarking and diagnostics, and choose between code-shape
trade-offs.

## Two optimization layers

- **AST optimizer** — PHP-preserving rewrites expressed over syntax: constant
  folding, constant propagation, control-flow pruning and normalization, and
  dead-code elimination, followed by conservative whole-program declaration
  reachability that removes unused functions, classes, and methods before EIR
  lowering. Dynamic lookup and Reflection widen its keep-set, while the
  prelude-backed `--with-pdo`, `--with-mysqli`, `--with-tz`, and `--with-image`
  requests, `--with-eval`, and every `#[Export]` function are roots. Always on; not behind a flag. See
  [The Optimizer](../internals/the-optimizer.md).
- **EIR optimization passes** — transformations that need value identity, basic
  blocks, or dominance, which the AST cannot express well. Run by a fixed-point
  pass driver after lowering. Controlled by `--ir-opt`. See
  [The EIR Design](../internals/the-ir.md#optimization-passes).

Declaration reachability favors correctness over minimum size. Runtime-computed
functions, methods, classes, callables, aliases, and Reflection may therefore
retain a wider declaration family than the eventual runtime value needs. On
macOS, linker atom splitting remains limited to the runtime object: generated
user metadata and address-taken callable labels can rely on one contiguous user
object, so user declarations are removed by the AST pass instead of by
`.subsections_via_symbols`.

Callback-consuming AOT builtins expose their callback slots through structural
shared-contract metadata, independently of the PHP parameter name and its broad
storage type. Registry validation rejects unknown or duplicate callback slots, and
registry-wide tests pin the complete current set and require every conventional
`$callback` parameter to resolve through structural metadata or a callable type.
Mixed callback parameters with other names remain explicit semantic metadata because
their role cannot be inferred from the PHP storage type alone.

Operations that invoke PHP protocols implicitly, including `foreach`, `count()`,
object offset access, `json_encode()`, and iterator helpers, keep the corresponding
protocol methods behaviorally reachable. Known receiver classes produce
class-qualified edges; opaque receivers widen only those protocol method names,
and recursive JSON encoding conservatively keeps `jsonSerialize` by name.
Declared array/scalar locals and positive `is_array()` guards avoid treating an array-only path as object dispatch.
Attributes on reachable declarations are scanned at every supported location,
including fixed and variadic parameters.

## EIR optimization passes

After the AST is lowered to EIR and validated, a fixed-point pass driver runs the
registered transformation passes over each function until none reports a change.
The representation-changing pipeline runs to a **module-level fixed point**: each
round runs the cross-function [small-function inliner](#small-function-inlining)
and then drives the per-function passes to convergence on every function,
repeating until neither the inliner nor any of those passes changes anything.
Interleaving lets the two layers feed each other — inlined bodies expose new
constants and dead code, and the simplified functions expose new (smaller) inline
candidates. Passes that set `run_after_fixed_point` are partitioned out of that
loop. [Fresh Mixed assignment transfer](#fresh-mixed-assignment-transfer) runs
after `optimize_module_to_fixed_point` returns, so removing an assignment clone
cannot restart the inliner or the checked-integer sink. The passes are **on by
default**.

```bash
# Default: EIR optimization passes enabled
elephc hot.php

# Disable them (for A/B comparison or diagnostics)
elephc --no-ir-opt hot.php
elephc --ir-opt=off hot.php

# Explicit enable
elephc --ir-opt=on hot.php
```

The environment variable `ELEPHC_IR_OPT=off` disables the passes for a whole run
without editing each command.

In debug and test builds the driver re-validates each function after **every**
pass and aborts if a pass produced malformed IR, so optimization bugs surface
immediately during development. In release builds those checks are compiled out.

### Identity arithmetic folding

The first registered pass folds algebraic identities on integer and float
arithmetic and bitwise operations:

| Pattern | Result |
|---|---|
| `x + 0`, `0 + x`, `x - 0` | `x` |
| `x * 1`, `1 * x`, `x / 1` | `x` |
| `x \| 0`, `x ^ 0`, `x << 0`, `x >> 0` | `x` |
| `x & x`, `x \| x` | `x` |
| `x * 1.0`, `x / 1.0` | `x` |
| `x ^ x`, `x - x`, `x * 0`, `x & 0`, `x % 1` | `0` |

Only PHP-equivalent rewrites are applied: integer `x / 0` and `x % 0` are left to
trap at runtime, and float additive-zero (`x + 0.0`) and `x * 0.0` are excluded
because signed zero and `NaN` make them observable.

You can see the effect directly with [`--emit-ir`](output-and-diagnostics.md#--emit-ir):

```bash
# With passes on, `$argc * 1` folds away; with --no-ir-opt it stays an `imul`.
elephc --emit-ir app.php
elephc --emit-ir --no-ir-opt app.php
```

This is a peephole-level optimization. It speeds up code that contains redundant
identity operations in hot paths and is a no-op on code that does not — unlike
register allocation, which helps broadly.

### Peephole patterns

The second registered pass applies local rewrites that clean up the shape of
lowered EIR. Each is refcount-balanced and produces output identical to PHP:

| Pattern | Rewrite |
|---|---|
| Box/unbox cancellation | `unbox(box(x))` → `x` for scalar (`NonHeap`) payloads |
| Redundant `move`/`borrow` | a forwarding op whose result has the same ownership and type as its operand folds to the operand |
| Load/store forwarding | a `load` of a scalar local right after a `store` to it reads the stored value directly |
| Dead store | storing a scalar local the value it already holds is removed |
| Acquire/release cancellation | an `acquire` whose result is consumed only by its `release` drops both |
| String-literal concat folding | `concat("a", "b")` → the single literal `"ab"` |

Load/store forwarding and dead-store removal apply only to non-aliased scalar
locals, so reference semantics and copy-on-write are never affected. The
remaining patterns only fold when ownership and type are preserved, so cleanup
and refcounting stay balanced.

You can see the effect with [`--emit-ir`](output-and-diagnostics.md#--emit-ir):
`$x = $argc; echo $x;` forwards the load so the `echo` reads the stored value and
the `load_local` becomes a `nop`.

### Scalar local promotion

The third registered pass (`mem2reg`) replaces loads and stores of eligible
`int`, `bool`, and `float` PHP locals with SSA values. Block parameters carry
values across branch joins and loop back edges, so register allocation can keep
loop counters and accumulators in registers.

Promotion requires plain local loads and stores, a value initialized before every
read, and no escaping reference. Parameters, globals, static locals, heap-backed
values, and volatile slots stay in memory. Functions using generators, exception
handlers, or dynamic eval are skipped. See
[Scalar Local Promotion](../internals/the-ir.md#scalar-local-promotion).

### Immutable local loads

The fourth registered pass marks `load_local` instructions that read a
proven-immutable concrete integer slot as pure, so later passes (CSE, LICM) may
deduplicate or move them. A slot qualifies when it is a read-only incoming
parameter (such as `main`'s `$argc`) or has a single entry-block store that
dominates every load. The pass changes only effect metadata, never values.

### Checked-integer sinking

The fifth registered pass specializes a boxed checked-arithmetic operation
(`ICheckedAdd`/`ICheckedSub`/`ICheckedMul`) to its `IChecked*ToInt` form when
every use of its result observes only the integer payload, removing the
transient boxed `Mixed` allocation without changing overflow semantics.
The same proof removes numeric local-slot retirement immediately before an
overwrite, or at the end of a binding when no later access to the slot is
reachable (a counter whose name a later straight-line assignment re-binds).
Retirement remains intact when a read, reference alias, or a value
with a possible destructor prevents proving that the clear is unobservable.

It also unboxes a loop bound written as `$n - c` with a literal `c` from 1 to 1024
when its only uses are strict `$i < $n - c` comparisons against an integer. An
overflowing `$n - c` is the float -2^63 in PHP, below every integer; the unboxed
form yields `PHP_INT_MIN` there, which every integer also fails to be below. A
non-strict `<=` keeps the PHP comparison, because PHP's `$i <= -2^63` holds for the
513 integers from `PHP_INT_MIN` to `PHP_INT_MIN + 512`.

### Bounded mixed counters

The sixth registered pass replaces a boxed function-local counter with a scalar
integer only when every update is proven to fit in the signed 64-bit range. It
first rewrites a private trial, requires complete SSA promotion, and commits the
change only after integer range analysis removes every new overflow check.

Integer seeds, increments/decrements, comparisons and eligible by-value `mixed`
call parameters can participate. References, escaping values, dynamic `eval`,
suspension, exception handlers, uncertain type changes and unproven overflow
keep their existing representation. Call targets and ABIs are unchanged. See
`examples/bounded-closure-counter/main.php`.

Two shapes that PHP code uses for pair loops are proven as well. A counter
compared with a checked difference (`for ($i = 0; $i < $n - 1; $i++)`, also with
`<=`) keeps PHP's comparison for the bound, which may be the float an overflow
produced, while the counter itself becomes an integer: the bound can only overflow
below every integer, so the counter stays at most `PHP_INT_MAX - 1` inside the loop.
A counter seeded by checked arithmetic (`for ($j = $i + 1; $j < $n; $j++)` inside
the `$i` loop) is proven together with the counter its seed reads, and only when
range analysis shows the seed itself never overflows. Lowering then records that
seed as an integer, exactly as for `$j = 0`. See `examples/pair-loops/main.php`.

Numeric locals reused by nested loops can keep independent bindings when every
read belongs to a region initialized by its own assignment or `for` initializer.
A read between those regions, an unknown value, a reference or dynamic scope
access keeps the shared binding. This storage decision also applies before the
optional EIR passes. See `examples/reused-loop-counters/main.php`.

### Guarded loop counters

The seventh registered pass adds an integer copy of a canonical loop when a
static proof alone cannot cover an inclusive or Mixed bound. Entry checks require
integer seeds and invariant integer bounds. Ascending `<=` bounds must be below
`PHP_INT_MAX`; descending `>=` bounds must be above `PHP_INT_MIN`. Float bounds
and unsafe boundary cases use the original loop, preserving PHP's overflow and
integer-versus-float comparisons. Every normal fast exit publishes the counter's
final integer, so code after the loop sees the same value.

A dead-at-exit seed such as `$n - 1` in a descending `>= 0` loop can instead use a
single guard that skips the loop if the seed would overflow. Condition effects
and observable final seed values prevent this shortcut. References, dynamic
scopes, suspension, exception handlers, changing bounds and escaping SSA values
remain conservative. Versioning is limited to small innermost loops and four
copies per function.

### Canonical loop optimization

The eighth registered pass replaces self-carried scalar loop parameters with their
preheader values, exposing invariant bounds. It also shares equivalent integer
induction variables: every back edge must use the same constant step and the
initial values must agree. Checked recurrences must have identical overflow modes,
and the retained check must execute before the removed check. They remain checked
until integer range analysis proves overflow impossible.

Removing duplicate loop parameters and incoming arguments lets dead-code
elimination discard redundant updates. Integer comparisons place the induction
operand first, and self-comparisons exposed by counter sharing become constants.
The recurrence analysis is shared with integer range specialization. Different
initial values or steps, path-dependent updates, exception handlers, and suspended
generators are conservatively excluded. See `examples/loop-optimization/main.php`.

### Integer range and induction-variable analysis

The ninth registered pass propagates signed 64-bit integer intervals through
constants, branch comparisons, block parameters, and arithmetic. A PHP comparison
of an integer with a checked `X - c` or `X + c` bound also bounds the integer by
one step on the edge where the bound's one-sided overflow cannot be the reason the
test holds. Constant-step
loop counters can inherit bounds from their initial values and loop conditions.
When every possible result stays within the integer range, checked addition,
subtraction, and multiplication become ordinary scalar operations.

Boxed numeric results can also become scalar values when all consumers preserve
their storage and conversion semantics. Unknown ranges, unsupported control flow,
exception handlers, and ambiguous null-sentinel payloads retain the checked or
boxed path, including PHP's overflow-to-float behavior. The shared
`boxed_narrowing` helper also constrains constant folding; it is not a separate
registered pass. See [The Optimizer](../internals/the-optimizer.md).

### Checked numeric-chain fusion

The tenth registered pass fuses a left-associated chain of boxed checked
addition, subtraction, or multiplication when its only observable result is an
integer cast. The chain must stay within one basic block, and every boxed
intermediate may be used only by the next arithmetic operation plus removable
release bookkeeping. The replacement `ICheckedNumericChainToInt` instruction
keeps in-range values in scalar registers. At the first overflow it promotes
the exact accumulator and operand to PHP float semantics, evaluates the
remaining suffix in order, and applies the normal float-to-int conversion once.

Chain fusion can also cross private local storage, including the data local in
a versioned integer loop. It follows each writer to every read before overwrite,
requires a dominating writer on every incoming path, and preserves suffix
arithmetic through the final cast. Conditional overwrites and retirements that
reach a shared read prevent the rewrite. For
`$cell = $row * $columns + $column; (int)($cell + 1)`, the overflowed float must
continue through `+ 1` before converting to an integer. Generic loop copies can
keep their boxed storage while the fast copy eliminates its unobserved store.

### Constant folding

The eleventh registered pass folds operations whose operands are all compile-time
constants into a single constant, in place. It covers integer arithmetic
(`iadd`, `isub`, `imul`), bitwise ops, in-range shifts, unary `ineg`/`ibit_not`,
float `fadd`/`fsub`/`fmul`/`fneg`, signed integer comparisons (`icmp`), and the
`is_null`/`is_truthy` predicates. A constant value in EIR is the same constant at
every use, so a single forward scan discovers constant operands and collapses
chains like `(2 + 3) * 4` in one sweep.

Each fold reproduces exactly what the op's lowering computes at runtime, so the
compiled result never changes: integers wrap at 64 bits (matching the native
`add`/`sub`/`mul`), shifts fold only for counts in `0..=63`, and the trapping
integer division/modulo and `NaN`-sensitive float division are left untouched.

Together with the peephole's scalar load/store forwarding — which moves a
constant assigned to a local onto its later uses — this realizes per-block
constant propagation over EIR value ids *and* local slots: the peephole carries
the constant through the slot, and this pass folds the constant-operand
operation built on it. Constants surfaced by identity arithmetic feed it too:
`($argc * 0 + 5) * ($argc * 0 + 5)` reduces to `$argc * 0` → `const_i64 0`
(identity), then `0 + 5` → `5`, then `5 * 5` → `const_i64 25` (this pass), with
the three `imul`s eliminated.

### Common subexpression elimination

The twelfth registered pass removes a pure computation when an identical one is
already available on every path to it, redirecting its uses to the earlier
value. It does both per-block and cross-block elimination in one dominator-tree
value-numbering traversal: a scoped table maps each pure instruction's
`(op, result type, immediate, operands)` to the value that first computed it, and
because blocks are visited in dominator-tree order the table holds exactly the
definitions that dominate the current block. A match is therefore an earlier,
dominating value, so the redirection is safe; the redundant instruction becomes a
`nop`.

Only pure (side-effect-free) instructions that have at least one operand and a
`NonHeap` or `Persistent` result are eligible — their value depends on their
operands alone and they carry no owned-heap cleanup, so the rewrite is
refcount-neutral. SSA operands are equal-by-value, so identical pure ops on
identical operands compute identical results. Bare constant and address
materializations (`const_*`, `data_addr`) are *not* deduplicated: they are
cheaper to rematerialize than to keep live, so CSE only targets computations.
Functions that use exception handling are skipped, because a dominator over the
terminator graph can be bypassed at runtime by a throw to a handler.

Combined with the peephole's load forwarding, `($n + 1) * ($n + 1)` loads `$n`
once and computes `$n + 1` once. The redundant instructions it neutralizes leave
dead operands that dead-instruction elimination then removes.

### Loop-invariant code motion

The thirteenth registered pass moves a pure computation whose operands do not change
across a loop out of the loop body and into the loop's preheader, so it runs once
instead of every iteration. It builds the loop forest on the dominator tree, then
for each loop grows an invariant set to a fixed point: an instruction is invariant
when each operand is either defined outside the loop (its definition dominates the
preheader) or is itself being hoisted.

Only pure instructions with a `NonHeap`/`Persistent` result are hoisted, including
constant bounds, update steps, and address materializations. Purity means the
result depends only on the operands and the
op neither reads mutable state nor faults, so evaluating it once in the preheader
(unconditionally, even if its original block ran only on some iterations) is safe.
Loops are processed innermost-first, so a value invariant in several nested loops
moves all the way to the outermost preheader. Loops without a detected preheader,
and functions using exception handling, are skipped.

Standalone constant and address materializations move only when their block
dominates every loop back edge. Constants used only by conditional work stay
near their uses, avoiding extra entry work and register pressure in short or
zero-trip loops. A constant needed by an invariant computation can still move
with that computation, including from a conditional block.

Scalar local promotion exposes invariant source expressions as SSA computations,
and immutable integer-local loads can also become pure operands. Expressions
that still read mutable or aliased slots remain in the loop.

Raw integer-slot checked arithmetic retains its original execution point because
its overflow path can terminate the program. Only explicit integer-cast overflow
conversions are safe to speculate; proven non-overflowing operations can move after
integer range analysis removes their checks.

### Dead instruction elimination

The fourteenth registered pass computes CFG liveness and neutralizes unused
result-producing instructions whose effect metadata says they are pure. This
cleans up dead values exposed by earlier EIR rewrites. For example, identity
folding can turn `$argc + 0` into `$argc`; dead-instruction elimination then
drops the now-unused `const_i64 0` from optimized EIR.

The pass is deliberately conservative. It keeps read-only, allocation,
mutation, refcounting, output, warning, fatal, throw, and deopt-capable
instructions even when their results are unused. The goal is to remove dead pure
arithmetic and literal scaffolding without changing PHP-visible behavior or
ownership cleanup.

You can compare the shape directly:

```bash
elephc --emit-ir app.php
elephc --emit-ir --no-ir-opt app.php
```

### Dead store elimination

The fifteenth registered pass removes `store_local` writes whose value is never read
before the slot is overwritten or the function exits. It computes backward,
CFG-aware liveness over local slots (a `load_local` makes a slot live, a
`store_local` kills it) so a dead store is dropped even when the overwrite is in a
later block. Unlike the peephole's per-block, value-equality store drop, this pass
is liveness-based and crosses block boundaries, so it removes a store of a
*different* value whose result is never observed.

It is restricted to non-refcounted scalar locals that are accessed only through
plain `load_local`/`store_local` and never escape by reference. Refcounted slots
are wrapped by separate `acquire`/`release` ownership ops, and by-reference call
arguments or closure captures alias the slot through its `load_local`, so both are
left untouched to keep reference counting and aliasing semantics intact.

### Branch simplification

The sixteenth registered pass simplifies the control-flow graph:

- **Constant-condition folding** — a `cond_br` whose condition is a constant
  (`const_bool`, non-zero `const_i64`, or `const_null`) becomes an unconditional
  `br` to the taken edge; a `switch` on a constant scrutinee folds to its matching
  case. A `while (true)` loop, for instance, lowers to a constant `cond_br` that
  this fold collapses.
- **Empty-block jump threading**: compose incoming SSA arguments through blocks
  that only forward control. Argument permutations and parallel edges retain their
  original values. A parameter used outside the forwarding terminator keeps its
  defining block. Forwarding cycles are left unchanged.
- **Loop block merging**: merge an unconditional successor with one predecessor
  into its caller when both belong to the same natural loop. This removes separate
  update-block jumps while preserving headers, shared continue targets, and nested
  loop boundaries.
- **Unreachable-block removal** — blocks no longer reachable from the entry are
  neutralized (terminator set to `unreachable`, instructions to `nop`).

Functions that use exception handling are skipped, because their handler blocks
are reachable through implicit edges that the terminator graph does not show.

### Concat scratch-state optimization

The seventeenth registered pass (`src/ir_passes/concat_reset.rs`) removes or
coalesces statement-boundary `concat_reset` operations when dataflow proves no
scratch string can be live or consumed between resets. A forward analysis drops
a reset that already finds `_concat_off` at the frame's `concat_base_offset`.
A backward analysis drops an earlier reset when every outgoing path reaches a
later reset before any instruction can allocate, read, or observe concat
scratch, throw, or leave the function.

Calls, output, `eval`, exception-handler entries, and stores or releases that
may run a destructor stay barriers. Arithmetic-only and concrete-array loops
can lose redundant resets inside the body while keeping one before the loop.

### Small-function inlining

As part of the module-level fixed point, a cross-function pass inlines calls to small
user functions directly into their callers; the per-function passes then optimize the
expanded bodies, and because the pipeline iterates, a callee that only becomes small
enough after that optimization is inlined on a later round. A callee is inlined only
when **all** of the following hold:

- its body is at most **24** non-`nop` instructions;
- it is **non-recursive**, directly *or* mutually — a call-graph cycle analysis
  excludes any function that can reach itself, and a per-caller fuel cap backstops
  termination;
- it contains no exception-handling ops and is not a generator or fiber wrapper;
- it exposes a **destructor-free boundary and body**: no by-reference or variadic
  parameters, and every parameter, the return value and every local slot has a
  destructor-free type — scalars (`int`/`float`/`bool`/`string`) and arrays/unions
  whose elements are themselves destructor-free. Objects, packed classes, closures,
  resources, generic `iterable`/`mixed`, buffers, and ref-cell/static/global/capture
  locals are excluded.

The last rule is a correctness requirement, not just a heuristic. The splice replaces
the callee's `return` with a jump, bypassing the callee's implicit epilogue cleanup, so
the inliner reproduces that cleanup's per-slot decisions: parameter slots and
directly-returned slots (which the callee excludes — the argument is borrowed and the
return value's ownership moves to the caller) are transplanted so the host epilogue
ignores them, while ordinary refcounted internal locals are still freed by the host
epilogue. The only residual difference is *when* those internal locals are freed
(deferred to the host epilogue), which is unobservable precisely because the types are
destructor-free — no `__destruct` runs and arrays carry no observable identity. Types
that can run a destructor (objects, or arrays/mixed that may hold them) and by-reference
parameters (which alias caller storage) are therefore excluded, because a value-copy
splice cannot reproduce their cleanup timing or aliasing. As with all EIR passes,
inlining is gated by `--ir-opt` and changes only performance, never observable behavior.

The same pass also inlines **instance-method calls it can bind statically**, so small
accessors and predicates in hot loops cost what the inline code would:

```php
final class Table
{
    private buffer<Slot> $slots;
    public function matches(int $index, int $components): bool
    {
        return $this->slots[$index]->alive
            && ($this->slots[$index]->mask & $components) === $components;
    }
}

function scan(Table $t, int $n): int
{
    $hits = 0;
    for ($i = 0; $i < $n; $i++) {
        if ($t->matches($i, 3)) { $hits++; }   // spliced: no call, no vtable load
    }
    return $hits;
}
```

A `$receiver->method()` call is a candidate when the receiver's static type is a plain
(non-nullable) user class and the call cannot reach a different body at run time: the
class is `final`, the method is `final`, or the class has no vtable slot for it. The
exception is an instantiated generic receiver with object type arguments and a vtable slot:
covariance and contravariance can select a different monomorphized specialization, with its
own lexical class and state, so even a final class or final method keeps a real call.
Scalar type arguments cannot widen in place, so a final `Buffer<int>` or `Buffer<float>`
can use direct dispatch and inlining. The method body
then has to pass the function rules above (the `$this` receiver is the one object they
accept, as a borrowed argument) plus a few of its own:

- it is not static, magic (`__*`), deprecated, by-reference-returning or variadic, and has
  no by-reference parameter and no literal `false`/`null` parameter type (those carry a
  call-site argument guard the splice would skip);
- every instruction is **frame-independent**: constants, locals, scalar arithmetic and
  comparisons, `buffer<T>` element reads and writes, property reads and writes on `$this` or packed
  records, string concatenation/append/length, ordinary function calls, nested method calls
  on `$this`, and static calls to an explicit user class. Nested calls retain their own
  source spans for diagnostics. Lexical `self::`/`parent::`/`static::` calls, other late static
  binding, static variables, `func_get_args()`, closures and eval scopes keep the call;
- property-writing methods retain their real call boundary inside class initializers,
  whose property-store rules are specific to that synthetic frame. The recursion graph
  also follows statically bound instance and explicit static calls through bodies that
  cannot themselves be inlined;
- a source local assignment of an acquired concatenation keeps its real boundary, because
  that store implicitly resets the callee's string scratch. Direct returned concatenations
  remain eligible;
- the program does not observe PHP frames: when anything can call `debug_backtrace()` (or
  the eval bridge is linked), every method call stays, so backtraces never lose a frame.

The splice keeps what a real call does. A receiver that is null at run time still raises
`Call to a member function m() on null` before the body runs (the check is omitted only
for the host method's own `$this`). The spliced instructions still execute in the method's
class scope, so private and protected property reads and writes resolve exactly as inside the
method, even from a free function or another class. The method's locals are renamed so
they never show up in the caller's eval scope or `get_defined_vars()`. The receiver is
only borrowed, so destructors run where the real call would have released the object.

An extra receiver retain can also disappear when an unchanged frame owner already
keeps the object alive. This covers concrete by-value parameters and locals with
one owning entry assignment that dominates every read. Reassignment, reference
aliases, special storage, eval and uncertain initialization retain their leases.

### Tail recursion

With `--ir-opt` enabled, direct tail calls in recursive free functions can reuse
one native stack frame. Self recursion branches back to a body entry with new
parameter values. Mutually recursive functions share cloned EIR bodies within
each original ABI entry, and their tail transitions become ordinary branches.

```php
function gcd(int $a, int $b): int {
    if ($b === 0) { return $a; }
    return gcd($b, $a % $b);
}
```

Arguments keep their source evaluation order. Rebinding uses simultaneous block
argument moves, so swapping parameters is safe. Named and default arguments use
the normal call planner before the tail pass. `void` calls immediately followed
by a return can also qualify. Arithmetic or output after a recursive call keeps
an ordinary call and its stack frame.

Eligibility is conservative: parameters, return types and local storage must be
`int`, `bool` or `float` (`void` is also allowed for returns). Temporary boxed
numeric arguments qualify only when their roots have a balanced cleanup on every
normal return path; their ordinary ABI integer conversion runs before those roots retire.
Owning strings, arrays, objects and arbitrary `mixed` frame storage, reference
or variadic parameters, methods, closures, generators, static/global aliases,
exception handlers and unresolved dispatch retain ordinary calls. Eval or
backtrace argument snapshots anywhere in the module disable this optimization.
Mutual components are limited to 16 functions and 4096 total EIR instructions
per entry to bound code duplication; self recursion has no cloning limit.

The pass runs after the normal module fixed point and before ownership
finalization. The scalar passes run again on the new loops. It uses the same EIR
branch lowering on every supported target and is disabled by `--no-ir-opt`.
See `examples/tail-recursion/main.php` for Euclid and a deep mutual countdown.

### Buffer loops

With `--ir-opt` enabled, a loop over `buffer<T>` elements resolves the buffer
handle once in front of the loop instead of at every `$b[$i]`, and the packed
field accesses of one iteration share a single bounds-checked record address.
The resolution runs before the loop-invariant values the optimizer moved in
front of the loop are loaded, so a value such as `$dt` below stays in a
register through the loop on every target, including Linux x86_64, whose ABI
has no float register that survives a call:

```php
function step(buffer<Particle> $p, int $count, float $dt): void {
    for ($i = 0; $i < $count; $i++) {
        $p[$i]->x = $p[$i]->x + $p[$i]->vx * $dt;
    }
}
```

Diagnostics are unchanged. A stale handle still dies with `use of buffer after
buffer_free()` at the first access, an out-of-range index still dies with
`buffer index out of bounds`, and a loop that never runs its body reports
nothing. The ordinary bounds check uses one unsigned comparison, which rejects
negative indexes as well as indexes at or above the logical length. A zero-based,
unit-step loop guarded by strict `< buffer_len($b)` can omit that comparison for
the exact `$b[$i]` access in a distinct body block dominated by the continuation
guard. An access in the loop header itself keeps the comparison because it may
execute before a header-local guard. The length may be read directly in the
condition or cached before the loop, but it must come from the same stable buffer
handle. A cached length and an in-loop access may use separate loads of one local
handle only when that local cannot escape by reference and is never assigned
anywhere in the function.
Inclusive bounds, unrelated or other-buffer lengths, negative starts, decreasing
or non-unit steps, and adjusted indexes such as `$b[$i + 1]` keep the check.

The loop must not be able to free, allocate or rebind a buffer: a
`buffer_free()`, an assignment to the buffer variable, output, a warning, a
user-code throw, or a call to anything that may do one of those inside it keeps the
ordinary per-access resolution, because user code could free the buffer through
another handle. A call to a user function or method whose whole effect summary
only reads state (or ends in a fatal error) does not block the rewrite.
Generators, functions with `try`, dynamic `eval`, and programs that call
`pcntl_async_signals()` are left alone.

An integer accumulator such as `$found++` or `$sum += $b[$i]` also permits
hoisting when the compiler proves every value stored in it is an integer or
float. Overflow still promotes the accumulator to float. Allocating or retiring
these numeric boxes cannot invalidate a buffer; a value that may contain an
object, an escaped reference, or a conversion that can warn keeps the ordinary
safety restrictions. Casts to int or bool qualify only for operands already
known to be ints: float range checks and NaN-to-bool conversions can run a
warning handler.
Implicit bool checks of float or boxed numeric values retain the same barrier.

Loops over a buffer property of `$this` or another invariant object receiver can
also share one preheader read and resolution when no call, property write, free
or other invalidating operation can change that property. This includes reads
left by inlined accessor methods. Buffer acquire/release are word operations,
and plain `$this` reads that may throw without running PHP do not invalidate
descriptors.
A body-only read moves behind a duplicate of the scalar entry condition, so a
zero-trip loop never reads an uninitialized property. Conditional reads,
faulting prefixes and header values used after the loop keep their original
form. A foreign buffer read must reach its checked access before any observable
operation or branch, so a null-receiver warning cannot resume execution with
an invalid cached descriptor. Its first checked access stays in the guarded
entry. See the [buffer property loop example](../../examples/buffer-property-loop/main.php)
and the [direct and inlined object example](../../examples/buffer-object-loop/main.php).

Checked accessors can cache their stable state, length and storage reads too.
For a bounded straight-line entry chain, the optimizer repeats the initial scalar
loop condition, then evaluates the accessor's first checked prefix in source order.
Cold throwing branches retain the initial index, message, source span and cleanup.
Only a successful entry reaches the loop with cached properties, so empty loops
do not access closed or uninitialized owners. Later index checks remain at their
accesses. Calls, property mutation and other invalidating operations keep the
ordinary path. See the [checked owner example](../../examples/checked-buffer/main.php).

A method that reads the same buffer property of `$this` several times shares
that work too, inside or outside a loop:

```php
public function matches(int $index, int $components): bool {
    return $this->slots[$index]->alive
        && ($this->slots[$index]->mask & $components) === $components;
}
```

The property is read and its handle resolved once, the second access to the same
index reuses the first record address, and accesses to other indexes share one
resolution. Reads from different inlined class scopes stay separate, so a parent's
private buffer never aliases a shadowing private property in the child. Anything
between the reads that could change the property or free
the buffer keeps them apart: a store to that property or `unset()` on any
object, a `buffer_free()`, any function or method call, a release, a warning or
output. Guarded scalar reads from inlined `$this` accessors can share a value
across the same unchanged region. Other object property reads remain barriers unless they are buffer reads
immediately consumed by a checked access: a null read then returns handle zero
and the access fails before execution can continue. A change to the index variable keeps the record addresses
apart. Programs where a class declares `__get`, or that evaluate code at run
time, keep every read. See `examples/buffer-property-table/main.php`.

Packed field reads and writes are also known to be plain loads and stores, so
the loop index and locals stored into packed fields stay in registers and no
`concat_reset` is kept between statements. The rewrite is target-independent and
disabled by `--no-ir-opt`. See `examples/buffer-loops/main.php`.

### Fresh Mixed assignment transfer

`FreshMixedTransfer` (`src/ir_passes/fresh_mixed_transfer.rs`) is registered with
the other passes, and `run_after_fixed_point` keeps it out of
`optimize_module_to_fixed_point`. After the inliner and the
representation-changing passes have stopped, `optimize_module` runs this
finalizer phase once, through the ordinary per-function sweep. A change inside
the phase repeats the finalizer until a sweep reports no change. That
repetition stays inside the finalizer. The inliner and
[checked-integer sinking](#checked-integer-sinking) have already finished.

The finalizer removes a `MixedClone` of a fresh owned `ICheckedAdd`,
`ICheckedSub`, `ICheckedMul`, or dynamic `MixedNumericBinop` add/sub/mul result
when that result is used only by the
clone and by the next non-`nop` instruction in the same block, which must be
the `Release` of the source. Uses of the clone result are redirected to the
source, and the clone and that release become `nop`. The checked operation
stays in place, including overflow-to-float promotion and dynamic int/float
representation. Dynamic power and other producers remain excluded. Borrowed, acquired,
shared, and otherwise observed sources, and a release in another block, keep
the copy.

The clone is a `Mixed` use, so the sink does not treat the producer as an
integer-only observation while the copy is still there. Removing the clone
inside the fixed point would leave the remaining integer cast or store looking
like a sink, and a later sweep would rewrite the producer to `IChecked*ToInt`.
The transfer runs only after that sink has finished, which keeps the overflow
on the boxed `IChecked*` operation. `--ir-opt=off` skips `optimize_module`, so
the original copy remains. The proof and the adjacency rules are in
[The EIR Design](../internals/the-ir.md#fresh-mixed-assignment-transfer).

Later releases extend this driver with further EIR passes.

## Frame-local checked numeric cells

During frame layout, the backend can place addition, subtraction or multiplication
results from checked integer or proven numeric Mixed operations, plus numeric
increments or decrements, in a reusable stack cell instead of a heap allocation.
The cell retains the normal `mixed` integer/float tag and payload, including the
operation's overflow-to-float result.

This requires a proof that all observations are numeric and the cell cannot
escape or be read after its producer runs again. Private slots in the executable
main frame use the same lifetime proof. Parameters, globals, statics, references,
`eval`, suspension and unknown consumers keep ordinary storage.
This backend storage choice applies with EIR optimization both on and off and
uses the same layout on every supported target. See
`examples/numeric-local/main.php` and
[the codegen description](../internals/the-codegen.md#frame-local-checked-numeric-cells).

## Late machine optimizations

After the EIR passes, the backend makes a final round of target-aware choices
while it emits assembly: constant multiplies become shift/add/`lea` sequences
when that is cheaper on the target (`x * 31` becomes `(x << 5) - x`), loop and
`if` tests branch directly on integer and ordered relational float compare flags
instead of materializing a boolean, jumps to the next block and redundant
register moves are removed, and the
overflow paths of checked integer arithmetic are moved after the function's last
block so hot loops fall straight through. These choices never change program
behavior and apply with `--ir-opt` on or off. Inspect them with `--emit-asm`; see
[The Code Generator](../internals/the-codegen.md#late-instruction-selection-and-machine-peepholes)
for the cost model and the exact patterns.

## Register allocation

The EIR backend uses a linear-scan register allocator (Poletto–Sarkar) by
default, keeping hot scalar values in registers across calls instead of spilling
them to the stack on every use. Scalar constants use reconstruction recipes:
cheap values are rebuilt at each use, including as a target-legal immediate,
and a constant stays in a register only when its estimated loop savings exceed
materialization and live-range cost. `--regalloc=stack` keeps backing slots as
a diagnostic baseline. See
[Scalar Constants and Immediate Operands](../internals/the-codegen.md#scalar-constants-and-immediate-operands).

```bash
# Default: linear-scan registers
elephc hot.php

# Fall back to stack-only placement (spill everything)
elephc --regalloc=stack hot.php
```

`ELEPHC_REGALLOC=stack` applies the fallback to a whole run. The stack fallback
exists mainly for comparison and debugging; linear scan is substantially faster
on compute-heavy code.

Functions containing exception handlers and generators automatically use stack
placement even in linear mode. The allocator's current liveness analysis does
not model implicit exception edges or values preserved across generator suspension.

## Null representation

`--null-repr` selects how null-capable scalar slots are stored:

| Value | Meaning |
|---|---|
| `tagged` (default) | Inline two-word `{payload, tag}` scalars. |
| `sentinel` | In-band `PHP_INT_MAX - 1` sentinel in one-word slots (compatibility opt-out). |

```bash
elephc --null-repr=sentinel sentinel.php
```

`ELEPHC_NULL_REPR` overrides the default for a whole run. Most programs should
use the default; `sentinel` exists as a compatibility opt-out. See
[Memory Model](../internals/memory-model.md).
