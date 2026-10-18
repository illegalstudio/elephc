---
title: Inherited AOT methods must satisfy eval interface signatures
author: Guillaumeloulier
date: 2026-10-14
branch: review/1758-inherited-interface-contract
cost: { minutes: 0, tokens: 0 }
touches:
  - crates/elephc-magician/src/interpreter/statements/interface_contracts.rs
  - crates/elephc-magician/src/interpreter/statements/abstract_requirements.rs
  - crates/elephc-magician/src/interpreter/statements/trait_declarations.rs
  - tests/codegen/eval_inherited_interface_contracts.rs
  - tests/codegen/eval.rs
  - tests/codegen/mod.rs
---

## Decided
For PR #1758 / issue #1736, inherited generated/AOT methods must satisfy an interface contract, not merely expose dispatch metadata with the same name. Compare the inherited method static flag to the requirement, reject non-public/abstract implementations, then retrieve its registered signature using the actual declaring class.

Compare visible arity/optionality/variadics/by-reference flags through method_signature_accepts, parameters through method_parameter_type_signature_accepts, and returns through method_return_type_signature_accepts. These are the same primitives used for eval-declared implementations. Resolve relative types in the native method owner and the interface requirement owner, with the eval child as the pending class.

Preserve the existing metadata-only fallback when a builtin inherited method has no registered signature. Throwable::getMessage and legal exception descendants must still work, including aliases and two eval ancestors before the native parent.

## Tried and ruled out
Treating all inherited AOT methods as automatically PHP-compatible accepts independent Service methods with incompatible interface return types, required arguments, references and parameter types. Requiring every implementation to be non-static rejects legal static contracts. Inferring compatibility from the child scope misinterprets an inherited self return type.

## Watch out
Pin both source and the linked Magician archive for baseline comparisons. A codegen probe may automatically rebuild bridge archives; editing source while that replay is running can mix versions. One initial replay was discarded for that reason. The replacement baseline executable and archive are frozen under /private/tmp/elephc-1758-review, with all 13 cases selected at runtime from a string array. An unrelated AArch64 switch branch-range failure discovered in a larger aggregate witness is retained for a separate main-branch follow-up.

Validation already verified before saving: all 13 PHP-oracle declaration decisions on the corrected native aggregate, heap-clean accepted cases, 50 focused Magician interface tests, and the four original Throwable/native-parent controls. Individual new regressions and IR-off replay are recorded separately in the committed plan. Time/token costs were not measured and are explicitly zero.
