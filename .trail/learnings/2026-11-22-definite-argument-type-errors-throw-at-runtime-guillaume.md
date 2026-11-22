---
title: "Definite argument TypeErrors throw at run time; --strict-locals keeps the refusal"
author: Guillaume
date: 2026-11-22
branch: bead-1w3w
touches:
  - src/types/checker/functions/param_binding.rs
  - src/types/checker/builtins/mod.rs
  - src/optimize/exception_flow.rs
  - src/ir_lower/expr/array_access.rs
  - src/ir_lower/expr/native_isset.rs
  - tests/codegen/runtime_checked_refusals.rs
---

## Decided
- `f("x")` for `function f(int $i)` (a non-numeric/out-of-range string or NAN literal at an `int`/`float` parameter) and `strlen([])` (an array at a builtin `string` parameter) compiled to a refusal although php runs them and throws a catchable `TypeError`. Both now reuse `ThrowAccessKind::CallArgumentType` (lowered by `definite_argument_errors.rs`): the checker records the site and the call throws php's message. User functions get the `, called in FILE on line N` clause, builtins do not.
- `--strict-locals` keeps both refusals (error tests moved to `expect_error_strict` / `expect_builtin_strict_error`).
- Builtins whose php signature takes `array|string` where elephc narrows to `string` (`php_accepts_array_at_string_param`: substr_replace, preg_replace*, strtr, implode...) keep the compile refusal: a throw there would be wrong.
- The AST optimizer (`exception_flow`) prunes `catch (TypeError)` around calls it believes cannot throw, and it runs before the checker. `builtin_array_literal_at_string_param` marks an array LITERAL at a string parameter as throwing TypeError; without it the try/catch vanished from the IR and the TypeError escaped.
- A silent read (`??`, `isset`) keyed by a boxed checked-arithmetic result (`$a[$i + 1]`) converts a float key and warns "not representable as an int"; the read now advertises `MAY_WARN` so the warning gets its own line (it printed no location or the previous statement's line). `ArrayGetSilent` and `ArrayGetMixedKeySilent` allow effect refinement for this.

## Watch out
- Uncaught user-function TypeError still differs from php: php throws inside the callee (`and defined in FILE:LINE`, `#0 FILE(N): f('x')`, `thrown in ... on line <decl line>`); elephc throws at the call site with only `#0 {main}`. The runtime int-coercion path (`CallArgumentIntCoercion`) has the same gap.
- A bare array VARIABLE passed to a builtin `string` parameter inside a try is not seen by the optimizer, so a `catch (TypeError)` around it can still be pruned; only array literals are covered.
- Standalone `strtoupper([1]);` (result unused) is dropped as pure by the AST optimizer, so php's TypeError is not raised there.
- The base also fails ~50 error tests (nullable_fallback_flow, list_hash_param_binding, ...) that 42a2eab3d1 migrates; they are not caused by this change.
