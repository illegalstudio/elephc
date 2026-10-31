# PHP extension classes and constants

- [x] Create an isolated worktree from current main and remove the previous wrapper-only demo.
- [x] Inspect the existing producer, export safety, class schemas, state activation and ownership ABI.
- [x] Add explicit constant selection, constant-only modules and persistent global/namespaced constant registration.
- [x] Discover exported classes and preserve their method/property/constant metadata through reachability.
- [x] Prototype native instance leases and bidirectional object identity, including retained objects and cycles.
- [x] Add constructor, instance/static method and property operations through the existing native exception boundary.
- [x] Register real Zend classes, typed properties, methods and public class constants with coherent Reflection.
- [x] Extend transport for exported object parameters/results without exposing foreign PHP objects or raw handles.
- [x] Validate request teardown, fatal paths and Fiber reentry on Linux x86_64 NTS/ZTS.
- [x] Run the existing SDK CI matrix on real Linux aarch64 and macOS arm64 NTS/ZTS hosts.
- [x] Document the supported surface and demonstrate new Pippo() with only extension loading.

## Goal and boundaries

A PHP program that loads a produced extension must be able to instantiate an
exported class without including a PHP wrapper. Objects passed to compiled methods
or functions and returned to PHP must preserve identity, mutable state and lifetime.
Global/namespaced and public class constants must be visible through ordinary PHP
syntax and Reflection. Existing function-only modules must preserve their behavior.

Scope includes constructors, public instance/static methods and typed public
properties. Private implementation state must remain private and collectible.
Interfaces, inheritance and enums are deferred. Unsupported declarations must be
rejected before SDK compilation rather than silently changing their behavior.
Host subclassing must be rejected until native dispatch can represent it faithfully.
Do not mark a non-final source declaration final silently: require an explicitly
final exported class while subclassing is unsupported.

Supported module hosts remain PHP 8.5 NTS/ZTS on linux-x86_64, linux-aarch64 and
macos-aarch64. Existing iOS/cross-SDK diagnostics remain explicit. No PHP source is
executed at compile time and no replacement Zend engine is linked.

## Export syntax

Continue using #[Export] for top-level functions. Support #[Export] on a final
class to export its public constructor/methods, typed public properties and public
class constants. Preserve case, namespaces, argument names and source order.

For global constants use repeatable --export-constant NAME options alongside
--emit phpext. The source remains ordinary PHP: const VERSION = '1.0'; Attributes
on a global const are not valid PHP and are not introduced. Names are case-sensitive,
fully resolved, and may be namespaced. Unknown names, dynamic define() values and
unsupported constant expressions receive localized diagnostics. A module containing
only selected constants is valid. Other emit modes reject the option.

## Reuse and source integration

- src/exports.rs and a focused PHP export metadata module collect the public surface
  from the resolved AST and CheckResult before pruning. ClassInfo already supplies
  IDs, property offsets/defaults/types/visibility, method declarations/signatures,
  constant order/values/types and attributes (src/types/schema.rs).
- Pipeline reachability currently roots only functions (src/pipeline.rs). Exported
  class methods, constructors, property initializers and synthesized operations
  must become explicit roots so pruning cannot erase an externally called method.
- EIR remains authoritative. Synthesize typed helper bodies or dedicated private
  EIR operations for allocation without constructor, method invocation and property
  exchange. Avoid parsing generated PHP text or injecting runtime symbol guesses.
- src/codegen_support/cdylib/phpext.rs already owns the all-target contained native
  entry thunk. Extend its metadata rather than creating a second calling convention.
  Public arginfo excludes the hidden receiver; physical native arguments include it.
- src/phpext/producer.rs emits SDK-compiled C registration and handlers. Split class,
  constant and argument generation into cohesive modules instead of growing a dispatcher.
- Keep src/exports/safety.rs conservative: permit only audited exported object classes
  and all synthesized helper roots. Every reachable unsupported operation still fails.

## Runtime initialization and Zend registration

Reuse the checked real SDK and hidden-symbol linker path. MINIT initializes the
native template, registers persistent constants and real internal class entries,
then typed properties, constants and method tables. Registration failure returns
FAILURE with balanced persistent allocations. Globals/constants never point into a
request-native heap. API/build identity remains selected-host specific.

Class metadata must preserve names, parameter/return types, nullable/default forms,
visibility and public declaration order. Class constants preserve their declared
PHP types where supported by the selected headers. Namespaced object type names in
arginfo must be real Zend class types, not MAY_BE_ANY placeholders.

## Object identity and ownership

The initial design uses a real Zend wrapper object connected to one native instance
by a request-scoped identity registry. A pointer may identify an instance privately,
but PHP values receive only a Zend object, never a pointer or integer handle.

Each live wrapper owns an ordinary native reference. The existing cycle collector
classifies external owners by refcount minus heap-incoming edges on both architectures
(src/codegen_support/runtime/arrays/gc_collect_cycles*.rs). Do not reuse temporary
collector snapshot pins: their bit18 owner is deliberately subtracted from roots.

The registry must be transactional with request-image rollback. It distinguishes
request generation and instance identity, stages new owners until conversion commits,
and retires cache entries before storage can be reused. Repeated returns of the same
native instance reuse an existing Zend wrapper. Returning $this and repeated function
round trips must satisfy PHP ===, and host aliases observe the same mutations.

Native owners beyond a wrapper also matter: globals/statics, active frames and native
object graphs must keep the corresponding PHP identity alive when required. A weak
cache alone is insufficient. Prototype the owner-to-Zend root publication and expose
native-only strong edges through get_gc where needed. Cross-heap cycles must be
collectible; do not use an unconditional strong global wrapper cache as a shortcut.
If a shape lacks a proven ownership contract, diagnose it explicitly before admitting
it instead of implementing an identity-breaking copy.

free_obj must not execute native code against an inactive image. Retire the wrapper
and queue its native release for a guarded activation using preallocated lease storage.
Drain releases on safe boundaries and teardown. Native destructor invocation and
Zend destructor invocation must not double-run: the runtime already has a persistent
destructor-called bit14 (runtime/objects/call_destructor.rs). Use that contract when
admitting destructors; otherwise reject them explicitly until validated.

## Properties and object operations

Use standard Zend property metadata/storage for PHP typing, visibility, Reflection,
uninitialized-property behavior and indirect writes. Exchange relevant state with
the native instance at guarded entry/exit and host callback boundaries. Include all
state required for correct native behavior and GC, not only externally visible slots.
Preserve references held by PHP property aliases and COW for arrays. Synchronization
must not invoke Zend destructors while the native image is active or mutex held.

Allocate native defaults separately from __construct so PHP new and Reflection's
newInstanceWithoutConstructor have distinct, correct behavior. Bind returned native
instances without recursively invoking create_object or executing a constructor twice.
Clone must create a distinct instance while preserving PHP shallow-copy semantics;
reject unsupported magic/property hooks rather than accidentally inheriting generic
handlers that copy a private handle.

## Values, exceptions and lifecycle

src/phpext/transport.c remains the ownership boundary. Copy scalar/string/array data
as before; validate exported-object identities and class membership for object values.
Reject unrelated host objects, foreign-module objects, resources and unproven graphs.
Native zval pack/unpack must retain/release object owners through the same tagged
Mixed contract and all-target ABI helpers.

Reuse ep_invoke and the state/mutex/host-callback protocol in activation.c. Native code
must not call allocating Zend APIs or allow panic/longjmp across native frames.
Object conversion, property synchronization and destructors must participate in the
same rollback, exception chaining and bailout containment as ordinary exports.
Keep late Fiber/request shutdown behavior: RSHUTDOWN rejects new entries but preserves
parked activations; post-deactivate retires remaining images only after switching stops.
Generation-invalid objects and late release paths must fail/retire safely without
accessing another request's image. ZTS requests never share an object registry.

## Constant materialization

Reuse existing compiler constant folding/resolution for supported scalar, binary string
and array constant expressions. Do not duplicate PHP arithmetic or invoke the host PHP
interpreter to evaluate source. Preserve PHP array keys/order and case-sensitive names.
Emit constants into the host's persistent allocation domain at MINIT, recursively
balancing owned strings/arrays. Class constants and global constants use the same
value materializer and source validation. Undefined, recursive or unsupported values
must fail clearly. Request code cannot mutate the persistent value through an alias.

## Validation

Keep focused local runs behind me-build-lock. Add exact tests for export discovery,
constant selection, constant-only modules, rejected syntax/surfaces, reachability,
arginfo and class/constant/property Reflection. Use supported-target emitter contracts
and the existing real-SDK NTS/ZTS matrix for all three native hosts.

Real PHP tests must cover new Pippo with only extension loading, constructors and
factory returns, public/private state, static methods, defaults and named arguments,
nullable/object parameters/results, === identity, property aliases/indirect writes,
clone, retained instances, WeakReference observations, cycles, unset/GC, constructor
failure and repeated calls after exceptions. Compare supported behavior with the same
source executed by PHP. Cover constants in namespaces and class constants, binary
strings, arrays, typed Reflection and failed registrations.

Extend embed lifecycle tests for repeated requests, ZTS threads, suspended host callbacks,
late Fiber destruction, fatal abandonment and object cleanup. Keep allocation oracles,
leak tools and the existing4MiB fatal bound intact. A feature is not complete while an
admitted object shape loses identity, survives the wrong request or leaks across calls.

## Delivery

The old wrapper-only demo has been deleted. The replacement example must use a real
exported final Pippo class and constants, loaded with php -d extension=... and no PHP
wrapper require. Update docs/beyond-php/phpext.md and CLI docs/README for new options.
Do not edit CHANGELOG.md. Draft PR2185 tracks this phase, with independent Grok
review and native-host CI running concurrently. No merge has been requested.

## Implemented runtime contract

The producer emits real final internal classes with standard Zend allocation and
constructorless lazy native binding. Private typed EIR helpers allocate instances,
read/write/unset/test fields, release wrapper pins and classify native roots.
Standard clone and serialization preserve Zend storage until a compiled boundary.
Public metadata retains source self/static types, constructor promotion, defaults,
visibility and typed constants. All helper bodies validate on the five targets.

Request-local sidecars index both native and Zend pointers. A wrapper owns one
native pin; independent native owners add one host root. Root classification stops
at the first mirrored object, whose Zend properties already own its descendants.
All implementation fields participate in synchronization and normal Zend GC.
Native object elements preserve identity while string and array storage is copied.

A lease stages birth and release intent on the active call. Releases allocate no
transport boxes or journal records, so a full native heap can recover after PHP
releases its objects. A release flag on the existing lease is committed with the
native image or cleared on rollback. Dead sidecars retire during a long active
call once no active journal references them. Empty pointer indexes release their
high-water allocations. A 6000-object, single-activation churn test enforces a
256 KiB steady-state growth limit; restoring the previous sweep suppression
correctly fails this oracle.

Every ordinary entry/exit and guarded output/warning callback exchanges fields
and roots. All new properties and roots become visible before old values retire.
The request owns destructor-safe temporary zval nodes by monotonic call identity.
Retirement pops each node before a Zend destructor and protects the registry from
nested failure cleanup. Callback transport arenas reset after publication. A
reference joining two same-module exported properties is rejected; a foreign typed
property alias must impose the same constraint, resolving self/parent lexically.

Recovery keeps the pre-entry checkpoint, never a paused native stack image. An
infrastructure failure after host publication disables native entries for the rest
of the request, detaches surviving Zend objects safely and retires the native image
once all active frames finish. Ordinary source exceptions remain recoverable.
RSHUTDOWN refuses new entries but preserves parked calls. Independent native-root
references are released only after those calls finish, before Zend's final GC.
Post-deactivate abandons C state after switching stops. Fatal abandonment leaves
executor-owned Zend storage to the request arena, with the existing 4 MiB native
allocation-growth limit unchanged.

Constant selection accepts only source const declarations and supported constant
expressions. Namespaced/class aliases share compiler folding. Persistent arrays
use immutable Zend metadata with interned strings/keys, and survive MSHUTDOWN.
The shared-library unload destructor releases their pool after Zend destroys its
metadata, clearing immutability and restoring the sole-owner refcount first.
Constant-name collisions are preflighted before publishing any classes.

Two prerequisite fixes have focused heap-debug regressions: nullable function
statics retain boxed concrete object/scalar payloads, and pure global constant
types are available before class schemas validate array defaults. Runtime calls
and class lookups are excluded from that early declaration-only type scan.

## Verification status

Verified locally on Linux x86_64 with real PHP 8.5.12 NTS and debug PHP 8.5.11
NTS/ZTS. Seven class tests and six constant tests pass in each threading mode.
Coverage includes real construction, private state, arrays, identity, retained
native owners, WeakReference, clone, property aliases, uninitialized fields,
constructor/native exceptions, nested PHP callbacks, Reflection and immutable
metadata. A 64 KiB heap-pressure test repeatedly allocates 10000 host objects,
catches the native allocation failure, releases the caught Error trace and the
argument graph, then verifies subsequent calls recover.

The extended embed harness passes allocator, ordinary and fatal phases on NTS
and ZTS, including eight concurrent ZTS threads, object-bearing Fiber handoff,
late shutdown and retained native statics/cycles. Focused codegen regressions
report a clean heap. Seven direct class scenarios pass Clang ASan/LeakSanitizer,
including nested destructor failures and pressure recovery. After the shutdown-root correction, the allocator, ordinary and fatal embed
phases also pass with both PHP itself and the adapter compiled with ASan, with
LeakSanitizer enabled on each actual harness process. No sanitizer findings remain.

The Pippo example produces the same demonstration results as its pure-PHP source.
The driver prints a distinct mode line for the native extension and source fallback,
and requires only extension loading on the native path. Its actual module is built
with the matching installed PHP 8.5.12 SDK. Clang accepts extension assembly for
ELF x86_64, ELF aarch64 and Mach-O arm64. Real ARM64-host execution and the
Valgrind/Apple-leaks matrix passed in all six SDK jobs on `2f0f4738ef`
(run `41202917299`). No full local Rust suite is run. Current-head review and
validation remain visible on PR2185.

## CPU comparison example

The exported class also exposes countPrimes(int): int, a trial-division CPU workload.
The driver accepts an integer limit (default 2000000), labels the execution mode,
and reports the count plus hrtime-based seconds around one complete method call.
Both paths share the exact source algorithm. Reference sieve checks cover ten
small/edge inputs and independently confirm 148933 primes at the default limit.
Five alternating real-PHP measurements per mode show the current Linux x86_64
native module slower on this workload; no performance improvement is claimed.
Measurements and the boxed-numeric follow-up are recorded in Code Journal.

## Review and CI integration

The branch merged main4843af1291 to start PR2185 CI. The only textual conflict
was the runtime_gc test module list; both the new static-object tests and the
upstream GMP tests remain registered.

Grok round1 (delegation3c1b07aa) identified an AArch64 PIC register-clobber defect
in the GC wrapper-owner mode query. Native SDK CI independently reproduced four
class failures on Linux/macOS ARM64, both NTS and ZTS, while x86_64 passed. The
mode query must materialize its symbol in the destination register so the live
heap header in x9 survives. A focused target regression and all-host CI validate
that correction as the independent review continues.

Typed class constants share one resolved type contract for integer-to-float
coercion and Reflection. Their resolved expressions also populate hosted class
schemas and global constant declarations before EIR lowering, so native methods and array storage inference see
the same values as Zend. The real-host parity fixture compares nullable/union
constants, aliases, nested arrays, property defaults, mixed-return methods and
native array construction against identical pure PHP source.
