# elephc-memoir

Compiler-side assembly generation for Elephc memory management. This crate is
an ordinary Rust `rlib` dependency of the compiler. It is never linked into
compiled PHP programs and produces no runtime static archive or `--with-memoir`
capability. Generated programs continue to use the same emitted `__rt_*` helpers.

## Boundary

Memoir owns heap allocation and consolidation, allocator diagnostics and metrics,
reference counting, cycle collection and candidate proofs, graph traversal,
container allocation and growth, copy-on-write and cloning, deep release, owned
strings and scratch allocation, byte concatenation, owned C-string copies,
growable output/capture buffers, consuming string append,
callable-descriptor retirement, hash-entry ownership guards, generation-safe
buffers, Mixed boxing and unboxing, raw-slot array conversion, persistent references,
reference-cell lifetime, wrapper promotion and writable child-slot selection,
parameter-owner acquisition, original-identity pins, protected cleanup, temporary
owner retirement, validated boxed object transfers, the object-handle pool, guarded stack mapping,
zend_string allocation, recursive zval/string/array storage retirement, and memory
proofs for direct scalar/empty-array cleanup, static binding storage, guard
records, depth-stamped temporary owner lists, abandoned activation cleanup,
stored-property ownership completion, previous-slot release and box reuse during hydration,
non-owning observer caches, hash-entry reference promotion and borrowed payload views,
temporary/fixed-frame owner record publication, exact chain unlink and consuming
container boxing and Mixed symbol publication through persistent reference children.
It also emits fixed heap, GC, buffer, reference-root, handle-table and string-scratch
declarations, including raw capture-buffer storage, keeping their existing order
and byte layout.

The `Emitter` facade composes three independent host capabilities:

| Capability | Responsibility |
|---|---|
| `AssemblyEmitter` | Text, labels, comments and raw directives |
| `TargetAbi` | Target conventions, registers, frames, calls and symbol spelling |
| `MemoryContext` | Layout facts and explicit compiler-owned semantic hooks |

The ABI forwarding layer requires only `TargetAbi`; protected release forwarding
requires `MemoryContext`. The compiler implements each capability using its existing
ABI and output services.
`DataTarget` delegates object-format syntax. `ValueFeatures`, `GraphLayout` and
`GcStatusSelectors` carry the small set of compiler facts needed by the emitters
without importing PHP types or EIR. Resource-boxing hooks retain PHP subtype/id
policy and inventory publication in the compiler adapter. Shared payload sizes
and pointer-container tags live in `layout`. Capture finalization receives a compiler
callback that resets output mode after storage retirement. Storage emission does
not select an output destination. Zval storage owns copied strings and arrays;
object and resource payloads remain borrowed native pointers. Mixed wrapper
promotion receives an optional compiler callback for native type-source association.
The host also supplies the uninitialized-property sentinel and scalar tags eligible
for cleanup without child ownership. Static retirement receives the compiler's
weak-source unbind callback. Guard primitives receive host owner/spill offsets and
chain storage; scoped lists receive head/depth symbols. Handler installation,
failed-constructor destructor policy and parser completion stay in the compiler.
`activation_cleanup` requires only assembly output and target ABI capabilities.
The compiler supplies its chain symbol and selects the surviving activation;
Memoir detaches every abandoned record before invoking its cleanup callback.
`hydration_owners` likewise uses only assembly and target ABI capabilities. Its
context names the host uninitialized marker, box deferral entry and saved-payload
release entry. Wire parsing, property validation, duplicate-key selection and
parser completion stay in the compiler.
`weak_cache` receives the compiler's wrapper class-id symbol. It publishes cached
observers without retaining referents and clears both directions before storage
reuse. `hash_entry_references` separates shared Mixed boxes before moving the
entry's owner into a reference cell. Its inline reads use caller-selected registers
and return a borrow without allocating or changing ownership. Both modules require
only assembly output and target ABI capabilities. Iterator cursor and successor
rules, PHP method validation and class identity stay in the compiler.
`owner_records` centralizes the complete six-word record size, publication and exact
unlink operation. Its context carries the host chain, cleanup and visitor symbols;
fixed records use `FrameOffset`. The compiler chooses record placement and owner
discipline. Stack addressing, scratch register selection and scratch reservation
continue through the compiler's authoritative target ABI. Record linking and unlinking
emit no runtime calls and preserve newer activation records and live arguments.
`values::owned_container` receives its entry label and assembly heading from the
compiler. It lets Mixed acquire the fresh container before retiring the caller's
original owner. JSON flags, parsing and exception publication stay in the compiler.
`reference_cells::mixed_symbol` receives the host symbol, initialization metadata
offset, prior-owner release policy, optional transfer slot and label prefix.
`RecordOffset` and `FrameOffset` keep metadata layout separate from owner parking.
It publishes the replacement through any persistent wrapper, clears transferred
ownership and initialization metadata, then retires the prior owner when requested.
Static-property type checks and EIR ownership decisions remain in the compiler.

The compiler retains EIR lifetime analysis, ownership optimization and placement,
PHP reference/type validation, formatting, output routing, zval pack/unpack,
resource identity/inventory policy, class/coroutine/SPL layout definitions, platform
mapping flags, semantic runtime data, helper ordering, runtime object caching, and
final linking.
Memory constants live in Memoir with compatibility re-exports where needed.
The former compiler memory modules retain thin adapters and integration tests;
they contain no duplicate implementation.

The extractions preserve emitted instructions, labels and ownership behavior.
Frozen assembly fingerprints cover all five supported targets and executable,
PIC, recoverable-library, hosted-extension and dead-strip configurations. The
value-storage fingerprints also cover optional resource hooks and the complete
fixed runtime-data block. Storage fingerprints additionally cover growable
buffers, append metadata variants, callable capture release and hash guards.
Boxed-value fingerprints cover construction, persistent-reference operations and
protected reference retirement. Identity fingerprints cover unboxing, temporary
owners, reference acquisition, borrowed identity pins, handle recycling and stack
mapping, together with complete fixed data. The identity test also cross-assembles
every target and emission mode. String/interoperability fingerprints additionally
cover two scratch capacities, C-string storage, recursive zval release, capture
storage and the compiler routing adapter. Mixed-storage fingerprints cover both
native reference-validation settings, raw-slot conversion, writable wrapper slots
and scalar/empty-array release gates through the full compiler cleanup boundary.
Owner-record fingerprints cover static readers and record size, weak unbinding,
guard ownership transfer and scoped list detachment through the compiler adapters.
Reference-storage fingerprints cover weak-cache creation, reads and retirement,
complete hash-reference integration, and two inline register arrangements. These
60 cases cover every supported target and emission mode without fixed semantic data.
Another 60 cases preserve the complete temporary-owner adapter inventory and recursive
JSON decoder, including custom callback records and fixed return leases.
Mixed symbol publication adds 180 cases covering both release policies and absent,
small and large transferred-owner slots on every target and emission mode.

`build.rs` includes this crate's Rust sources in the runtime-cache identity.
Changes to these emitters therefore invalidate cached runtime objects even when
no compiler-local emitter source changes.

## Typed configuration

`HeapKind` contains only a storage-kind byte, while `ValueTag` represents a boxed
or container value tag. Their Rust types cannot be substituted for one another.
`RecordOffset` and `FrameOffset` address aligned owner words from different bases.
`GuardLayout` pairs them and rejects an owner overlapping the activation prefix.
Compiler adapters construct layouts from their authoritative ABI constants.
`ScopedOwnerState` names head and depth symbols instead of passing two positional
strings. These contracts configure emission; they add no target runtime checks.

## Boundary gates

Run `python3 -B scripts/audit_memoir_boundary.py` from the workspace to audit Cargo
metadata and Rust source. The gate rejects dependencies in Memoir, native crate
outputs/build hooks, workspace production consumers outside the compiler, PHP/EIR
semantic types, and native export/link attributes. Cargo dependency aliases,
target-specific entries and indirect workspace consumers are checked. Build/dev
consumers remain host tools. Source comments and literals do not count as imports
or attributes. The compiled CLI/bridge-catalog test checks generated-program
capabilities and archive registration against the live catalog.

All three host CI builders run the boundary audit and its regression tests, plus
Memoir unit tests and compile-fail documentation examples. The existing assembly
fingerprints remain the output contract, including the allocator/collector
cross-assembler matrix.
