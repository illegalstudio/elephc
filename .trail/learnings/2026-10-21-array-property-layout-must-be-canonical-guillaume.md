---
title: Array property layout must be canonical before return inference
author: Guillaume
date: 2026-10-21
branch: reconcile/dirname-symfony
touches:
  - target/framework-audit-20261020.Wcomr7
  - ".plans/symfony-web-execution-ledger.md"
---

## Decided
Normalize untyped array property slots to their existing hash-of-boxed-values runtime layout before any free function is checked. Reuse ClassInfo::untyped_array_property_storage when the first write initializes a null slot, and reuse the same policy in EIR metadata. A getter and every forwarding function now infer the same representation. Keep declared properties, static properties and independent literal-array returns under their existing contracts.

Permit return reinference through a full declaration-length chain, with early exit when stable. The former fixed three rounds left a 32-function chain on an int seed even after the property ABI repair.

## Tried and ruled out
The old late normalization left an inferred Array Str return over the getter's Hash Mixed pointer. The return conversion emitted no conversion instructions, and native foreach crashed on a null string pointer with length four. Frozen and candidate compilers both reproduced signal 11. Fixing the warning lifecycle alone did not repair the ABI. Canonicalizing array defaults alone also misses a first array write in a constructor, so that write uses the shared rule too. Blanket widening of unrelated object-array returns is unnecessary and risks nominal class-constant behavior.

## Watch out
Current evidence is 10 driver tests, 41 array EIR tests including five-target assembly generation, and 17 native integration tests passing on macOS ARM64. The controls include keys, copy separation, returned owners after object destruction, null defaults, 32 forwarding functions and an unrelated object-array class constant. Logs and frozen compilers are under target/framework-audit-20261020.Wcomr7; the single current plan is .plans/symfony-web-execution-ledger.md.

These tests do not close the independently observed vault-list use-after-free, web memory growth or complete framework acceptance. Compiler inputs remain exclusively in lazy-petting-popcorn; scratch target-f stores Cargo outputs only. No commit or app/vendor modification belongs to this checkpoint.
