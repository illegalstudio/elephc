# Shared metadata-object materializers

## Status and goal

This specification addresses two generic compiler defects exposed by a large stock application:

1. compile-time-known metadata objects are materialized by copying the full assembly graph into every call site, including recursively nested parents, interfaces, members, parameters, and declaring objects;
2. some AArch64 conditional branches target shared helper tails directly and therefore retain the conditional branch range limit.

The observed failing build emitted 563,123,428 bytes / 17,435,430 lines of assembly in 296.10 seconds, reached 1,927,806,976 bytes maximum RSS, and then failed in the assembler with an out-of-range `b.ne` to `__elephc_eval_value_method_call_fail`. No framework, package manager, or application-specific rule is permitted in production code.

The change must preserve all metadata semantics and fresh-object identity while emitting each identical compile-time materializer body only once per module.

## Non-goals

- Do not modify application, dependency, or generated vendor sources.
- Do not cache or share the mutable runtime objects produced by a materializer.
- Do not change runtime-selected metadata construction or interpreter fallback behavior.
- Do not suppress metadata, methods, properties, parameters, parents, interfaces, traits, constants, attributes, types, defaults, visibility, ownership, or errors to reduce output.
- Do not add names or detection rules for a framework, package manager, project layout, or third-party tool.
- Do not treat a successful assembly emission as end-to-end acceptance; `--web` runtime behavior remains a separate gate.

## Semantic invariants

1. Every materializer invocation allocates and returns a fresh object graph. Two identical constructor expressions must remain non-identical by `===`; mutating one returned object or a returned collection must not affect another.
2. Sharing applies only to immutable generated code and immutable compile-time metadata. No produced heap pointer is stored in module-wide state.
3. The existing raw object emitter remains the single implementation of property population. The shared helper changes code placement only.
4. Literal source operands use their exact extracted values as cache identity. PHP symbol normalization may be used only where the existing metadata resolver already uses it. A missed sharing opportunity is acceptable; merging semantically distinct metadata is not.
5. Runtime-selected, `Mixed`, union, callable, or object operands keep their existing runtime path unchanged.
6. Ownership remains unchanged: the helper returns one owned fresh object in the normal integer result register, and existing boxing/container insertion code consumes it exactly as before.
7. Exceptions and fatals must unwind through an ordinary target-aware helper frame to the caller's active handler.
8. The implementation supports macOS AArch64, Linux AArch64, and Linux x86_64 in the same change.
9. Module output remains deterministic for identical input and compiler revision.

## Exact materializer identity

Use one top-level typed, equality-compared enum (for example `ReflectionMaterializerKey`) whose variants discriminate every identity form below; do not use a lossy label fragment, string concatenation, or hash as semantic identity. Variant discrimination is what prevents equal payload strings from aliasing across object kinds.

Every variant carries a typed `ReflectionOwnerKind` (or an equally non-aliasing owner-class tag) in addition to its payload. The key covers:

- a direct literal owner constructor: typed owner kind plus an ordered vector of exact literal operands (`Text(String)` or `Position(i64)`);
- a full class-like object: typed owner kind plus exact resolved class-like name;
- a shallow class object: typed owner kind plus exact resolved class-like name;
- a shallow enum object: typed owner kind plus exact resolved enum name;
- a declaring function object: typed owner kind plus exact resolved function name;
- a declaring method object: typed owner kind plus exact optional declaring-class name and exact canonical method name.

Owner kind is part of every key variant, so equal resolved names or operands for `ReflectionClass`, `ReflectionObject`, `ReflectionEnum`, function-like owners, or any future owner type cannot alias. Variant constructors validate the currently permitted owner/payload combinations rather than silently dropping the owner. Parameter selector strings and integer positions remain distinct variants. Nested full and shallow forms remain distinct even when their current property values happen to match.

The current compile-time metadata resolvers accept only constant strings/class names, plus a constant string or signed integer selector for `ReflectionParameter`. Therefore the direct literal operand enum has exactly `Text(String)` and `Position(i64)`. `Bool`, `Float`, `Null`, arrays, and every other operand kind are ineligible for sharing and retain the existing runtime path or existing diagnostic; key construction must return ineligible rather than panic. New compile-time operand kinds cannot become shareable until both the resolver and this typed key enum are extended together with non-aliasing tests.

The module-wide cache stores the typed key and a module-unique assembly label in an append-only vector during successful emission. Immediately before reserving an outer key, record `cache.len()` as the subtree transaction checkpoint; reserve the outer entry, then permit nested reservations. If any helper body in that subtree fails, truncate the cache back to that exact length before returning the error. Thus entries from earlier successful siblings precede the checkpoint and survive, while the outer reservation and every successful or failed descendant created after it are removed together. An inner failure may truncate to its own nested checkpoint first; the outer truncation remains idempotent.

The backend inventory additionally owns an enclosing body transaction. `BackendInventory::run` must checkpoint both the emitter and the reflection-materializer cache length before invoking a body. If that body returns an error at any later point, it rolls the emitter back and truncates the reflection cache to the paired body checkpoint before scanning the next body. This covers materializers that emitted successfully before an unrelated later failure in the same body; no subsequent body may observe a cached label whose physical assembly was rolled back. When inventory is disabled, an error aborts artifact production as before. Existing unrelated shared caches are outside this change, but the new reflection cache and its assembly must always roll back atomically.

## Shared helper ABI and placement

Add a zero-argument object-result form of the existing synthetic shared-helper infrastructure rather than hand-writing a target-specific reflection frame.

- Its synthetic EIR function has no parameters and returns `IrType::Heap(IrHeapKind::Object)` with the precise `PhpType::Object(owner)` metadata.
- Its prologue/epilogue preserves frame pointer, return address, the reserved nested-call register, and required stack alignment on every supported target.
- The raw object emitter runs in a dedicated logical `FunctionContext` and frame layout built from the synthetic helper function, never the caller's context/frame/layout.
- At the first use, reserve the key/label, emit a caller jump over the helper's physical assembly region, create the dedicated synthetic `FunctionContext`, emit the labeled helper body through that context into the shared module assembly stream, end that context/frame, label the caller continuation, then call the helper. Physical interleaving in one assembly stream does not reuse the caller's logical context.
- Later uses only call the cached label.
- A nested first-use helper may be emitted inside an outer helper behind its own branch-over continuation. The key reservation prevents codegen recursion.
- Calls use a target-aware long-range ABI call helper. On AArch64 it must materialize the helper symbol address with the existing relocation-safe symbol-address primitive into a call-clobbered scratch register and use `blr`; it must not rely on `bl`'s +/-128 MiB range. On x86_64 it may retain the normal direct `call`. The chosen scratch register must not hold a live caller value at the materializer call site. The object result stays in the platform integer result register. Add target-specific assembly tests for this call shape.

No materializer body may capture caller locals, caller EIR values, or transient registers. If an allegedly compile-time metadata form requires any caller value, it is ineligible and must retain the existing runtime or inline path.

## Compile-time metadata indexes

Extend the immutable module-wide codegen state with a set containing exactly the canonical names of generator class-method bodies. Build it in `SharedCodegenState::for_module`, at the start of `block_emit::emit_module`, after lowering has produced the final EIR `Module` and before the first function body is emitted. Codegen receives that module by immutable `&Module`; no method can be added during emission. Build the set beside the existing emitted instance/static method indexes. `reflection_method_is_generator` performs a set membership query using the same canonical key as before; it must not scan and re-normalize the complete method list for every reflected method.

Do not conflate generator membership with emitted-method membership: generator status is a distinct semantic fact and gets a distinct set.

## Direct shallow metadata construction

`reflection_shallow_class_metadata_for_name` and the shallow enum equivalent must stop building full metadata and clearing discarded vectors.

Introduce a direct shallow builder with an explicit field contract. It populates from the resolved class-like metadata: `reflected_name`, `attr_names`, `attr_args`, `interface_names`, `trait_names`, `trait_aliases`, `parent_names`, `default_property_members`, `static_property_members`, `property_hook_members`, `constant_value`, `backing_value`, `is_enum_case`, `parameter_members`, `property_default_value`, `required_parameter_count`, `is_deprecated`, `is_generator`, `prototype_member`, `is_final`, `is_abstract`, `is_interface`, `is_trait`, `is_enum`, `is_readonly`, `is_anonymous`, `is_instantiable`, `is_cloneable`, `is_iterable`, `modifiers`, and `member_flags` with exactly the same values as the current full builder. It populates `type_metadata` likewise, except that the shallow enum wrapper replaces it with the resolved enum backing type exactly as today.

It always leaves these fields empty or absent, matching both current build-then-clear functions:

- method, property, and constant name vectors;
- `constant_members` and `constant_reflection_members`;
- `enum_case_members`, `method_members`, and `property_members`;
- `constructor_member`;
- `parent_class_name`.

No other field is implicitly omitted. If `ReflectionOwnerMetadata` gains a field, the comparable projection and direct shallow builder must be updated in the same change or fail to compile/test.

Before replacing the old implementation, capture old shallow metadata through a test-only comparable projection. Assert field-for-field equality between old and direct builders for a corpus containing class, inherited class, interface, trait, unit enum, backed enum, attributes, properties, constants, and methods. Remove the old builder only after this equivalence gate passes.

## Long-range AArch64 branches

Replace every raw conditional branch to a distant shared eval instance-method or static-method failure tail with target-aware helpers. The rule is structural, not a finite category list: every conditional branch whose target is either helper's shared fail tail must use a long-range sequence wherever it is emitted, including top-level receiver/scope checks, dispatch, arity validation, method preparation, and every per-argument type/ABI coercion rejection.

- For AArch64 condition-code branches, emit the inverse local condition to skip an unconditional `b target` (`b.eq 1f; b target; 1:` for a non-equal branch, using the emitter's accepted collision-safe local-label strategy).
- For AArch64 compare-and-branch instructions, invert `cbz` to local `cbnz` or `cbnz` to local `cbz`, then emit the unconditional `b target` and the local continuation label.
- x86_64 retains the equivalent direct conditional jump.
- Cover both `__elephc_eval_value_method_call_fail` and `__elephc_eval_value_static_method_call_fail`, plus their target-specific equivalents. Do not patch only the first observed assembler line or only branches emitted by the top-level helper function.
- Explicitly audit the argument coercion emitters used by both helpers: object type-hint rejection, array representation rejection, iterable rejection, and any other conditional path passed one of these fail labels.
- Existing unconditional AArch64 `b` has a +/-128 MiB range. The materializer reduction is required to make the helper smaller than that range; the branch helper alone is not a substitute for size reduction.
- Add target-specific assembly tests that separately assert the inverse-condition-plus-unconditional-branch form and the inverted-`cbz`/`cbnz`-plus-unconditional-branch form on AArch64, together with unchanged semantic shapes on x86_64. Add a source-level or assembly-level audit test which enumerates all conditional emissions receiving either distant fail label, so a later coercion path cannot silently reintroduce a short conditional branch.

## Implementation boundaries

Expected production scope:

- module-wide cache/index state;
- shared synthetic helper infrastructure;
- metadata object lowering and its nested emitters;
- shallow metadata builders;
- eval method-helper branch emission;
- focused ABI/codegen tests.

Every touched/new Rust file must retain or gain its required module preamble and every explicit function must have a specific Rustdoc docblock. Assembly instruction comments must remain aligned to column 81. Source files should stay cohesive and respect the repository's soft size policy; new materializer orchestration belongs in a focused module rather than enlarging the owner emitter.

## Verification gates

### Frozen baseline

Before production edits, create a generic ignored fixture under `target/` that exercises:

- repeated identical `ReflectionClass`, `ReflectionMethod`, `ReflectionProperty`, `ReflectionParameter`, `ReflectionFunction`, enum, parent, interface, trait, constant, attribute, and type/default metadata;
- inherited members and nested declaring objects;
- two identical constructors whose results are compared for non-identity and independently mutated/read;
- generator and non-generator method predicates.

Compile it twice with the current compiler. Record source hash, compiler HEAD, command, output, assembly hash, assembly bytes/lines, elapsed time, and max RSS. The two baseline runs must have identical output and assembly hashes.

### Focused correctness

After implementation:

1. the frozen fixture output is byte-identical to baseline;
2. repeated materialization returns distinct objects and isolated mutable collections;
3. old-vs-direct shallow metadata projections are field-for-field equal;
4. generator and non-generator results are unchanged;
5. one materializer body exists per exact key while multiple sites call it;
6. different typed owner kinds, operands, selector variants, full/shallow forms, and declaring identities do not share labels, while repeated identical owner-plus-payload keys do;
7. a deliberately failing helper emission does not leave a dangling cache entry after subtree rollback;
8. under backend-inventory mode, a body which successfully emits a materializer and then fails later rolls back both assembly and cache; a following body using the same key emits a valid body rather than calling a rolled-back label;
9. emitted assembly is deterministic across two post-change runs;
10. focused runtime Reflection tests pass on the host target;
11. focused emitted-assembly tests cover both AArch64 and x86_64; run the smallest relevant Linux target checks when locally practical.

Use `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0`, `RUST_MIN_STACK=67108864`, and offline Cargo commands. Run targeted test binaries, `cargo build --bin elephc`, `cargo check --lib`, scoped `cargo fmt --check`, the assembly-comment checker, and `git diff --check`.

### Size/performance acceptance

For the frozen fixture, record before/after assembly bytes, lines, elapsed time, and max RSS. The after assembly must be smaller, and repeated identical graphs must grow by call-site-sized increments rather than graph-sized increments.

Then rebuild the stock application with:

```text
ELEPHC_BACKEND_INVENTORY=1 RUST_MIN_STACK=67108864 target/debug/elephc --web --timings --quiet <entrypoint>
```

Record elapsed time, maximum RSS, assembly bytes/lines, object bytes, and executable bytes. Hard gates:

- no compiler or assembler errors;
- assembly is below the AArch64 unconditional-branch reach relevant to the shared helper and is at least 80% smaller than the 563,123,428-byte baseline;
- maximum RSS remains below 2 GiB;
- compilation trends toward the product target of 1-2 minutes and the final executable remains below the 140 MiB source-tree scale. Missing those product targets is a continuing optimization issue, not permission to weaken semantics.

### End-to-end web acceptance

Only after zero-error compilation:

1. start the produced `--web` executable on an isolated local port;
2. request the default route with a real browser and an HTTP client;
3. require HTTP 200 and the standard application page with no fatal/error output;
4. record server logs and terminate only the process started for this acceptance run.

Do not claim completion before this gate passes.

## Review protocol

The same final revision of this specification must receive an unqualified `APPROVE` from Fable 5, GLM 5.2, Kimi K2.7, and Kimi K3, sequentially. Any reservation changes the spec and invalidates earlier approvals; all four then re-review the new final revision. Implementation starts only after absolute consensus.

After implementation and web acceptance, run sequential code audits with those reviewers. Correct every accepted finding, repeat focused and web gates, and finish with a final Kimi K3 analysis of the exact final diff and evidence.
