# Strict profile foundation record

This records the verified foundations and numeric boundaries summarized at
step95. Later corrections and current completion work are tracked in the
[active implementation plan](strict-semantic-profiles.md), its commit ledger,
and Code Journal architecture document3739892b. Statements about pending work,
interpolation representation and cache formats below describe that historical
checkpoint, rather than the current implementation.

## Verified foundations

- The dependency-neutral elephc-semantics crate owns the immutable ABI1 profile,
  sixteen-feature catalog, independent request resolution, scalar decisions and
  exact key classification. Specific positive/negative requests override the
  umbrella regardless of order, with last-wins explicit requests. PHP-only naming
  has no legacy CLI alias.
- Profiles travel through frontend options/results, EIR modules and contexts,
  backend/runtime generation, eval contexts and host capability checks. Physical
  SourceMode, PHP strict_types and public builtin invocation contracts are separate.
  Internal implementation operations retain explicit reviewed PHP semantics.
- Positive and negative parsed-source/executable caches partition by effective
  semantic identity, PHP-only visibility and PHP version. Eval source provenance,
  disk headers and versioned host setters agree. Magician disk format is 30.
  Runtime build fingerprints include semantic source. EIR EvalSourceProfile uses
  a checked compact u32 so Instruction remains within its 112-byte budget.
- DynamicCode audits every syntactic branch, resolver/autoload/include boundary,
  recursively verified literal eval, EIR fallback, opaque execution and REPL.
  Statically eligible literal eval remains AOT; runtime source is refused.
- Bools covers known and dynamic conditions, short-circuit/xor/ternaries, loop and
  iterator conditions, predicate callbacks, walker/VM, native hosts and cached REPL.
  Owned predicate/iterator failures clean up through catches and destructors.
- String interpolation still lowers to ordinary concat tokens. Preserve explicit
  formatting intent before admitting string-only concatenation.

## Verified numeric implementation

- Exact signed literal provenance distinguishes deliberate floats, integer
  overflow and the minimum signed magnitude. Both compiler/eval lexers and parsers
  retain the required source evidence, including interpolation offsets and caches.
- Compact ArithmeticPolicy travels explicitly in NumericOperation, NumericUpdate,
  NumericUnary and NumericFunction targets. Native capability does not select an
  operation's semantics. Module validation, effects, ownership, printing and
  optimizer transformations retain the policy.
- Scalar IOverflows guards, boxed exact operand classification, integer power,
  division/modulo, unary normalization and updates preserve PHP kinds. Arithmetic
  and Coercions independently guard original numeric domains, while Overflow
  independently changes integer promotion into catchable ArithmeticError.
  Negative integer powers are rejected only by Arithmetic. Bit operations retain
  their reviewed wrapping/shift contracts. IEEE floats, signed zero and NaN bits
  remain governed by their explicit operations.
- Source validation precedes folding, retains full numeric-string provenance,
  declaration/default/call evidence, named/spread planning and constant cycle
  guards. AST effects and exception-flow summaries preserve warnings, user callback
  writes and discarded failures. Public Arithmetic tests audit original evidence before the initial fold. The
  remaining numeric features retain their own admission audits.
- Unary plus is an explicit AST node, traversed by every structural visitor and
  rebuild/default/reflection/synthetic adapter. Typed EIR unary normalization uses
  independently owned boxes and preserves native object precedence. Direct scalar
  identities require matching logical kinds and physical representations.
- Pow shares the operator graph for every active numeric policy. Native class
  result contracts, original kinds, exact integer results, named/spread/FCC and
  indirect calls agree across frontend, native and eval execution.
- NumericParameters descriptors preserve original builtin arguments before binding
  can coerce them. Prepared public wrappers and direct calls share rooted EIR
  guards; Internal and permissive signatures remain unchanged. Abs accepts int or
  float under numeric domains, while intdiv requires int. Intdiv validates each
  known parameter even when its companion is unknown. Its value failures remain
  catchable in all profiles.
- Sum/product use typed NumericFunction policy for every numeric guarantee.
  Profiled native entries receive numeric bits as a third internal ABI argument,
  held in a frame slot checked against the exception record. They guard elements
  after explicit native to_number adapters, select checked versus promoting
  combination from caller bits, and share the existing source snapshot/unwind
  loop. PHP and Overflow-only entries retain their old ABI.
- Aggregate checker/source proofs retain concrete element contracts, partial final
  entry evidence, overwrite/key identity and native Number casts. An arithmetic
  object overload alone does not authorize numeric aggregation. Eval snapshots
  the array and roots source, element, numeric view and partial carry. Identity
  numeric views are borrowed; fresh casts and temporary iteration keys are retired.
- Outlined boxed operators partition trusted Internal and user callers when a
  profile is active. Only public builtin invocation adapters carry the builtin
  wrapper flag. Native comparison dispatch uses an explicit Internal contract.
- Exact typed literal-key planning preserves original string bytes/kinds and
  append provenance in source reduction proofs. This is groundwork; the complete
  runtime key, producer, array type and indexing guarantees remain outstanding.

## Verified value contracts and declarations

Steps40-52 share exact scalar, nominal and structural membership through the
neutral conversion planner, full compiler type adapters and typed EIR guards.
These contracts distinguish false from bool and exact alternatives from physical
boxed storage. Invalid known PHP/LFC bindings fail before permissive assignability;
Internal operations retain their reviewed behavior. PHP strict_types remains
independent, including its ordinary int-to-float exception.

Parameters, declared and inferred returns, conditional closures, local stores,
initializers, defaults and public descriptors retain original source kinds.
Runtime guards run after source-order operand evaluation with owners rooted,
before coercion or representation extraction. Each declared local store consumes
CheckResult.local_store_contracts instead of success-only narrowed flow facts.
Exact union membership includes nominal objects, interfaces, callable descriptors,
arrays, iterables, resources and structural contracts. Reusable condition graphs
preserve their original rejection class and noninteger union alternatives.

Steps53-57 retain producer expressions rather than only prepared values. Original
call proofs consume CallArgPlan, static associative spread expansion and actual
trait/inherited-constructor signatures. Expr.source_profile and producer_binding
survive copying and folding. Producer binding occurs once under its physical
source policy, including declaration defaults, omitted arguments, property
initialization, class constants and global constants. Identity and legal widening
have explicit binding metadata; lowering does not globally override synthetic
expression policy. Raw defaults remain available for reflection.

GlobalConstants stores (Expr, PhpType), not an isolated ExprKind. The same declared
constant materializer serves globals, classes, local consumers and public default
arguments, preserving signed zero, strings and conditional original kinds. Resource
alias constants keep their special handling. Effects, exception summaries and
frontend walkers preserve all required failures before initial folding.

Native verification covers both optimizer settings and clean heaps, with selected
profile checker/EIR evidence for every supported target. Staged integration
fixtures currently check permissively and then select the module profile; they
prove runtime guards, not selected-profile inference. The separate root tests use
the actual selected checker and remain required evidence.

## Verified explicit numeric conversions

Steps58-61 share NumericCastTarget/Input/Failure, checked_numeric_cast and exact
radix conversion kernels. Finite float-to-int truncation checks range before
conversion. Complete decimal strings preserve exact endpoints, fractional and
exponent truncation without a binary64 round trip. Full-string grammar rejects
junk and invalid bytes; explicit floating results retain IEEE infinity, NaN and
signed zero. Radix parsing validates complete digits before reporting range,
including PHP prefixes and base0 octal. Unknown string bases defer proof, while
nonstrings intentionally ignore the base.

Original-source validation runs before folding and retains strings in its evidence
pool. CheckedNumericCast and CheckedRadixInt EIR targets carry exact types,
capability requirements, effects and validator contracts. Target-aware runtime
entries share decimal/radix parsers and catchable failures. PHP string result
registers differ from C arguments, especially x86_64 rax/rdx versus rdi/rsi; the
ABI string_result_regs helper owns that materialization. Darwin failure branches
use an inverse local branch followed by a wide global jump. Real clang assembly,
PIC/dead-strip checks and emitter tests cover all five targets.

Magician walker and VM scalar conversions use the same pure Rust kernel.
native_results::finish transfers protected native Throwable ownership and keeps
success/unsupported states independent of pending errors. Reviewed object adapters
run before scalar conversion. Rejected results, source views and scratch owners
are retired on every failure. VM positional-call tests use an unreachable same-name
declaration to admit their shape; unsupported VM syntax is tested through the
walker. FakeOps has a separate permissive prefix/precision gap tracked by journal
task dc79bdf2, so controls do not claim unsupported PHP equivalence.

Step62 gives intval/floatval explicit cast semantic descriptors, a shared
BuiltinLoweringContext::emit_numeric_cast boundary and common rooted EIR graphs.
The optional intval base applies only to strings. CallArgPlan owns static named
and spread source proofs. Direct, first-class and indirect eval dispatch select
the same checked behavior before native fast paths. Checked conversions retain
MAY_THROW effects, source-order leases and original operand kinds.

Step63 converts numeric settype values before reference writeback. Potentially
failed ordinary local conversions retain tagged storage and conservative views,
so catch blocks still read the original value. Authoritative local contracts and
existing reference storage remain active. Eval with_direct_ref_target owns
nonescaping array keys/receivers through conversion, writeback and cleanup; its
owner capture must not be applied blindly to escaped generic reference metadata.
Known unsupported targets and dynamic target-domain parity still require audit.

Step64 adds CheckedNumericProjection to the shared native class contract. Final
internal adapters expose original GMP decimal digits and SimpleXML text before
legacy numeric adapters narrow them. The compiler and eval validate this source
with the shared kernel, retaining original receivers, projected strings and
adapter-result owners through failure. GMP floating conversion preserves get_d
truncation after source proof. SimpleXML bypasses public __toString overrides.
Typed intval/floatval calls share syntax adapters; AST usage and reachability root
the original source methods before EIR selects them. Real native preparation
requires core, GMP/XML, DOM, object-cast and function-argument preludes in order.

The permissive erased SimpleXML floatval fallback retains its preexisting public
string override behavior; journal task 693969cd tracks that separate baseline
mismatch. Checked profiles use original native text. Managed GMP/XML bundles,
walker/VM owner tests, all-target selected checker/EIR tests, compiler gates and
builtin-docs regeneration pass. The public mask remains unchanged until complete
numeric cast, settype, source and host guarantees are enforced.

## Native eval boundaries and ownership

Protected checked_numeric_v1, numeric_unary_v1 and warning_v2 hooks contain native
exceptions beneath Rust frames. Success transfers one owner; unsupported requests
leave outputs untouched; failures use pending Throwable ownership and status2.
Unary requests validate selector/capability subsets before reading pointers.
Numeric operands are snapshotted before warning callbacks; C ABI spills, operand
pins, normal call versus jump-entry stack alignment and cleanup must remain sound
on every target. No Rust unwinding may implement a PHP Throwable.

Step39 closes invalid checked aggregate sources. A native regression was red on
step38: sum/product receiving an int returned status0 with a null result. Shared
source diagnostics and normal call-entry failure helpers now produce protected
TypeError status2 before result publication, preserve the output slot and borrowed
input, and transfer the pending Throwable correctly. The new five-target emitter
unit and native regression pass, along with three boundary/owner controls, the
compiler build, architecture gate, assembly comment checks and diff hygiene.
