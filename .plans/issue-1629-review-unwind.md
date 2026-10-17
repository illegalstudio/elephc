# Issue #1629 / PR #1765: nested unserialize error ownership

- [x] Reproduce nested parser-context and partial-object ownership failures.
- [x] Split option/property error boundaries and pin unfinished parser values.
- [x] Replay focused ownership and serialization regressions.
- [x] Fix shared Mach-O diagnostic joins and assemble every supported target.
- [x] Reproduce private-parent/public-child name shadowing against PHP.
- [x] Resolve serialization keys, offsets, tags and diagnostics from the same physical slot.
- [x] Complete the final registered shadowing/ownership replay and update the existing PR.

Reviewed base: `af604baffb74e3671fccae2beb70359268a9652b`.

## Reproduced review findings

- A caught property TypeError in `__wakeup()` closed both the inner and outer parser contexts, corrupting later back-references and the outer allowed-class policy.
- Twelve caught failures inside arrays left 48 live blocks / 3744 bytes. Unfinished hydration-data hashes, ancestor objects, and user-hook throws reproduced the same missing unwind ownership.

## Implementation

The option-validation TypeError entry still closes its context because validation precedes the protected decoder boundary. The property entry leaves context closure to that boundary. Both architectures share the same distinction.

Each recursive parser frame publishes three initially empty slots in the existing exception cleanup chain: the unfinished object box, array hash, and hydration-data hash. Hash shadows follow reallocations. Normal returns transfer the initial owner without retaining; explicit parse-failure cleanup zeros the slot before releasing; hook adaptation transfers data ownership to the context before calling PHP. Cleanup callbacks zero slots and bound destructor throws through the existing unwind walker.

AArch64 reserves 288 bytes (FP at SP+272); x86_64 reserves 256 bytes below RBP. Hook argument offsets follow that layout. No new global owner registry or extra retain is introduced.

## Regression boundaries

Eight focused cases exercise nested policy/reference restoration; partial arrays, data hashes and objects; wakeup/hydration throws; pre-boundary option failures; deeper parents and exactly-once destruction; and throwing cleanup destructors. Wire fixtures are independently run with PHP. Memory cases use heap-debug assertions and repeat caught errors.

The object-parent fixture uses a declared mixed property without a default. Separate existing limitations were observed in typed-string hydration (orphaned parsed boxes) and overwriting initialized mixed defaults; they are independent of the reviewed missing parent-frame owners. The nested policy fixture stores options in a variable so ordinary call-argument ownership is explicit. Prototype fixtures and baseline outputs remain in `/private/tmp/elephc-1765-review`.

## Validation

- Warning-free `cargo build`; seven focused runtime-emitter unit tests.
- 79 focused `unserialize` integration tests, including eight new exception-cleanup cases.
- Eight PHP-oracle fixtures with EIR optimization on/off, executed on macOS ARM64 and Linux ARM64/x86_64: 48 matching outputs, all heap-clean.
- Three representative fixtures with EIR on/off on both iOS device/Simulator targets: 24 assembled program/runtime objects and runtime-symbol closure.
- Assembly comment alignment, Rust module preambles, and `git diff --check` pass.

Evidence and replay scripts: `/private/tmp/elephc-1765-review`. No full local suite or iOS execution is claimed.

## CI maintenance, 2026-10-15

Rebased on main 132b4da6f7, including the rustls advisory update.
The old CI atom test found a local join inside the property error helper
referenced by the option error helper. Both architecture joins now use
label_shared. Their prologues, argument ABI, context boundaries and diagnostic
body are unchanged. The runtime documentation names the property error entry
and explains its closure boundary and the Mach-O alternate-entry invariant.

The current forced-fresh dev compiler is warning-free. Both existing Mach-O
atom/assembler test bodies pass against the production runtime generator.
The full generated runtime assembles for macOS ARM64, both iOS ARM64 variants,
Linux ARM64 and Linux x86_64. These five checks are assembly evidence, not
runtime execution of iOS or Linux programs.

Focused validation passes 16 hydration/exception-cleanup GC tests (plus 17
support checks) and 59 unserialize functional/emitter tests. No full local
suite was run. Shared-target harness dependencies are selected by the exact
resolved Cargo package version; one initial setup attempt chose sha2 0.11
instead of the project's sha2 0.10 and is excluded from validation evidence.
Evidence: /private/tmp/elephc-pr-maintenance-20261015/1765-*.

## Physical property slots, 2026-10-16

The published Rust source at b6fd42b038 rejects a valid child string as an integer:
private parent and public child properties share a name, while the layout retains
two slots. The old name-keyed serializer maps both rows to the child destination
but pairs the first with the parent's type. The frozen baseline and complete PHP
object oracle are recorded in 1765-private-baseline.json.

Serialization now reuses the existing ancestor walk for physical property metadata,
renamed from renderer_property_slot_metadata to physical_property_slot_metadata.
The same resolution drives mangled keys, byte offsets, storage tags, declaring-class
IDs and declared-type diagnostics. Rendering still uses its optional debug projection;
serialization consumes the underlying physical slots directly. The table ABI is unchanged.

Six regressions are registered through the actual serialize parent module: full
object and wire round trips, separate private/public writes, both diagnostic owners,
null-default untyped child acceptance, debugInfo separation, and a heap-owning private
string parent with a scalar child. Five-target program assembly passes with IR on/off;
iOS uses static-library emission and is compile/assembly coverage only.

The first heap-clean assertions exposed the already documented successful typed
hydration residue. The frozen pre-fix CLI reproduces the same 2 blocks / 88 bytes
on distinct-name slots. Final tests compare complete PHP output and require shadowing
not to increase retained blocks/bytes over that distinct-name control; error paths
remain clean. No heap-clean successful typed-hydration claim is made. An additional
integer-default untyped-property probe exposed an independent inferred-storage shape
error; its artifact is retained for exact-main replay and untyped-property maintenance.
The declaredness regression uses the supported null-default Mixed slot.

Final review replay: 94 registered serialization tests passed (89 existing and five
new); after adding the opposite heap/scalar layout, all six new tests pass on/off.
All sixteen hydration/exception ownership controls pass. Ten program objects
assemble across the five supported targets on/off; iOS uses staticlib emission.
Compiler build is warning-free; assembly alignment and diff checks pass.

The frozen baseline also reproduces the inferred untyped-default shape error on
an ordinary single-slot class, independent of inheritance or this correction.
Evidence: 1765-untyped-inferred-baseline.json. It remains an adjacent follow-up,
not a claimed fix in this PR. Full CI on the published head is still required.
