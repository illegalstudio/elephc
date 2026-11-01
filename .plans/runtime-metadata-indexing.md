# Runtime metadata indexing specification

## Checklist

- [ ] Build emitted-method coverage once per dynamic-class metadata selection.
- [ ] Replace global member-candidate cross products with per-class candidate sets.
- [ ] Build one canonical class-name index for parent traversal.
- [ ] Preserve registry rows, list ordering, visibility, inheritance, and diagnostics exactly.
- [ ] Run focused semantic, deterministic-output, hygiene, and large-build performance gates.

## Evidence and goal

Two samples of the same large web-mode compilation exposed successive polynomial costs after EIR
assembly emission:

1. dynamic class selection rebuilt the complete emitted EIR/intrinsic method set for every class;
2. member-existence metadata crossed every class with every method/property in the module, then
   walked parent chains whose class lookup linearly rescanned and recanonicalized the class map.

The goal is to remove only this repeated work. Runtime-visible class/member sets, visibility rules,
case rules, inheritance behavior, table layout, byte ordering, and diagnostics must remain unchanged.

## Emitted-method coverage reuse

`dynamic_instanceof_class_names(&Module)` builds `emitted_class_method_keys(module)` exactly once.
It passes the immutable borrowed set to a private
`class_metadata_supported_for_dynamic_instanceof_with_methods` helper for every candidate class.
The existing public-within-codegen predicate may remain as a compatibility facade, but the loop
must never rebuild the set per class.

The set construction remains unchanged: EIR class methods use the raw class prefix from the final
`rsplit_once("::")`, `php_symbol_key` is applied only to the method suffix, the static flag remains
part of the key, and intrinsic wrapper specifications are added exactly as before. No `HashSet`
iteration may choose emitted output order.

## Member registry construction

The old global `all_method_candidates` and `all_property_candidates` cross products are redundant.
For each class, the old predicate can only accept names present in that class's flattened metadata
or, for object method probes only, in an ancestor's method metadata. The new builder therefore
derives candidates per row:

- class-string method candidates are the canonicalized union of that class's instance and static
  method keys;
- object method candidates are that union plus the canonicalized instance/static method keys of
  every reachable ancestor;
- property candidates are the case-sensitive union of that class's instance and static property
  visibility-map keys.

Candidate sets are `BTreeSet<String>` so final list ordering stays byte-for-byte identical to the
old globally sorted candidates. Candidates are still passed through the existing visibility and
existence predicates; direct metadata iteration must not bypass private-method or private-property
rules.

Parent traversal uses one immutable `HashMap<String, &ClassInfo>` built before row construction
from the complete, unfiltered `module.class_infos`, including internal synthetic classes that do
not receive rows but may still appear in a parent chain. Its key is exactly
`php_symbol_key(raw_name.trim_start_matches('\\'))`. Parent names are normalized with the same
expression before lookup. A per-traversal canonical `visited` set retains the old cycle behavior.

The checked frontend guarantees case-insensitive class-name uniqueness. To define deterministic
behavior even if malformed metadata violates that invariant, index construction first sorts all raw
class keys by unsigned UTF-8 bytes, then performs first-wins insertion in that order and issues a
debug assertion on a duplicate canonical key. Plain `HashMap` iteration may never choose the
winner.

The raw class name used by visibility checks remains the row's original `module.class_infos` key.
The row sort key remains `php_symbol_key(class_name.trim_start_matches('\\'))`. Internal synthetic
class filtering, 64-byte row layout, label naming, explicit string lengths, empty-list encoding,
and row/list emission are unchanged.

## Exact semantic equivalence

For a class-string method probe, the result is true only when the flattened class metadata contains
the canonical method in its instance or static map and the old declaring-class/private visibility
predicate accepts it.

For an object method probe, the result is true when the flattened class metadata contains it or any
reachable ancestor declares it in its instance/static method map, including inherited private
methods. Missing parents and parent cycles remain false for the ancestor-only path.

For a property probe, the result is true only when the class's flattened instance/static visibility
metadata contains the exact case-sensitive property name and the old private declaring-class rule
accepts it. Ancestor-private properties absent from the flattened child metadata remain absent.

## Verification

Focused end-to-end tests cover native class/object member probes, inherited private and protected
methods, instance/static methods, case-insensitive method names, case-sensitive properties, private
properties, missing members, parent class constants, and dynamic eval/AOT metadata interaction.

Malformed-module unit coverage additionally pins the ancestor frontier: an ancestor reached before
a missing grandparent still contributes its methods; a method that would exist only beyond a
missing parent remains absent; and a parent cycle terminates with the same false ancestor-only
answer as the old traversal.

The reproducibility corpus is exactly `target/runtime-metadata-corpus.php` at HEAD
`72fe59b6708a2a164000730a3a293d6e9d8dfff3`, plus the pre-implementation dirty source state
identified in `target/runtime-metadata-baseline/manifest.txt` by SHA-256 hashes of the compiler
binary, corpus, `src/codegen/runtime_metadata/classes.rs`, and
`src/codegen_support/runtime/data/member_exists_registry.rs`. It covers class-string and object
member probes, inherited private/protected methods, instance/static methods and properties,
case rules, missing members, and a variable-RHS `instanceof` over a class/interface hierarchy.

Before implementation, emit that corpus twice with the same checked compiler and exact command
shape:

```text
target/debug/elephc --emit-asm --quiet --output-dir \
  target/runtime-metadata-baseline/old-N target/runtime-metadata-corpus.php
```

Retain both `.s` files and their SHA-256 values in the manifest. `cmp -s` must first prove the two
old assemblies identical. After implementation and rebuilding the compiler, emit to `new-1` and
`new-2`; require old-1 versus new-1 and new-1 versus new-2 to be byte-identical. Whole-assembly
identity is the authoritative semantic and determinism gate, and strictly subsumes scoped table
comparison. If it fails, use symbol-delimited extracts only as diagnostics: the member region runs
from the first `_memex_` label through the payload of `_member_exists_table_count`, while the
dynamic-instanceof region runs from `_instanceof_target_count` through the final
`_instanceof_name_interface_*` payload before its trailing `.p2align 3`. Record the extraction
command and hashes beside the artifacts rather than accepting a merely visual diff.

Run the targeted formatter check, assembly-comment validation for touched codegen files,
warning-free library compilation, and `git diff --check`.

Finally rerun the same exhaustive web-mode build with backend inventory enabled. Record elapsed
time, peak RSS, generated assembly/object/executable sizes, and a fresh sample if the build still
exceeds five minutes. Any later optimization must be driven by that new profile and reviewed as a
separate semantic change.
