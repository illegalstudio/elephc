# Emit PHP extensions from Elephc

- [x] Inspect the current export ABI, runtime lifecycle, value bridge, and PR #1743 without changing that PR.
- [x] Define the producer architecture, reuse boundary, ownership rules, and acceptance matrix.
- [x] Discover and validate the PHP host SDK and add `Emit::Phpext` with target diagnostics.
- [x] Separate export discovery from output-specific signature validation and generate Zend registration metadata.
- [x] Introduce a request-scoped hosted runtime context with NTS, ZTS, and nested-call isolation.
- [x] Implement the neutral value transport and scalar, string, array, and null conversion.
- [x] Generate recoverable native invocation thunks and translate exceptions after native cleanup.
- [x] Integrate PHP output and diagnostics without bypassing the host output system.
- [ ] Add the example, differential tests, lifecycle/concurrency/leak checks, and six mandatory CI configurations.
- [ ] Document the supported surface and verify existing executable, cdylib, staticlib, and iOS behavior.

## Scope and status

This plan tracks the implementation of a distinct
producer output, `elephc --emit phpext`, whose artifact PHP loads through
`extension=...`. Top-level functions marked `#[Export]` become PHP functions.
The existing C ABI for `cdylib` and `staticlib` remains compatible.

The first delivery covers by-value `int`, `float`, `bool`, binary `string`,
`array`, `null`, supported nullable forms, and `void` returns. Arrays include
nested values, integer and string keys, sparse indices, and heterogeneous values.
Objects and classes at the boundary are a later phase. Exception construction
inside compiled code remains supported because it is needed for `throw`.

Explicit initial restrictions are by-reference export parameters/returns,
variadic exports, callable/resource/object values, and recursive or
reference-containing array graphs. Unsupported signatures fail compilation;
unsupported dynamic array contents fail with a PHP exception before the body
runs. Never silently substitute null, flatten keys, or recurse indefinitely.
An ordinary PHP reference used as a by-value argument is dereferenced and copied;
the restriction concerns references inside the transported array graph.

This producer plan supersedes the producer design in
`bidirectional-php-extension-bridge.md`, particularly its old handler signature,
synthetic-zval reuse assumptions, acceptance of process termination, and exclusion
of thread safety. Consumer hosting and a bidirectional demonstration are outside
this delivery. No changes, commits, reviews, or dependency additions are made to
PR #1743 as part of this planning task.

## Inspected baseline

- Checkout: `24857c77176b62f87a1c850e7f2dca8f4cca16a1`, branch
  `feat/emit-php-library`.
- PR [#1743](https://github.com/illegalstudio/elephc/pull/1743) was OPEN at
  inspection, head `a7ee31b722162264c7535b93a5e50cd92873d5f3`.
  Its source was read through GitHub, without checking out or changing its branch.
- `src/exports.rs` already records PHP names, mapped C names, source-visible
  signatures, physical signatures, defaults, and source spans. Validation currently
  excludes arrays and nullable types.
- `src/codegen_support/cdylib/` provides target-aware native trampolines,
  internally contained exception recovery, scalar status reporting, independently
  owned string results, and lifecycle symbols. The documented ABI is single-threaded.
- `elephc_init()` initializes boundary/scratch state and the stack guard, plus
  optional startup. `elephc_shutdown()` resets boundary state; it is not a complete
  request heap/resource teardown implementation.
- Mutable runtime state includes common/global heap state, concat scratch,
  exception and cleanup stacks, stack limits, and builtin state. Merely putting
  the adapter's pointer in Zend module globals would not make those globals safe.
- `zval_pack` creates Elephc-owned PHP-shaped storage. Packed arrays in this
  transport are represented as Bucket arrays. Real Zend packed arrays use the
  `arPacked` zval view, so this representation cannot be passed directly to PHP.
- The inspected machine has Linux x86_64 PHP CLI 8.5.11 NTS and matching
  `php-config`; this is an environment observation, not an extension test result.
  Its `php-config` has no `--phpapi` option, so SDK discovery must use header probes.

## 1. Reuse from elephc-zend

| Part of PR #1743 | Producer use |
| --- | --- |
| `src/abi/values.rs` and relevant scalar types | Reference for value tags, widths, and the separation of scalars and refcounted values; reuse definitions only if independently checked against real PHP headers. |
| `src/abi/engine.rs` module/function/arginfo definitions | Reference for generated metadata and layout tests. Host headers, not these definitions, construct the actual Zend structures. |
| `abi_header.rs` generation approach | Reuse the single-contract generation pattern for the private Elephc transport header. Do not use its generated replacement `php.h` as the host SDK. |
| `src/host.rs` import/export/free logic | Identify and extract genuinely common ownership-neutral copy/rollback helpers in the producer commits, rather than maintaining a copied host bridge. Keep engine allocation separate and use real Zend iteration APIs for host arrays. |
| `tests/abi.rs` | Reuse the layout-probe test method, but compare against the installed PHP SDK. Its existing C/Rust checks compare definitions generated from the same crate. |
| `alloc`, `string`, `hash`, `zval`, `startup`, `global`, `module`, `zpp`, `exception`, C bailout implementation | Do not link their engine implementations into the produced extension. PHP supplies these operations and its own allocator and executor state. |

The crate's default `engine` feature implements Zend symbols, maps `emalloc` to
the C heap, and assumes one thread and one long request. Its scalar constants
pin PHP 8.5.6, NTS, and an NTS build ID. Some engine-internal layouts are
deliberately Elephc-specific. Disabling `engine` avoids those implementations but
does not turn the remaining definitions into validated host bindings.

Use a producer PR stacked on #1743 when reusing its actual code. The parent
supplies the existing crate and its history; producer commits can extract
genuinely shared helpers without cherry-picking or copying the engine onto main.
This source dependency does not mean linking the replacement Zend engine into
the generated PHP extension. The produced module still imports the real host API.

### PR structure

Create a new producer branch from a refreshed, recorded head of #1743. Its head
branch, `feat/php-ext-hosting`, lives in `Guikingone/elephc`, not in the upstream
repository. For one draft PR in `illegalstudio/elephc`, mirror the exact parent
head into the producer-owned base branch `stack/pr-1743-php-ext-hosting` and open
the producer PR against that mirror. Do not create another PR for the mirror.
The review diff must contain only
producer changes and any explicitly justified shared-helper extractions.
Do not push to, rewrite, merge, or change the base PR's branch. Changes to its
files made on the child branch belong exclusively to the producer PR.

Keep implementation slices as reviewable commits, with focused validation at
each slice. Check #1743's live state/head at each commit checkpoint, after major
validation, and immediately before publication. Its author may rewrite the
parent commit; compare actual source changes, not just commit counts. Refresh
the child and its producer-owned base mirror while preserving our own commits,
rechecking the exact resulting head. Never update the author's fork branch.
Merge order is #1743 first, producer second.
After the parent merges, retarget/rebase the producer onto main as appropriate
for the parent's merge method, remove inherited commits from the review diff,
and rerun validation on the new exact head.

The reuse audit must distinguish code that can actually be shared from code
that only informs the design. Extract only the former into engine-free helpers;
host-specific allocation, executor state, and ABI layouts remain separate.
Do not claim that stacking makes the entire crate reusable or host-compatible.

### Implementation checkpoint: complete producer and matrix validation

PR [#1862](https://github.com/illegalstudio/elephc/pull/1862) now contains the CLI,
SDK discovery, generated handlers/arginfo, native invocation thunks, deep-copy
transport, exception translation, guarded host callbacks and request isolation.
Its producer-owned base mirrors #1743 at
`9f928d6353139abf8766ffcc63e945f2c9993029`; the author's branch is unchanged.

The actual native thunk lives in `src/codegen_support/cdylib/phpext.rs`, sharing
the library recovery machinery. `src/phpext/state.rs` collects the entire native
writable region at stable addresses. `activation.c` exchanges request snapshots
under a module mutex and parks the active snapshot while PHP callbacks run.
The public example lives in `examples/php-extension/`. SDK selection uses
`ELEPHC_PHPEXT_PHP_CONFIG` and `ELEPHC_PHPEXT_PHP_BINARY`.

All 14 real-PHP integration tests pass locally and on the four Linux matrix
jobs. Local PHP 8.5.11 debug ZTS embed tests pass with eight concurrent threads,
repeated requests, reentrant output callbacks and stack recovery. The Linux
x86_64 ZTS embed lifecycle passes Valgrind without suppressing diagnostics.
The six-way CI remains the completion gate. The first full run exposed a
shared ARM64 frame-footer lifetime bug, an NTS test stack-limit configuration
issue and the need to register PHP's bounded Valgrind string comparison for
the embed library's soname. These fixes are under focused verification.

## 2. Architecture and code ownership

```text
PHP caller
  -> generated C Zend handler (real host headers, ZPP, arginfo)
  -> owned, versioned Elephc transport values
  -> generated native status/out-result thunk
  -> native import into Elephc runtime values
  -> existing compiled EIR function body
  -> native export into independent transport values
  -> ordinary return to C with status and owned diagnostic
  -> C builds a host-owned zval or sets a host PHP exception
  -> PHP caller
```

Use `src/phpext/` for SDK discovery, metadata, C generation/templates, adapter
compilation, and linking integration. Keep that name distinct from `src/php_ext/`,
which #1743 uses for consumer hosting. The C adapter is private support code,
embedded with the compiler and compiled on demand with the selected PHP SDK.
It is not another engine or a PHP userland interpreter.

Use `src/codegen/phpext/` for native invocation and type adaptation through existing
target-aware ABI helpers. Shared native helpers belong in focused leaves under
`src/codegen_support/runtime/phpext/`. Request-state access belongs in shared ABI
and runtime-state infrastructure. PHP-visible semantics continue through EIR;
the adapter only translates the external ABI and values.

The private transport has explicit tags, integer/double fields, pointer/length
strings, typed integer/string keys, ordered array entries, and a versioned
status/error record. Array metadata also carries the next-free integer index,
including the state left by deleted entries. The transport is not named `zval`
and has no Zend refcounts or layouts.
All transport storage belongs to a fallible per-call arena with one explicit
destroy operation, including rollback of partial trees. The arena is independent
of both runtime heaps. The native side reaches its allocator only through
non-throwing C ABI operations that never call Zend.

This extra transport copy is intentional for the first implementation. It keeps
real Zend pointers out of generated native value handling and makes cleanup
independent of PHP request allocation and native scratch lifetimes. Optimize
copies only after ownership and benchmarks justify it.

## 3. PHP SDK, registration, and linking

Proposed invocation:

```sh
elephc --emit phpext --php-config /path/to/php-config foo.php -o foo.so
php -n -d extension=./foo.so caller.php
```

`--php-config` selects an SDK, with PATH discovery as the default. Use argument
arrays, not shell evaluation, when consuming flags. Require PHP 8.5 headers,
a compatible C compiler, and the matching host binary for native validation.
The compiler's PHP semantic-version option is not a host ABI selector. Reject
an explicit incompatible semantic profile rather than mixing signatures.

Probe the headers for module API, full module build ID, ZTS, debug mode, relevant
sizes/alignments, architecture, and platform. Compare the paired binary's build
where executable. Do not hardcode the NTS build ID, infer ZTS from a directory
name, or depend on nonportable `php-config --phpapi`. Module version identifies
the generated extension; it is distinct from PHP version and module API.

Generate C metadata with `STANDARD_MODULE_HEADER`, real arginfo macros,
`zend_function_entry[]` with its terminator, and `ZEND_GET_MODULE`. All five
MINIT/MSHUTDOWN/RINIT/RSHUTDOWN/MINFO pointers are present. MINFO reports the
module version, compiler version, and PHP build identity through PHP's info API.
Generate handlers through the host's `PHP_FUNCTION`/`ZEND_NAMED_FUNCTION` macros,
with the PHP 8.5 signature `void ZEND_FASTCALL handler(zend_execute_data *, zval *)`.
Read arguments through ZPP and host call-frame macros, not hardcoded stack offsets.
Name/version default to a validated source-stem module name and the compiler's
version; allow explicit module-name/version overrides. Detect module/PHP function
name collisions, including case-insensitive duplicates and namespaced names.

Preserve the PHP name in the Zend table. C-safe symbol names are internal details.
Generate arginfo, arity, parameter names, defaults, nullability, and return
metadata from the source-visible signature, excluding physical hidden slots.
Generate host ZPP parsing and constant defaults, including omitted optional named
arguments, from that same descriptor. Nonconstant export defaults are initially
rejected. Test strict/weak scalar conversion against equivalent PHP declarations;
ZPP is the implementation mechanism, not evidence that semantics already match.

Linux output is a PIC shared object. macOS output follows the PHP module/bundle
link convention, normally with a `.so` filename. Do not equate the existing
Mach-O `-dylib` command with a PHP module link. Host Zend APIs remain imports;
do not link libphp, an embed SAPI, a second Zend engine, or a static libc.
Audit permitted unresolved host imports, especially with Darwin dynamic lookup.

Only `get_module` is an intentional defined dynamic export. Keep the module
entry, handlers, native functions, runtime, and linked bridge definitions local
using final-link export controls as well as existing assembly visibility.
Undefined host `zend_*` imports must retain the visibility needed to resolve.
Test loading two generated modules together to detect symbol interposition.

Add the hosted-runtime profile and its state/transport version to runtime-cache
identity. Cache SDK-dependent adapter objects by the complete ABI fingerprint,
compiler flags, and source identity. Never reuse a standalone runtime object for
the hosted profile merely because the CPU target matches.

Initially support Linux x86_64, Linux aarch64, and macOS arm64 on native matching
SDKs. Reject `phpext` on iOS device and Simulator with explicit diagnostics and
tests, since this delivery targets a dynamically loading PHP host. Cross-SDK
builds without a verifiable SDK are also rejected initially. Existing iOS C
library outputs stay covered by their current compile/emitter checks.

## 4. Runtime initialization and request ownership

| Hook | Responsibility |
| --- | --- |
| MINIT | Validate module-level configuration and register immutable descriptors. Do not initialize a request heap, execute source top-level statements, or replace PHP process handlers. |
| RINIT | Update the TSRM cache in ZTS builds; create a fresh module-specific request context and attach its opaque pointer to Zend module globals. Initialize native heap bookkeeping, scratch, exception/cleanup stacks, per-request statics and settings, and the current thread's stack guard. |
| First exported call | Verify an active context and enter a nested-call boundary. Lazily materialize optional resources through non-throwing initialization if needed. Do not create process-global request state. |
| RSHUTDOWN | Close owned request resources, clean native roots and temporaries, collect/free the native heap, clear diagnostics, and detach/destroy the context. Destruction is idempotent after partial initialization or failed calls. |
| MSHUTDOWN | Release module-level resources after request cleanup. Never free PHP globals, close host-owned standard descriptors, or shut down Zend. |

Use the same context model for NTS and ZTS. The context includes all mutable state
reachable from supported exports: heap/free-list/GC state, concat and output
scratch, exception and cleanup chains, boundary depth/status/diagnostics, stack
limits, compiled globals/statics, and mutable builtin caches/settings.
Immutable code, literals, and metadata can remain shared.

The initial producer uses complete request snapshots and a module-local mutex.
All native writable `.data`, `.bss`, and `.comm` storage is emitted into one
explicitly bounded section. RINIT copies the immutable initial image into a
request-owned snapshot. Before each native call, the adapter acquires the mutex
and restores that snapshot at the section's stable addresses; on return it saves
the entire image and restores the initial template before releasing the mutex.
This preserves internal pointers, including pointers into the fixed native heap,
without rewriting ABI-helper or raw assembly symbol accesses. The compiler
rejects storage sections it has not classified. All native state is covered,
not only the adapter pointer. Native calls are serialized in ZTS in this first
version; requests remain independent.

Native thunks call only C transport callbacks. Results and exception records
are copied into malloc-owned storage before the native boundary returns. Output
and warnings park the complete active image, release the mutex, and invoke the
real host API inside a C bailout guard. Nested PHP-to-native calls restore the
parked image and publish their changes before the outer native activation
resumes. Host exceptions and bailouts become a callback status, followed by a
contained native unwind; a bailout is re-raised only after all native frames
have returned. On resumption the adapter restores the outer thread or Fiber
stack floor. Core-only capability checks reject bridges and process-monitoring
state until they have an explicit hosted lifecycle policy.

Nested calls in the same request share persistent request state but receive
independent activation frames, temporary owners, diagnostics, and recovery
records. Restore the outer activation on every success and failure path.
Concurrent ZTS requests must not share mutable native state. Reject bridge or
runtime capabilities without an audited hosted state/lifecycle policy instead
of claiming compatibility from successful linking alone.

Do not call the existing cdylib lifecycle verbatim: it lacks complete request
teardown and uses global boundary state. Reuse its initialization primitives,
stack-guard logic, cleanup machinery, and idempotent startup patterns under the
new context ownership model.

## 5. Where zval conversion lives

Zend-facing conversion lives in the C adapter under `src/phpext/`, compiled
against the selected host headers. Runtime-facing conversion lives in native
helpers under `src/codegen_support/runtime/phpext/`, invoked by generated thunks.
No PHP-shaped allocation from `zval_pack` is handed to Zend, and no host zval is
freed with `zval_free`, Elephc heap cleanup, or C `free`.

| Value | PHP to Elephc | Elephc to PHP |
| --- | --- | --- |
| int / float / bool | Validate/coerce with the host parser, copy exact scalar bits into transport, then adapt to the physical native signature. | Read a typed native result, write the host zval through `ZVAL_LONG`, `ZVAL_DOUBLE`, or `ZVAL_BOOL`. |
| string | Read bytes and length, make an owned transport copy, then allocate an Elephc-owned string. Embedded NUL and non-UTF-8 bytes remain valid. | Make independent output transport bytes, then allocate a host string with `zend_string_init` and transfer it to `return_value`. |
| array | Iterate with real Zend hash macros, skip holes, preserve key type/order and next-free-index behavior, recursively copy supported values, then construct native arrays/hashes with existing runtime constructors. | Walk native arrays/hashes into transport, create host arrays through Zend APIs, insert the exact integer/string keys, and transfer child ownership according to Zend API contracts. |
| null / nullable | Use an explicit null transport tag and existing native nullable representation; never infer null from a generic zero pointer. | Emit `ZVAL_NULL` only for the null variant or a successful void return. |

Validate depth, allocation sizes, graph restrictions, and unsupported nested
values before invoking the compiled body. Preserve sparse indices, negative
keys, numeric-looking string keys that PHP actually stored as strings, empty
array behavior, and append index after deletion. Check transport/native array
construction against the source and physical signatures, rather than assuming
every PHP array is the same native typed array representation.
Restore the saved next-free index through SDK-compiled accessors where insertion
alone cannot reproduce it; never use synthetic HashTable offsets for this.

Inputs are borrowed only while the C adapter examines them. Each successful
heap crossing produces independently owned storage. Zend retains ownership of
arguments; the call owns transport arenas and native imported roots; Zend owns
the completed return value. Register native input roots for cleanup before
calling user code. If a return aliases an input or native static, export the
independent transport copy before releasing the relevant owner. Do not
double-release that alias.

Every partial-construction path has a cleanup ledger. Publish `return_value`
only after the result tree is complete; destroy a partially built Zend tree with
Zend destructors and native/transport trees with their own owners. A PHP bailout
during output conversion must still release non-Zend call storage.

## 6. Exceptions, panics, output, and host integrity

The generated invocation thunk always returns a status plus an out-result or an
owned error record. Reuse the cdylib exception-boundary and frame-cleanup design,
but keep its setjmp/longjmp entirely inside generated native execution. Do not
let a native exception jump into Zend or across Rust frames. Existing generic
cdylib diagnostics contain only a bounded message; extend the private producer
error record to retain message length, built-in exception class, and code before
the native Throwable is released.

After the thunk has returned and native cleanup is complete, C creates the host
exception through `zend_throw_exception` or the length-aware
`zend_throw_exception_zstr` path. Map supported built-in Throwable classes to
their host equivalents without calling a user-defined constructor. Preserve
message and code; use a documented module RuntimeException fallback for a
compiled custom class that is not registered in PHP. Objects/classes are not
otherwise exported in this phase. Argument failures use Zend TypeError or
ArgumentCountError. Existing pending host exceptions must not be overwritten.

No Rust frame may be below a Zend API operation that can bail out. Keep Zend
operations in C and protect fallible adapter phases with local Zend bailout
guards and explicit cleanup. Nonrecoverable host request aborts remain Zend's
responsibility, after native frames/resources have been removed. They are not
normal exported-function exception handling. Rust C ABI support, if needed,
must return explicit errors and contain recoverable panics locally; an aborting
panic profile cannot be made recoverable by wrapping the outer Zend handler.

Reuse the transitive export safety checker with a producer policy. Keep
`exit`/`die`, process/signal operations, uncontained fatal paths, opaque foreign
calls, and unaudited dynamic execution unavailable. Extend the checker for new
conversion and host-callback paths instead of bypassing it to accept arrays.
Apply validation under normal compile, `--check`, and `--emit-ir`.

Route terminal native output from `__rt_stdout_write` to a guarded C adapter
that uses PHP's output API, so host `ob_start()` and the SAPI see the bytes.
Do not use raw fd 1 writes or globally redirect the host stdout descriptor.
Keep internal string-return capture local. Host output handlers can throw,
bail out, or reenter an export: the C output callback must catch its own Zend
bailout, detect a pending exception, and return a status. Native code then
unwinds through its own boundary. Only after native return may the handler
propagate the host exception or resume a Zend-owned request abort.

Audit warning/diagnostic routes similarly. Any output-buffer or diagnostic
feature that cannot honor the host policy must have an explicit producer
diagnostic until implemented. A deferred end-of-call output flush is not an
equivalent replacement for observable callback ordering.

## 7. Implementation sequence and verification

Each implementation slice includes focused tests. This order does not make an
NTS-only intermediate milestone the completed feature.

1. **SDK and registration:** add discovery, fingerprinting, `Emit::PhpExt`,
   linker/export policy, empty module load, MINFO, and incompatible target/build
   diagnostics. Assert the real PHP host loads it, not just `dlopen`.
2. **Hosted state and lifecycle:** add request context/access policy, native
   setup/teardown, nested activation frames, and native multi-request/ZTS tests
   before exposing stateful compiled functions.
3. **Metadata and calls:** separate `exports::collect` discovery from C-library
   versus PHP-extension validation; add transport, typed invocation, scalar,
   string, null, optional/named arguments, and failure recovery.
4. **Arrays and ownership:** add native array adaptation, host API traversal,
   graph diagnostics, alias-safe cleanup, and allocation-failure injection.
5. **Host integration:** exception class/code/message conversion, guarded PHP
   output/diagnostics, nested reentry, two-module coexistence, and capability
   restrictions.
6. **Delivery gates:** example, docs, six CI configurations, leak/sanitizer
   runs, release artifact probes, and existing-output regressions.

Create `examples/phpext/` with `.gitignore`, build instructions, compiled source,
and a separate pure-PHP reference/caller. The four required functions are integer
addition, binary-string transformation, nested-array transformation, and a
function throwing RuntimeException. Additional focused fixtures cover
float/bool/null/nullable/void, defaults/named args, and unsupported signatures.

Use one differential driver against the extension and the reference in separate
PHP processes. Compare exact values, key types/order, binary string bytes,
exception class/message/code, and visible output. Check successful calls after
each exception or conversion failure. Test both IR optimization settings where
boundary lifetime/effect behavior can differ.

| Platform | PHP 8.5 NTS | PHP 8.5 ZTS |
| --- | --- | --- |
| linux-x86_64 | Required load, differential, lifecycle, and leak gates | Same gates plus actual concurrent requests |
| linux-aarch64 | Required on native ARM64 runner | Same gates plus actual concurrent requests |
| macos-aarch64 | Required native Apple runner | Same gates plus actual concurrent requests |

Pin PHP source/build provenance for reproducibility and verify runtime ZTS/debug
identity in each job. Rebuild the extension for each ABI; do not reuse an NTS
artifact in the ZTS job. Add negative NTS/ZTS and API/build mismatch load tests.
Successful PHP CLI execution with a ZTS binary proves ABI compatibility, not
concurrency. Use a threaded embed/SAPI test harness or an actual threaded SAPI
with independently initialized requests. Only the test harness may link libphp.

Leak verification has three independent views: native heap-debug live
allocations at call/request boundaries, a debug Zend allocator across repeated
requests, and native ASan/LSan or Linux Valgrind for transport/system allocations.
Run success, exception, invalid nested value, partial allocation, repeated
request, and unload paths. Use a PHP-only baseline for suppressions; avoid broad
suppressions of adapter/native stacks. RSS alone is not a leak criterion.
Add sanitizer coverage for races where the platform/PHP harness supports it.

Other required assertions: packed and hash arrays with holes, COW preservation
of caller inputs, input/result aliases, empty and large strings, embedded NUL,
NaN/infinity/signed zero, integer extremes, module/function name collisions,
no source top-level execution, output capture and throwing/reentrant output
handlers, no cross-request statics, and two extensions in one PHP process.

Run focused `tests/phpext_tests.rs` and native helper/emitter tests locally.
CI must fail if the selected PHP binary/SDK is missing or mismatched, rather
than skip a required configuration. Existing cdylib/staticlib ABI tests and
target-specific iOS compile tests provide regression coverage. Apply warning-free
builds, assembly-comment checks for edited emitters, and `git diff --check`.

Update CLI/output/target docs, the README flag examples, a producer guide under
`docs/beyond-php/`, the zval bridge ownership explanation, and the relevant
internals docs. Do not edit CHANGELOG during this work. Update an existing
roadmap item only under the repository's normal roadmap policy.

## Acceptance and remaining design risk

The feature is complete only when PHP 8.5 loads the generated artifact and the
four-function example, full value surface, recoverable exceptions, request
isolation, and leak checks pass in all six configurations. iOS has explicit
unsupported diagnostics rather than an unimplemented lowering path. No
process exit, panic, or Elephc longjmp is an accepted normal failure path into
the host.

The largest work item is hosted mutable-state isolation, followed by array
representation and cleanup under host callbacks. The present code does not
justify treating this as a small wrapper around `--emit cdylib`. The plan
deliberately requires those changes before claiming ZTS support.

## Primary references

- Inspected PR source at exact head:
  [crate and host bridge](https://github.com/illegalstudio/elephc/tree/a7ee31b722162264c7535b93a5e50cd92873d5f3/crates/elephc-zend),
  particularly `src/lib.rs`, `src/abi/{scalars,engine,values}.rs`, `src/alloc.rs`,
  `src/host.rs`, and `tests/abi.rs`.
- [PHP 8.5 module ABI](https://github.com/php/php-src/blob/PHP-8.5/Zend/zend_modules.h):
  module header/build identity and NTS/ZTS globals fields.
- [PHP 8.5 function and argument API](https://github.com/php/php-src/blob/PHP-8.5/Zend/zend_API.h):
  function entries, arginfo, and ZPP parsing macros.
- [PHP 8.5 values](https://github.com/php/php-src/blob/PHP-8.5/Zend/zend_types.h)
  and [hash traversal](https://github.com/php/php-src/blob/PHP-8.5/Zend/zend_hash.h):
  real packed/hash representations and iteration.
- [PHP 8.5 exception API](https://github.com/php/php-src/blob/PHP-8.5/Zend/zend_exceptions.c):
  host exception creation, including length-aware message construction.
- [PHP extension skeleton](https://github.com/php/php-src/blob/PHP-8.5/ext/skeleton/skeleton.c)
  and [php-config implementation](https://github.com/php/php-src/blob/PHP-8.5/scripts/php-config.in):
  lifecycle, TSRM cache handling, and available SDK discovery options.
