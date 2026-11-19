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
callable-descriptor storage initialization and retirement, hash-entry ownership
guards, generation-safe buffers, Mixed boxing and unboxing, raw-slot array
conversion, packed array header updates, persistent references,
reference-cell lifetime, wrapper promotion and writable child-slot selection,
parameter-owner acquisition, original-identity pins, protected cleanup, temporary
owner retirement, validated boxed object transfers, the object-handle pool, guarded stack mapping,
coroutine payload and initial stack storage, Zend array header allocation and inline zval
ownership transfer, zend_string allocation, recursive zval/string/array storage retirement, and memory
proofs for direct scalar/empty-array cleanup, static binding storage, guard
records, depth-stamped temporary owner lists, abandoned activation cleanup,
declared-object slot layout and initialization, stored-property ownership completion,
Throwable previous-slot access and ownership transfer, previous-slot release and box reuse during hydration,
borrowed frame and raw reference cleanup, eval scope cell replacement, fixed array resize storage,
non-owning observer caches, hash-entry reference promotion and borrowed payload views,
temporary/fixed-frame owner record publication, exact chain unlink and consuming
container boxing, Mixed symbol publication through persistent reference children,
external owner record publication, contained foreign-owner release and conditional
retirement of independently acquired helper owners, bounded cleanup invocation,
borrowed reference chains, scratch growth in `implode`, nested C export scratch
rewind, guarded callback-result ownership, staged reference pins and the Zend
storage transfers used by pack and unpack. Memoir also marks transient concat
reservations, probes buckets using normalized keys, tracks append-index storage,
adopts staged payloads into Mixed cells, owns scoped serialization streams, and
snapshots borrowed registry storage for nested serialization and deserialization,
including the outer context around nested deserialization.
It shares physical hash bucket publication, call argument setup for protected
cleanup and local cell release, pre-read stream buffer growth, and packed array
splice slot transfer, growth and compaction.
It also emits checked offset-vector allocation, boxed callback argument storage
and intercepted exception retirement, concat cursor saves and restores, owned
string buffer growth, protected transfer of previous Throwable owners, and
acquisition of staged hash payloads.
Memoir also builds packed offset rows and callback argument vectors, copies retained
Mixed slots, and emits weak resource cell and hosted replacement owner storage.
It builds packed slice destinations from normalized bounds, publishes initialized
prefixes in preallocated Mixed arrays, and initializes resource inventory nodes
before linking them in creation order.
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
release entry. Property and duplicate-key replacement share the in-frame box
transfer and saved-payload release. Wire parsing, property validation,
duplicate-key selection and parser completion stay in the compiler.
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
`owner_records::publish_external` receives caller-owned record, visitor and borrowed
data registers, plus chain and cleanup symbols. It initializes the entire record
before publication without calling the runtime. The compiler retains its versioned
C ABI wrappers, pure visitor adaptation and external cleanup discipline.
`foreign_release` receives an optional staging-owner transfer hook and explicit
exception unboxing/boxing hooks. It releases through the existing guarded cleanup
boundary, updates the caller's owned nullable exception accumulator and returns
whether destruction threw. ABI versioning and hook selection stay in the compiler.
`conditional_owner` receives typed mode/payload offsets for both architectures and
the flag bit identifying an extra owner. Its cleanup preserves the pending exception;
the compiler creates and propagates PHP assignment-refusal errors.

`strings::strcopy` emits both scratch-backed copies and independent owned copies.
Scratch-backed copies reserve and publish through the concat allocator. Independent
copies allocate fresh string storage without mbstring identity metadata.
`interop::cstr_scratch` owns the two reusable NUL-terminated destinations, including
their inline allocations, growth and reset emission. The host controls when to reset
them and whether to retire grown blocks individually or reclaim their arena.
`values::owned_payload` transfers an existing string or pointer owner directly into
a fresh Mixed cell. The host chooses the storage tag for pointer payloads and
handles false and null results.
`frame_owners` clears a typed frame slot before calling the selected release helper.
Its callers define cleanup order and install the surrounding exception handler.

`values::boxed_array` emits outer-cell copy-on-write, empty indexed-array
construction and hash payload preparation. It publishes a replacement hash before
retiring the old indexed payload. The compiler selects the lvalue, key and
conditions that allow creation of an empty array.
`callables::argument_owners` manages the private list of adapted descriptors used
by eval. It exposes the actual owner slots to the collector and detaches each
node before protected retirement. Bridge frame placement and entry labels come
from the compiler.
`reference_cells::validated_commit` publishes the validated candidate, optionally
consumes the publisher's snapshot lease, and retires the displaced owner through
the caller's pending Throwable slot. The compiler provides status values and
chooses when validation and commit run.
`strings::guarded_snapshot` parks a private string copy before a reentrant callback
and installs a guard that can free it during unwind. The caller supplies the frame
layout and selects the warning and its timing.

The typed and boxed container emitters carry the storage conversions, indexed
replacement and hash retirement beneath the compiler's key and promotion policy.
An indexed replacement or hash removal publishes its state before releasing the
former owner so a reentrant destructor sees the new state. `frame_owners` and
`cleanup_preserve` transfer frame owners and pending exceptions across guarded
releases. Container cleanup delegates handler installation and Throwable chaining
to the host exception ABI.

`values::object_clone_slots` acquires owners for direct and tagged clone slots
using the host's class metadata and independently preserves each path's string
handling. The native eval scope allocates and retires its stored nodes in Memoir.
A bounded deserializer registry takes a lease for each published cell and
delegates its final release to the existing scoped owner chain. A stream row
detaches both stored strings before destroying either allocation, and byte-view
consumers can stage a borrowed string byte through an aligned native stack slot.

`buffers::output` owns the output stack's pointer, length and capacity storage,
along with allocation, append, snapshot, truncation, depth reset and detachment.
Detachment clears the slot and frees its bytes and name before the host dispatches
callback retirement. PHP output flags, callback handling and flush policy stay
in the compiler.

Indexed-array emitters also own physical writes, appends, bulk copies and scalar
slot shifts, including the inline append fast path. Hash growth and clone share
the raw insertion-order cursor and owned bucket insertion with the compiler. The
host supplies automatic-key bookkeeping and controls reference-aware assignment.

`arena_reset` clears the collector and allocator for a full request reset. The
live-base index is cleared while the old arena extent is still available.
The web lifecycle chooses the full reset boundary. Persistent applications
keep their arenas and roots.

`handler_owners` saves, restores and retires active handler records using
compiler-selected field layouts and exception completion. `deferred_owners`
detaches each queued owner before invoking its destructor and drains additions
made during reentry. Scoped parser owners can move to an enclosing scope or
retire after detachment, while the parser chooses when to share its registry.

Object construction and clone share the allocator header and handle sequence.
Eval reference adapters share the transfer of a replacement payload into an
existing box and keep the two displaced-owner policies distinct.
Stream byte windows grow from staged offsets, and print_r renderers can park
and restore their capture owner around nested PHP calls.

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
External ownership adds 150 cases covering complete external-record adapters,
ordinary and staging-pin release, conditional retirement and the surrounding PHP
refusal path. These cases also cross-assemble on all five targets and six modes.
String copies, reusable C-string destinations, frame owner release and both web
reset paths add 90 assembly fingerprints across the same target and mode matrix.
Boxed array preparation, descriptor lists, reference commit and guarded string
snapshots add another 60 fingerprints across all five targets and six modes.

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

The native lifecycle fingerprints cover 30 combinations of target and emission
mode, including container conversions and mutation, protected cleanup, tagged
clones, native eval and registry storage, and output buffer storage and emitters.

The residual-storage fingerprints add 30 cases for indexed storage, raw hash
transfer, handler records, mbstring's FIFO, parser cleanup and stream windows.
The existing web reset and object clone fingerprints cover the reused emitters.

## Boundary gates

Run `python3 -B scripts/audit_memoir_boundary.py` from the workspace to audit Cargo
metadata and Rust source. The gate rejects dependencies in Memoir, native crate
outputs/build hooks, workspace production consumers outside the compiler, PHP/EIR
semantic types, and native export/link attributes. Cargo dependency aliases,
target-specific entries and indirect workspace consumers are checked. Build/dev
consumers remain host tools. Source comments and literals do not count as imports
or attributes. The compiled CLI/bridge-catalog test checks generated-program
capabilities, the public capability report and archive registration against the
live catalog.

All three host CI builders run the boundary audit and its regression tests, plus
Memoir unit tests and compile-fail documentation examples. The existing assembly
fingerprints remain the output contract, including the allocator/collector
cross-assembler matrix.
