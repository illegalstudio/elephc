# Strict profile admission history through109

These are the former detailed implementation notes from the active strict plan.
Each section describes its verified checkpoint and the admission status at that
time. The [active plan](strict-semantic-profiles.md) owns current guarantees,
outstanding work and the complete commit ledger. Historical pending language
must be read in that checkpoint's context, not as the current build status.
Code Journal plan revisions retain the original chronological snapshots.

### Current casts and reference implementation

Checked numeric/string conversion plans, native object-number adapters, original
GMP projections, settype target aliases and collection targets, prepared scalar
builtin graphs and protected contextual warnings are implemented. Current
invocation policy stays distinct from physical Internal producer syntax. Builtin
contract ownership, callback effects and result kinds remain shared across direct,
named, dynamic and first-class calls.

Prepared writable places capture their original binding with LocalPlace markers
at source argument position and read a rooted current-value snapshot after all
arguments finish. Owned instance scalar, nullable, union, object, array and
Iterable cells share canonical promotion and immutable declaration validation.
Generic receiver reloads use the canonical reader; raw eval identity reads retain
their separate ABI. Exact Mixed retains its established metadata flag, while
reviewed constrained payloads use bit10. Projection authenticates resolved shape.
Raw Iterable metadata uses tag13 and a sentinel-safe leaf reader reused by JSON,
serialization and hydration, preserving arrays, hashes and Traversable values.

Boxed static bindings reserve24 bytes and register their physical declaration
once. Layout2 includes canonical promotion and weak-source registration; repeated
selection preserves generation. Retirement detaches metadata before PHP cleanup.
Source references and eval exports share the descriptor selector and wrapper
identity. The versioned eval reference replacement boundary validates through the
same coordinator and transfers the displaced owner back to Rust before snapshot
cleanup can throw, preserving eval metadata-update/release ordering.

Public Arithmetic and Overflow are independently admitted. Their native fusion,
warning, callable, source-fold and eval boundaries retain checked policies and
ownership. The other profiles and the umbrella remain gated as described above.
Detailed historical steps63 through87 remain in their Git commits, earlier plan
revisions and architecture document revision76; the full commit ledger below
retains their traceability. The approved contract and remaining delivery scope
above are unchanged.

### Concrete static storage and live conversion references

- The shared static storage planner keeps original declarations in ClassInfo,
  selecting boxed physical storage for reviewed user statics under Casts. Native
  reads project the declared ABI from that box; validators still use the original
  int, float, bool, string, nullable, object or Iterable contract.
- Assignment and Reflection ownership planning, eval getters/exports, and
  process/web owner enumeration consume the same physical policy. Typed scalar
  statics now own boxes and participate in cleanup just like constrained unions.
- Literal defaults also follow the physical policy in the initializer prologue,
  which bypasses ordinary EIR static stores. They retain their original declared
  conversion before their resulting owned native value is boxed for storage.
- Native fixtures cover inherited aliases, conversion and rejected replacement,
  declaration-preserving scalar results, object/Iterable values, direct Mixed
  reference functions, Reflection, self-assignment, missing fields and GC.
- The physical policy unit, selected-checker/EIR generation and export checks on
  all five targets, three native static bundles with both optimizer settings and
  clean heaps, two permissive Reflection/string lifetime controls, architecture828
  and assembly/diff checks pass. The fixture converts an array and separately
  replaces it with an iterator: pre-existing ArrayIterator array-cast behavior is
  tracked independently as task97375be9.
- Before Casts admission, complete protected direct static setter publication,
  including unbound declaration coercion and initialization visibility during
  callbacks. Concrete checked eval setters currently return a guarded miss rather
  than write through a raw-layout handler. The public profile mask is unchanged.

### Validated static publication and initialization

- Direct checked static stores prepare a canonical binding even before an alias
  exists. Uninitialized bindings retain their sentinel during conversion callbacks;
  an accepted static commit publishes the child and initialization before cleanup.
- Native, deferred-eval and initialized-static publication share the same rooted
  validation body. A typed retirement plan selects old-value cleanup, transfer to
  Rust or static initialization; it does not duplicate declaration coercion.
- Static store effects conservatively include arbitrary PHP conversion and cleanup
  callbacks while excluding invented direct blocking/network I/O boundaries.
- Selected-checker/EIR and effect tests, commit/publisher units, four static cell
  tests including all-five-target real assembly, protected reference wrapper
  assembly, three native static bundles and five publication ownership controls
  pass. Native cases cover both optimizer modes with clean heaps.
- Direct Reflection fixtures verify unbound float coercion, failed writes,
  uninitialized Stringable conversion, contained exceptions and same-field reentry.
  Use fresh known Reflection receivers for this path. After broad catch effects,
  a receiver can enter the runtime fallback, whose protected setter wiring is next.
- Before Casts admission, audit PHP's temporary Reflection Stringable destruction
  order (knowledgec1351278) separately from old-value retirement. Named retained
  source values and explicit error release make the current publication probe's
  lifetimes intentional.

### Protected scoped runtime static setters

- Checked runtime static slots now select the same binding preparation and static
  publisher as direct EIR writes. Their C adapter borrows the original RHS and
  returns a boxed Throwable through an explicit output.
- A versioned scoped setter captures the existing seven arguments plus a Throwable
  output, preserving register/stack ABI placement. It calls the existing scoped
  dispatcher inside native exception containment, then returns miss, success or
  Throwable status to Magician. Legacy native callers keep their boolean entry.
- RuntimeValueOps consumes the explicit status and schedules the returned Throwable
  through ordinary interpreter catches. Scope and visibility still come from the
  original dispatcher; declaration validation remains in the shared coordinator.
- Direct versioned C ABI probes exercise conversion, rejection, initialization,
  callback reentry and private scoped access under staged Casts. A real opaque
  eval constructs its own Reflection receiver to verify the Rust Throwable path.
  Reusing a native Reflection receiver after catch effects is an unreliable proxy:
  a separate permissive baseline reproduces a silent no-op (taskcbfbae83).
- Verified both new native fixtures with both optimizer settings and clean heaps,
  real five-target assembly for the scoped setter and runtime publisher, compiler
  check/build, architecture inventory828, assembly comments and diff hygiene.

### Exact declaration references under Coercions

- A shared checked-property eligibility helper selects canonical declaration
  storage for Casts or Coercions independently. Source references, static storage,
  native metadata and scoped runtime setters consume that same decision.
- Native declaration validators distinguish PHP strict_types widening from exact
  Coercions membership. User declarations under Coercions reject int-to-float
  changes regardless of the PHP caller bit, while unions preserve exact members.
  Trusted implementation declarations retain their reviewed PHP behavior.
- Prepared builtin reference argument graphs are selected independently from cast
  domain checks. Coercions-only settype therefore writes the original checked
  place but preserves permissive explicit conversion domains. Descriptor argument
  selection and call_user_func value semantics use the shared reference policy.
- Checked callable catalogs can materialize settype wrappers after source scans.
  Profile-aware object-cast prelude injection reserves their declared dependencies
  even without an explicit source settype or object cast; permissive usage-based
  injection remains unchanged.
- Staged fixtures cover scalar static/instance aliases, prepared settype, declared
  unions, later direct writes and independent cast behavior. A direct versioned
  static setter fixture covers all four Casts/Coercions combinations and contained
  errors. All five-target selected-checker/EIR checks, both optimizer modes,
  every Casts/Coercions combination, native producer/descriptor controls, compiler
  check/build and the complete generated-builtin workflow pass. No generated docs
  changed. The native module test helper links the exact reference archive from
  Cargo JSON output inside its queued job. Public admission stays at six until
  the remaining boundaries are done.

### Exact eval-owned property values

- Shared runtime membership now lives in value_contracts.rs, retaining scalar,
  nominal, union, intersection, generic and callable rules from return checking.
  Declaration owners resolve self/parent independently of the writing scope.
- A focused property_value_contracts adapter checks candidates before instance,
  static, Reflection or reference-binding publication under Coercions. Existing
  permissive non-null object diagnostics remain in their original policy.
- The bytecode engine deliberately reports PropertySet as a compilation gap.
  Focused VM-call tests enter the ordinary walker function-body fallback, so both
  supported execution routes observe the same property guard.
- Whole-owner checks exposed independent activation leases in the fake Reflection
  factory. Fresh metadata fields now transfer into the fake object graph, and
  class-only flags are allocated only for class reflectors. A permissive control
  checks that ledger independently of strict property behavior.
- Verified scalar/union/structural and lexical nominal cases over direct and
  reflected writes with walker and VM-call fallback, plus a permissive Reflection
  owner control. Existing null property/reference/return, exact parameter/return
  and fake graph-owner controls pass. A real opaque native eval Stringable throw
  passes both optimizer modes with clean heaps. Compiler check/build, architecture
  inventory828 and hygiene pass. Coercions and Casts remain staged.

### Live eval-owned property reference writes

- Checked property bindings promote source variables into persistent reference cells.
  The context projects current instance/static associations into borrowed declaration
  constraints, including parent-private storage names. No association retains a receiver.
- Interpreter cell replacement checks every live property source before publication.
  Alias assignments, by-reference parameters, invoker slots and escaped closure
  captures use this adapter. Unset, rebind and metadata retirement release only the
  departing source, while other sources continue to constrain a shared cell.
- Five new regressions cover assignment-point errors before later effects, escaped
  captures, source retirement, private shadowing and multiple live declarations.
  Walker and supported VM caller/fallback paths preserve the previous value and
  balance all fake runtime owners. The fake retirement case explicitly invokes the
  native metadata-forget equivalent; it does not claim to test native finalization.
- All eight property tests, existing null-property/local-reference/settype/owner
  controls and the real opaque native eval Stringable exception test pass. Compiler
  check/build, architecture inventory828, assembly comments and diff hygiene pass.
  Native stores outside the interpreter and cross-context interoperability remain
  admission work. Coercions and Casts stay staged; the public feature mask is six.

### Eval-owned constraints at native reference publication

- The native coordinator checks eval-owned property sources before its native
  declaration coercers. A versioned Magician callback returns success, failure
  or an owned raw Throwable; no native exception crosses Rust. Inputs stay borrowed.
- The callback copies live thread-local ABI context handles and an active native
  caller, then checks their weak property associations. The interpreter also checks
  foreign live sources and moves a source owner's TypeError to the writing context.
  Context and receiver lifetimes are not extended by these metadata scans.
- EIR requests canonical validation for Coercions with eval even when an opaque
  class has no corresponding AOT property declaration. Other profiles and binaries
  without the eval bridge retain their existing dependency boundary.
- A real opaque native experiment was red with later callee effects and a changed
  integer value. With the guard it passes both optimizers with clean heaps,
  including native catches, nested eval, static/shared sources, unset and native
  object finalization. That experiment temporarily opened the development runtime
  admission gate. The gate was restored before this commit because a separate
  builtin probe still accepts strlen(7) under Coercions. Those integration fixtures
  remain pending the completed runtime admission step and are not this commit's CI
  coverage. No new public profile is enabled.
- Nine property tests, the pending-profile API rejection, three native coordinator
  tests including real five-target clang assembly, compiler check/build,
  architecture inventory828 and selected-source hygiene pass. Next use neutral
  builtin parameter types and existing argument ownership/binding paths to close
  builtin conversions before admitting Coercions; then finish all other profiles.

### Eval builtin value input contracts

- The evaluated builtin dispatcher validates fixed input parameters against the
  canonical neutral TypeSpec contract before runtime or value hooks can convert
  them. Scalars, nullable types, unions, arrays, objects and callable membership
  share one adapter. No second builtin name or signature table was introduced.
- Mixed input APIs and explicit conversion functions retain their declared
  behavior. Null defaults remain explicit API sentinels. Reference outputs retain
  their callee-owned validation and writeback policy, since their old kind can be
  irrelevant to the operation. Special source/direct reference handlers still
  require their own shared boundary before runtime admission.
- A strlen(7) fixture was red with output1 and no exception. Three new tests cover
  direct, named, spread, callback and mixed-case calls; scalar/null rejections;
  exact and explicit input controls; a supported VM caller; Stringable rejection
  before its hook; and complete fake owner accounting. The old one-argument eval
  implode limitation is tracked separately as task94773458; the strict control
  uses its supported two-argument form.
- All fifteen selected parameter, return, property and pending-profile tests pass,
  plus compiler check/build and the curl builtin exporter. The complete generated
  builtin workflow passes with no generated diff, architecture inventory828,
  assembly comments and diff hygiene. Coercions remains staged with six public
  features until special source handlers, AOT/folding and host audits are complete.

### Eval source builtin contracts and reference owners

- Selected eval signature parameter types now travel with EvalParamSpec and the
  passing-mode view, including eval-specific extra parameters. Shared source
  wrappers check the already evaluated inputs before invoking original handlers;
  regex outputs, OpenSSL tags, socket/stream outputs and other source-sensitive
  behavior keep their existing targets and source order. Array mutators share
  their established evaluated reference dispatcher under Coercions.
- Checked reference variables, ordinary mutable eval properties and eval statics
  capture stable cells at their source position. Later argument writes are visible
  without reading a property again; an unset/rebind leaves the captured old cell
  separate from the replacement property. Casts-only alias preparation remains an
  independent admission audit.
- Fresh direct replacements share an owning publication helper. Variable storage
  consumes its lease; reference cells and other borrowed targets receive the value
  before its producer lease is retired. Explicit Owned output writes also honor
  that transfer policy outside plain variables. Source/output failures retire the
  candidate while preserving the original target and pending TypeError.
- Rooted sort/splice builders retire owned key/value reads, generated integer keys
  and partial results. Sort collection also cleans reads on conversion failure;
  scalar float extraction retires its conversion temporary. Positive source
  controls exposed these ownership errors rather than accepting leaked results.
- All twenty-five focused tests pass, including nine parameter tests and
  direct/named/callable input checks, exact mutation/output controls, live and
  detached arguments, typed output rejection, legacy splice/settype and complete
  fake owner ledgers. Compiler check --tests, build and curl exporter pass.
  The full generated-builtin workflow passes without generated changes, with
  architecture inventory828 and zero structural errors. Whitespace and function
  documentation checks pass. The six-bit public admission mask remains unchanged.

### Original builtin input evidence before folding

- The source call validator also resolves neutral builtin signatures when no
  source declaration owns the name. User/extern ambiguity and namespace rules
  retain their existing conservative handling. Named/spread placement reuses the
  shared argument planner rather than matching parameter names again.
- A focused builtin input adapter projects selected AOT parameter types with null
  defaults preserved as API sentinels. Mixed input APIs and output references keep
  their specialized contracts. Declaration and builtin source checks share the
  same original-kind proof and diagnostics.
- The original strlen(7) source-validation test was red. Four focused tests now
  pass: builtin rejection before and after initial folding; explicit/exact/null
  sentinel and trusted-source controls; prior scalar parameter rejections; and
  original-value guarded storage emitted for all five targets in both optimizer
  modes. Compiler check/build, the curl exporter and the full builtin docs workflow
  already cover this source snapshot; generated files remain unchanged.
- This closes the proven scalar source evidence gap. Dynamic builtin inputs still
  need typed checker/EIR guards before coercive ABI preparation. Coercions remains
  staged, alongside the independent Casts lifetime and host audits.

### Converted ReflectionClass argument retirement

- ReflectionClass and ReflectionProperty static writes now carry distinct argument
  retirement policies through EIR. The class setter hands its borrowed caller
  parking slot to the checked publisher; ordinary/property setters keep their
  argument until publication. Both parking operands are validated as registered
  unwind owners, and the backend uses target-aware ABI helpers.
- The class-call publisher roots the converted candidate while retiring the source
  snapshot and clearing the caller's argument owner. PHP destructors therefore see
  the previous property. A successful conversion is still published when this
  retirement throws; only then does the pending Throwable reach the caller. The
  initialization marker follows that publication, including an Error caused by
  reading a previously uninitialized property from the destructor.
- Two native tests pass with both optimizers and clean heaps. Five focused root
  tests also pass, covering EIR policy on every target before/after optimization,
  root/commit protocols and real clang assembly for the complete reference write
  family on all five supported targets. Assembly comments and whitespace pass.
  Three adjacent native setter controls also pass, as do compiler check --tests
  and build, the architecture828 gate, and source documentation hygiene.
- Casts remains staged. PHP probes also distinguish a pre-existing reference from
  one introduced by a destructor: follow up with native rebind fixtures before
  claiming binding identity parity. Runtime/opaque ReflectionClass calls and
  Casts-only eval argument aliases remain independent admission audits. The public
  mask still contains the same six completed guarantees.

### Native builtin input contracts

The native dynamic-input regression exposed five unchecked conversions in the
standard positional builtin path: `strlen` accepted int, bool, null and a
Stringable object, and `str_repeat` accepted a float count. The old result was
`1AX1AX0AXS9AXaaAX|1A|abc`, with balanced owners. The corrected five-case fixture
passes under both optimizer settings and has a clean heap.

The common builtin argument entry now projects fixed value-input types from the
neutral AOT metadata before planning. Names, defaults, by-reference modes and
variadic collector shapes stay with the existing signature. Nullable alternatives
are retained. The common parameter-order pass checks evaluated inputs before
builtin-specific value adaptation. JSON's source preparation also runs that pass
before its detached string conversion. Shared and non-lazy checker validation
reuses each already inferred input type for static strict diagnostics.

The expanded native fixture adds named arguments, unpacking, a first-class
callable, later-argument effects, JSON and runtime-null substring length. It
passes under both optimizer settings with clean heaps. Five focused library
checks pass, including typed source diagnostics, trusted implementation controls
and EIR generation on all five targets with both optimizer settings. The compiler
build and builtin architecture audit pass. Numeric and permissive controls are
checked separately. Coercions remains staged while the remaining
source/prelude/host paths are audited.

### Verified scalar comparison core

Step 100 was implemented in the owned `feat/strict-next` worktree while
`feat/strict-mode` retained the verified step 99 checkpoint. The worktree is based
on `b8f700b0467fc1c14794dda1b9068e9b1c6dfcd4`; no feature admission mask changed.

The dependency-neutral comparison policy distinguishes exact scalar ordering,
runtime guards and rejection. Source validation retains loose equality rejection
and original-kind evidence before folding, including nested literal eval.
The native path uses ordinary EIR predicates and branches: matching scalar kinds
are checked before conversion; strings use bytewise `StrCmp`, including spaceship;
other matching scalar kinds keep the reviewed IEEE/PHP scalar kernel. Operand
snapshots precede later source effects. AST folding, exception summaries and DCE
preserve these boundaries. Native switch matching uses strict equality and keeps
null distinct from integer zero.

Walker and VM binary evaluation share the same scalar policy and bytewise string
operation. Eval parsing rejects loose equality tokens before executing source.
Strict eval switch keeps source-order label effects, live variable subjects and
existing operand-owner ledgers, retiring temporary subjects before the case body.
Nine focused tests pass: source/folding/all-target checks, walker and VM scalar
contracts, parse rejection, live switch subjects, the native heap bundle and the
neutral policy matrix. Both native fixtures pass with both optimizer settings
and clean heaps. The compiler build, builtin architecture audit, example syntax
and whitespace/docblocks pass. The runnable example sorts string labels through
an explicit comparator while retaining leading zeros, verified as `02, 10, 9`.
An existing DCE comparison-guard regression also passed. Array construction stays
in the walker in the VM fixture; its comparison still executes in both engines.
Eval's already unsupported `<>` spelling stays unsupported in permissive mode
and gets the strict equality diagnostic in the checked parser.

Comparisons remains staged. Native value-class comparison domains, comparison
builtin policies, typed switch-label evidence and the remaining literal-eval/host
paths must be completed before admitting the feature.

### Prepared source policies and exact switch boundaries

Step 101 adds one reusable expression source-policy stack to the shared AST
visitor. Prepared producers retain their original operation policy through a
physical internal consumer. Nested statements establish their own lexical policy;
expression exit restores the parent policy. Inverse, sibling and nested closure
fixtures cover both accidental exemptions and accidental restrictions.

Typed switch labels use exact PHP kind overlap, plus existing nominal inheritance
and resource compatibility. Variable subjects are checked after label effects,
matching their execution-time lookup. Open unions and interfaces retain valid
possibilities. Nonvariable strict switch subjects use the existing call-owner
rooting protocol and retire before the selected body, including object and string
temporaries. The optimizer keeps the switch boundary instead of rewriting it to
a loose binary comparison with different capture and retirement timing.

Actual regression evidence: source copies incorrectly passed inside internal
statements; known disjoint typed cases passed checking; the native temporary
fixture printed `DB|SL` and leaked one object and one string. After owner repair,
only AST optimization changed its result to `DDX|SX`, exposing the single-case
rewrite. Verification covers independent AST/EIR optimization settings and heap
balance. All eight focused compiler checks and three native fixtures pass;
the temporary-switch fixture passes all four AST/EIR optimizer combinations with
a clean heap. The compiler build, all-five-target EIR emission, architecture
inventory and source hygiene pass. Public admission remains unchanged.

### Numeric producer proof and prepared result binding

Step 102 reuses the shared source-policy traversal in numeric evidence. Recursive
proof selects each prepared producer's mode and PHP strict-types setting, restores
the caller after either success or a diagnostic, and applies the validated
IntegerToFloat binding to its result before a public consumer checks the value.
Declaration binding remains a consumer boundary with its own physical policy.

The two baseline fixtures failed before this change: all eight combinations of
physical/producer mode for Overflow, Arithmetic, Coercions and Casts were inverted,
and a prepared trusted float result was reported as its unbound integer value.
Both fixtures pass after the fix. Nine focused source/checker tests and the
compiler build pass in the original checkout, including existing declaration,
trusted constant, user/builtin call and all-five-target storage controls. The
baseline failures and the independent successful rerun establish the regression
and its correction without changing public feature admission.

### Builtin comparison policy integration

The canonical builtin contract now carries comparison metadata, keyed by stable
identity and parameter name. Searches
require explicit identity mode. Automatic comparison APIs direct the caller to
a source-selected comparator; enumeration-only `array_keys()` remains valid.
Existing defaults stay unchanged. Compiler source proof consumes the common call
planner; a shared projection handles fixed source/default operands without
assuming values supplied by dynamic unpacking. Native mode checks run after all
source arguments, retain operand owners and emit ordinary EIR. Eval's existing
named/reference binder and positional value boundary share the same policy.
Comparison-mode failures are observable to AST DCE and exceptional flow.

Tests cover static omitted/false modes before folding, named/static spread
arguments, prepared producer modes, all-five-target EIR, native callable/default
mode failures, source effects, heap accounting and eval walker/VM consumers.
The label example now demonstrates an explicit identity membership check.
Focused source, prepared producer, neutral contract and all-target EIR checks
passed. Native descriptor fallback now uses prepared EIR bodies, including a
rejection-only body for automatic comparison APIs. Reused callables after a caught
failure retain explicit/default mode guards and normal return/argument ownership.
Direct eval sort calls also consume the shared evaluated comparison contract,
independently of Coercions. VM tests use compiled callers with the supported
interpreter fallback rather than claiming unsupported direct builtin bytecode.

Actual owner regressions exposed and fixed two separate search issues. Eval now
retires each key, fetched entry and comparison result through the existing owner
finisher. Native array_search returns a fresh boxed key or false, never its
haystack allocation; its previous alias metadata leaked a temporary string array
and both entries (152 bytes), also in permissive mode. New native isolation
controls and the complete builtin failure bundle pass with clean heaps; the
bundle covers every AST/EIR optimizer combination. Four focused eval tests and
all-five-target descriptor checks pass. Compiler build, curl exporter, complete
generated builtin documentation workflow (828 AOT, 679 eval), assembly-comment
alignment and diff hygiene pass. The local Rust1.99 toolchain reports only the
previously documented inherited native_catalog deprecation and test import warnings.

No policy has been newly admitted or claimed complete. Production implementation
and verification use the original checkout and its own Cargo target directory.
Strings source baselines have established 14 rejected cases are still accepted,
with explicit formatting positive controls passing. New, unregistered Strings
helpers and fixtures are prepared for the next step and excluded from step103.


### Verified string compiler and native integration

String concatenation now checks original string kinds through shared source/type
helpers and EIR predicates, independently of Coercions. Formatting of scalars and
Stringable values remains an explicit context; arrays are refused before output.
The compiler lexer emits an internal StringFormat prefix for every interpolated
operand, represented by CastType::StringFormat. Folding, type/result/alias
projections, missing-string handling, source reconstruction and AOT eligibility
retain this context. Generated source uses braced interpolation when possible and
a scoped, once-evaluated formatting argument for other prepared expressions.

Native concatenation evaluates and captures both original operands before
checking them, sharing the existing snapshot and unwind owner helpers. Compound
stores validate before publication. Native output guards retain temporary owners.
A focused regression also exposed print's existing omitted temporary retirement:
print new Stringable left one24-byte object and skipped its destructor. Print now
uses the same owned-value retirement as echo, with a separate permissive control.
Another actual integration red showed a lost Stringable catch under AST
optimization because StringFormat lacked the ordinary String cast throw summary;
that projection is now shared and its strict/permissive native regression passes
all4 AST/EIR optimizer combinations with clean heaps.

The14 source rejection baselines and formatting positives pass. Five source,
producer/consumer, reconstruction and all-five-target EIR tests pass, together
with5 exact lexer tests and1 Pratt/parser provenance regression. Native controls
cover all4 AST/EIR combinations for failures, alias snapshots, compound-reference
writes, output union failures, hooks and owners; the row-label example and
permissive print controls pass. Final focused verification and compiler build
passed after the throw-summary repair, together with architecture828, assembly
comment alignment, example syntax and diff hygiene. Eval and host parity remain
the next step, and Strings remains staged. No new public profile is admitted.


### Verified eval string integration

Both eval consumers now retain StringFormat through lexer tokens, EvalCastType
and the bytecode formatting opcode. Shared checked_strings helpers validate
original concatenation kinds and array formatting before conversion hooks.
Walker echo and owned output retain their separate existing lease protocols;
both call the same formatting check. Explicit casts preserve their own behavior.
Compiler and eval diagnostics share neutral conversion messages.

The actual baseline accepted all14 expected rejection cases across walker and VM.
Those failures now throw TypeError. Four new eval tests cover original kinds,
source effects, explicit conversions, Stringable hooks/throws, compound local,
alias, entry and property stores, and whole fake owner accounting. These and the
existing explicit string conversion control pass. Eleven exact lexer fixtures,
a parser provenance/precedence test and the old textual-key interpolation control
pass. Compiler reconstruction/all-five-target checks, two native owner/throw
bundles and compiler build also pass after the shared eval implementation.

The already-built format guard reproduced the schema mismatch: fingerprint
3110961241602077184 must become14787256250629008100. File-cache format31 now
retains formatting casts and tokens; a new disk roundtrip checks that distinction,
and touched cache fixtures use explicit directory owners. Main's live format was
28 when inspected on2026-11-16. Schema identity, old-version refusal, ordinary
and formatting disk roundtrips, compiler build and real runtime scalar
interpolation compatibility all pass. The runtime scalar harness uses a directory
owner on every exit; its archive is prebuilt so inherited Cargo warnings do not
become compiler diagnostics. No profile is newly admitted.

A separate106 native XML property-update probe reproduced a strict bypass after
both permissive optimizer controls passed. An erased receiver accepted integer7
and stringx through the specialized helper and modified the XML: output
Xa7|Xa7x|a7xx instead of Ta|Ta|ax. The heap remained clean. Its module registration
and new fixture stay outside105. Public original-kind validation before native
write-fetch, further native views and broader host parity remain required before
Strings admission.


### Native string update boundary (106, verified)

The reproduced native XML bypass is addressed by a shared, non-mutating
ObjectProperty::UpdateRead adapter. It preserves empty-name and uninitialized
receiver diagnostics before a readonly property selection. Both EIR and eval
inspect that original result and the already evaluated RHS under Strings before
entering the existing writable update helper. Permissive source, trusted
implementation source, ordinary object fallback and numeric update operations
retain their existing paths. Verification is queued as2195bebd: native named and
dynamic update fixtures plus the independent eval protocol owner control. No
feature admission is changed while those results remain pending.

Job2195bebd reached compilation and found a moved-value error in the new fake
fixture before any execution test ran. The fixture now clones its per-case
source value. Retryd9bd6db8 (session53650) includes the eval owner test, five
first-class target emitter tests with EIR optimization on/off, both native XML
execution fixtures, compiler build and builtin exporter. The all-target module
is registered. The missing typed-string read probe is also registered but stays
outside the retry filter; its concern remains unverified until separately run.
No native fix is yet verified or committed.


### Native106 verification and typed-string107 baseline

Retryd9bd6db8 exited0 after14m47s. The eval native protocol owner test passed;
all five supported target emitter tests passed in both EIR optimization modes
and public/internal source controls; both real XML named/dynamic execution
fixtures passed in strict/permissive and optimized/unoptimized modes with clean
heaps; compiler and curl builtin exporter builds passed. The generated builtin
workflow, module/comparison generation, docs/site audits and architecture audit
all passed with no generated metadata drift. The first all-target test fixture
retained unrelated XML/DOM declarations and took525.27s collectively. It now
uses the production reachability pass with explicit exported probe functions and
asserts that the neutral contract retains UpdateRead. That test-only refinement
is queued as26b68d0b/session56243; it must pass before106 is committed.

Separate baseline25bec80c/session84112 reproduced the typed-string missing-read
issue after four permissive AST/EIR controls passed. Strings with both optimizers
off produced x|ax|[]|x instead of T|ax|[]|x. Formatting, explicit casts and heap
balance passed; the original null kind is lost before the concat check. Fact
6a822db7 captures the evidence. The fixture and its module registration remain
outside106. Strings is still staged and the complete implementation continues.


Final106 fixture verification26b68d0b exited0: all five supported target tests
passed after production reachability pruning, with both EIR optimization modes
and public/internal source controls retained. Collective execution dropped from
525.27s to391.26s. Together with the native/eval, build and generated-doc results
above,106 is verified. The new missing-string fixture and its registration stay
outside this commit. Strings remains staged because107 still needs its logical
null check. Inspection confirms str_persist preserves the dedicated sentinel on
both architectures; TypePredicate(String) alone accepts concrete Str storage,
while IsNull already checks its logical missing value (facte2d23047).


### Logical null behind typed string storage (107, verified)

The concat contract now uses the existing target-aware IsNull operation for
concrete Str storage before the ordinary kind predicate, with the same unwind
owner pin discipline. Ownership/persistence and explicit formatting/casts retain
their existing behavior. The small original five-target string fixture now also
includes a typed string-array miss and asserts that the logical guard survives
EIR optimization in public source and stays absent in trusted implementation
source. Queued060fc446/session97410 selects that exact library test, the
eight-program strict/permissive AST/EIR missing-read regression and compiler
build. The native XML106 fix is already signed and complete; no feature is newly
admitted while107 and later host/boundary audits remain pending.


A separate, currently unregistered tests/codegen/strict_string_boxed_nulls.rs
probe is prepared for the next audit. It distinguishes missing typed-string
values passed through native mixed arguments/returns from genuine empty strings.
PHP syntax validation passed; no execution result exists yet. Register and run
it after107 before claiming Mixed/native boundary or Strings admission parity.


Final107 job060fc446 exited0 after3m22s. The existing small string guard test
passed all5 target/source-policy checks with both EIR optimization modes in0.98s.
The native missing-read regression passed all8 strict/permissive AST/EIR mode
combinations with clean heaps, correct TypeError behavior and unchanged explicit
formatting/cast controls. Compiler build passed. This107 commit excludes the
new boxed-null probe and its module registration. That probe is now registered
and queued as2892c6b5/session8050 for the next boundary audit. No execution result
exists for it yet, and Strings admission remains closed.


### Logical string identity through native Mixed boundaries (108, verified)

Baseline2892c6b5 reproduced both Mixed argument and return paths accepting a
missing typed string as an empty string in concat. Four permissive optimizer
controls passed, strict noopt produced x|x|x|ax instead of T|T|x|ax, owners clean
(factdadab672). Shared string-null helpers now classify the dedicated pointer
marker without confusing genuine empty strings or sentinel-valued integers.
Borrowed/owned Mixed construction publishes canonical null, unboxing supports
legacy string-shaped null cells, and scalar/boxed kind predicates share that
classification. Owned frontend boxing delegates to the existing runtime boxer.
This replaces107's operation-local IsNull check with the reusable string-kind
predicate. Raw gettype classification also uses the shared pointer rule.

First verificationead5a1c6 passed both relocation modes through Clang on all five
targets, the small all-target string fixture, and all8 native boxed-null concat
optimizer/profile controls. A second control fixture used unsupported
get_debug_type and was corrected to the existing gettype/is_null/is_string
surface; no builtin was added. PHP reference stdout confirms kind, genuine
empty-string, colliding-integer and owned strstr result controls. Retry7b8b9b68
(session80352) selects that control, the direct missing-read regression, existing
Stringable throw/owner and permissive print owner fixtures, then compiler build.
Its results are pending; no108 commit or new feature admission yet.


Final108 verification7b8b9b68 exited0. Both default and strict kind/boxing
controls match reference PHP, including empty/owned strstr strings, gettype,
is_null/is_string and a real integer whose bits equal the string sentinel. Direct
missing reads, existing strict boundary/owner bundle, Stringable throws,
permissive print owners and compiler build pass with clean heaps. Additional
backend verificatione72aabe4 passed raw unboxed gettype emission on all5 targets.
The earlier Clang runtime check covered all5 target triples in direct/PIC modes,
and the small EIR string fixture passed both optimizer settings. Assembly comment
and whitespace audits pass. Task-owned cross-assembly directories were removed
by their RAII owner. Shared string-kind classification now replaces107's local
concat null check, so the same rule serves other value consumers. No serialized
shape or semantic ABI bit changed; the existing source-derived runtime identity
covers the new helpers. Strings remains staged pending real opaque eval,
callable/reference and remaining host-boundary audits.


### Real opaque eval string host parity (109, verified)

The full native Magician fixture covers nullable scope parameters and aliases,
canonical missing returns, native descriptors through all3 call forms, Stringable
formatting/hooks/throws, array output, failed compound writes and VM exceptions
crossing into AOT catches. Reference PHP establishes permissive stdout.

Initialb3fd3b65 selected1 test and stopped on a pre-existing native/eval catch-name
contract collision (facta7883b31). Formatting and native catches use distinct
names while that case remains for the stronger local-binding audit. Retry89890af8
then proved a permissive alias-visibility and destructor-timing regression:
A:NULL: inside eval versus native string:x after reload, and endDQ rather than
per-object native unset destruction (fact1aa3ceaf). Both runs eventually had
balanced owners, so this was observable lifetime, not an exit leak.

Managed native references now authenticate storage and acquire canonical wrapper
identity through existing typed adapters before scope import. Headerless ABI
inputs retain the fallback. Eval-scope capability emits those adapters without
enabling property validation or linking its bridge. After native writeback,
activation copies retire through a bounded destructor adapter. Interpreter-only
bindings survive and a surviving borrowed alias acquires the appropriate lease.
Global scope remains materialized for callbacks between eval entries.

84247404 passed scope ownership, canonical snapshots, retirement and destructor
adapters on all5 targets; all4 native strict/permissive optimization controls
passed with exact alias/destructor timing and clean heaps (120.17s), as did the
public CLI source and independent-coercion checks. The initial REPL expectation
omitted its ordinary assignment-expression display; corrected expected output
retains int(7) before T[7]. Final38f24000 exited0 after4m05s: dynamic value/ref
owner controls64.29s, REPL45.11s, existing canonical native snapshot23.80s, pending
throw/writeback39.06s, all5-target string guards0.69s and compiler build53.24s.
Each selected filter was counted to1 before execution. Assembly comments and
whitespace audits pass. Runtime/source identity covers the additive ABI helper;
no stored EvalIR shape or profile bit numbering changed.

Strings is now publicly admitted independently, covering compiler/eval/native
callables/REPL and the previously verified native XML update paths. User CLI and
runtime docs describe formatting and explicit casts. The complete umbrella and
remaining guarantees stay gated. Two future Coercions property-reference probe
files remain unregistered and outside109. Continue with their actual native
execution and remaining conversion/cast/Locals admission work, then the approved
nullability/mixed/initialization and exact-array guarantees.
