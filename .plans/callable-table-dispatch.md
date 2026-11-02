# Callable table dispatch specification

## Goal

Replace callable dispatch ladders duplicated at each generated call site with compact metadata
tables and pure runtime lookup helpers. The change is generic and must preserve the existing PHP
semantics, descriptor/invoker contracts, diagnostics, ownership, and all supported targets.

## Semantic universes

A lookup universe is the exact tuple of:

- callable shape: string function/static-method name, instance callable array, static callable
  array, or invokable object;
- strictness profile;
- `source_arg_ty.codegen_repr()` when the existing shape specializes descriptors with it; and
- the existing candidate filter: `Open` when analysis returns `None`, otherwise `Finite` with the
  sorted, unique, normalized reachable names.

Each shape keeps its existing target filters. A table never introduces a target that the current
ladder would omit. Sites with the same tuple and identical table content share a table; identical
word content is also deduplicated by `DataSection`.

After filtering and collision resolution within one universe:

- zero entries keep the current compile error or runtime miss path for that shape;
- one through eight entries remain inline;
- nine through 256 entries use a linear table; and
- more than 256 entries use deterministic open addressing.

## Canonical keys and ordering

Both the compile-time builder and runtime selector normalize names identically:

- remove at most the first leading backslash;
- lowercase ASCII `A` through `Z` only;
- preserve every internal backslash and every other byte;
- reject a compile-time identifier containing NUL; and
- treat a runtime selector containing NUL as a miss.

Original names remain in descriptors and diagnostics. The key encodings are:

- string name: the complete normalized name. A full `Class::method` string remains one string key;
  it is not split. Namespace fallback has already been resolved upstream by name/candidate
  resolution and is not repeated at runtime;
- instance callable array: `class_id` encoded as exactly eight little-endian bytes, then `0x00`,
  then the normalized method name;
- static callable array: normalized class name bytes, then `0x00`, then the normalized method
  name. Class-name normalization is exactly the common rule above: remove at most one leading
  backslash, fold ASCII `A` through `Z`, preserve internal backslashes, and reject NUL; and
- invokable object: `class_id` encoded as exactly eight little-endian bytes.

All supported targets are little-endian. FNV and ordering consume exactly these bytes. Equality
uses field lengths plus canonical bytes, with numeric `class_id` equality for instance/invokable
entries.

`class_id` is the program-global `ClassInfo::class_id` assigned by the checker for the one linked
AOT module. Object allocation writes that exact `u64` as the first object payload word, and runtime
class metadata tables use the same ids; callable lookup reads that word directly. There is no
per-object, per-table, runtime-object, or link-time remapping step.

Canonical vectors sort complete keys lexicographically as unsigned bytes; a strict prefix sorts
before the longer key. Hash buckets are filled in that vector order. A `HashMap` may index a
builder, but may never determine emission or iteration order.

## Collisions and identity

Existing filters remain authoritative, including strict-profile visibility and extern declarations
excluding a homonymous builtin. Entries are grouped by canonical key after those filters.

Global descriptor/template caches run before table collision resolution. Two entries may merge only
when their canonical key, descriptor-or-template label, and lookup flags are exactly equal. The
label here is the already-cached value identity, not a substitute for name collision handling. The
same key with a different cached value label or flags is a compilation error. Tables never dedupe
solely by entry-wrapper label and never pick an arbitrary winner.

## Target enumeration

- String-name entries use the current extern, supported builtin, user-function, and visible static
  method cases. Static callable strings use the complete normalized `Class::method` key.
- Instance callable-array entries enumerate every checker-metadata class and each public visible or
  inherited instance method whose implementation body is emitted. The key uses the receiver class
  id, while the descriptor template targets the recorded implementation class.
- Static callable-array entries enumerate every checker-metadata class and each public visible or
  inherited static method whose implementation body is emitted. Each final borrowed descriptor
  calls the implementation class while its wrapper bakes the called-class id of the class named by
  the key. It performs no heap allocation or receiver capture.
- Invokable entries are exactly the instance enumeration restricted to public `__invoke` methods
  with emitted bodies. The template targets the recorded implementation class and captures the
  runtime receiver only after lookup.

## Table algorithms and layouts

Linear tables scan in canonical-vector order. Open-address tables use FNV-1a 64-bit with offset
`14695981039346656037` and prime `1099511628211`. Capacity is the smallest power of two with load
factor at most one half. The initial bucket is `hash & (capacity - 1)` and collisions use linear
probing. A null value pointer marks an empty bucket. A hash match always performs full field equality.

All fields are `.quad` words. Linear layouts are:

- string: `key_ptr, key_len, value, flags`;
- instance: `class_id, method_ptr, method_len, value, flags`;
- static: `class_ptr, class_len, method_ptr, method_len, value, flags`; and
- invokable: `class_id, value, flags`.

Hash layouts prepend `hash` to the corresponding linear layout. Canonical strings are stored in
`DataSection` and referenced by `DataWord::Symbol`; tables are never reconstructed from descriptor
contents. Existing `.quad` relocation behavior used by descriptors and dynamic `instanceof` is the
target model.

## Lookup ABI and ownership

On success, a resolver returns the descriptor/template pointer in `x0` or `rax` and flags in `x1`
or `rdx`. On miss it returns zero in both outputs. The only legal flag values are:

- `0`: a final borrowed descriptor, directly invokable; and
- `CALLABLE_LOOKUP_CAPTURE_RECEIVER = 1`: a borrowed immutable template requiring the caller to
  capture the saved receiver and produce a fresh owned descriptor.

Debug builds assert flags are exactly zero or one. Release builds inspect only bit zero; all other
bits are zero by construction and structural tests enforce that invariant. The caller spills both
outputs immediately before any call or capture operation. Resolver inputs are borrowed; the caller
keeps the original selector pointer/length or selector spill live until the miss path completes.
The resolver allocates nothing, changes no refcount, emits no fatal, and does not mutate PHP stack
or exception state.

String and static entries return flag zero. Instance and invokable entries return bit zero set; the
existing receiver-capture helper creates the owned descriptor after lookup.

## Miss, eval, exceptions, and diagnostics

A table miss never falls back to an emitted ladder:

- string-name miss uses the current undefined-function fatal path;
- instance/static callable-array miss uses the current eval callable-array bridge when an
  `EvalContext` exists, preserving its temporary-stack and exception state, otherwise the current
  callable-array abort;
- invokable miss uses the current non-callable fatal path.

Runtime string misses in eval contexts remain fatal exactly as today. Adding a runtime-name eval
bridge is a separate audited change. Fatal bytes, exception behavior, longjmp behavior, and ownership
remain unchanged. The pre-existing fresh-descriptor longjmp leak remains tracked separately and must
not worsen.

The current undefined dynamic string diagnostic is generic and does not interpolate the selector.
The retained original selector nevertheless remains available to the existing caller-owned fatal
path; the canonical table key never replaces diagnostic input or becomes user-visible text.

## Required verification

Focused tests cover case folding, zero/one/double leading backslashes, namespaces, strict profiles,
`source_arg_ty`, collision errors, all four shapes, inherited static late-static binding, named and
spread arguments, by-reference parameters, defaults, variadics, eval callable-array hit/miss,
double invocation, ownership, and exceptions.

The inherited-static test must assert both called-class behavior and absence of descriptor heap
capture/allocation. Assembly/link checks cover AArch64 and x86_64. A structural generated-corpus test
requires universes over eight entries to emit no local candidate ladder over 30 instructions and to
show a substantial assembly reduction. Data bytes and instruction counts are recorded before the
next full application build.

Only after callable gates pass should the same data-driven principle be evaluated for dynamic object
construction and dynamic properties. Production code and tests must contain no application,
framework, dependency-manager, or tool-specific special case.
