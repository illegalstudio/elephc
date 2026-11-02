---
id: bug_fix-array-property-layout-must-be-canonical-before-i-486b4fd7
type: bug_fix
title: "Array property layout must be canonical before inferred forwarding returns"
description: "Late array property normalization changes a getter ABI after its callers inferred packed arrays; canonicalize before body inference and propagate through the full function chain"
tags: [array-storage, return-abi, type-inference]
created: 2026-10-21
verified_by: "cargo test -p elephc --lib types::checker::driver::tests; cargo test -p elephc --lib ir_lower::tests::arrays; cargo test -p elephc --test foreach_non_iterable_tests"
stale_when: "The untyped property representation or return inference ordering changes. Full framework behavior still requires separate current-source verification."
sources:
  - path: src/types/schema.rs
    blob: 0696bac331ab5f1984f530cf8d6c7d440d326e2e
    lines: 816-845
    snip: 7548541d9761
    anchor: "pub(crate) fn normalize_untyped_array_property_storage(&mut self) {"
  - path: src/types/checker/driver/mod.rs
    blob: de1f9ebcfeeb61f31966c0450bf34961186c16c6
    lines: 460-526
    snip: 9bbe59d97faf
    anchor: "for class_info in checker.classes.values_mut() {"
  - path: src/types/checker/stmt_check/assignments/properties.rs
    blob: fff3e9fd21530235628dff1bb9386fc5e1d403cf
    lines: 1091-1097
    snip: 074957b7ecc9
    anchor: "refined_ty"
  - path: src/ir_lower/program/metadata.rs
    blob: 12831dabdc73bb7b3c89a15eedbc18a351735c9c
    lines: 144-161
    snip: abca6d44eac0
    anchor: "fn normalize_untyped_instance_storage_for_eir("
  - path: src/ir_lower/tests/arrays.rs
    blob: c3058bdcaeceb9a74eac4a140b4b50240aea3bb6
    lines: 12-68
    snip: 5a1f8ca9d836
    anchor: "fn inferred_array_property_forwarding_uses_canonical_storage_on_every_target() {"
  - path: tests/foreach_non_iterable_tests.rs
    blob: d130837670acae2ebbe248a0e32e29be510567ad
    lines: 202-350
    snip: 7cc0852d066b
    anchor: "fn forward_method_array_result_compiles_without_scalar_warning() {"
supersedes: "bug_fix-array-property-layout-must-be-canonical-before-i-fbd355b1"
---

# Array property layout must be canonical before inferred forwarding returns

## Fact

The physical storage of an untyped array property is a hash with generic keys and boxed values. Establish that representation before the first function body is inferred, and reuse the same rule when a constructor write first turns a null-defaulted slot into an array. EIR metadata must call the shared rule rather than change the getter ABI after its callers have already inferred packed string arrays.

A function forwarding the getter otherwise retains Array Str while the getter returns Hash Mixed. The generic return conversion emitted only pointer loads and stores; foreach then read hash words as packed strings and crashed in string persistence. Both frozen and candidate compilers reproduced signal 11 before this repair.

Return propagation also needs enough rounds for the declaration graph. A chain of 32 forwarding functions exposed the old fixed three-round limit, leaving the outer function on an int seed. The driver now permits a declaration-length chain and still exits early when stable. This does not blanket-widen independent literal-array returns or the object-array contract used for nominal class-constant reads.

Verified on the current dirty worktree with 10 driver tests, 41 array EIR tests including code generation for all five supported targets, and 17 native integration tests on macOS ARM64. Native controls preserve string keys, snapshots after mutation, values after the owning object is unset, null-defaulted constructor initialization, dynamic return widening, independent object-array class constants, and the complete 32-function chain. The framework vault-list lifetime divergence and web RSS growth are separate open gates; these focused results do not close them.
