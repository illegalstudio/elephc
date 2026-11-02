---
title: "Laravel closed-world round: namesake typing, $this narrowing, filter ABI, Serializable catalog, BcMath\\Number"
author: Guillaume
date: 2026-10-27
branch: reconcile/dirname-symfony
touches:
  - src/bcmath_number_prelude.rs
---

## Decided
- A gradual (`mixed`) receiver's method call is `mixed` whenever some class lacks the method but declares `__call` (`mixed_receiver_method_return_type`): Faker's `month(): string` typed Carbon's `->day(1)->month()` chain otherwise.
- A narrowed `$this` (`\x01this\x01$this`) survives `purge_property_narrowings`: no call changes the running body's class. `$this->redirect(...) + $ret` after `if (!$this instanceof I) throw` re-infers its operands after the call purge.
- Stream filter contract ABI only for classes `stream_filter_register()` names literally (or descendants, or `php_user_filter` ones); `None` (dynamic name) keeps the old any-class-with-`filter` rule (`types::is_user_filter_class`, `builtin_class_gate::user_filter_registered_classes`).
- `Serializable` added to the builtin class catalog: the autoloader's bind check (`autoload_target_can_bind`) only knows catalogued builtins, and an interface extending an uncatalogued one is dropped SILENTLY (no warning on its file, only on implementers).
- `BcMath\Number`: AST prelude `src/bcmath_number_prelude.rs` (backend-gap group, declared by resolved name since it is injected after name resolution, no `namespace` block). Operators: checker `infer_binary_op_type`/Negate record the span (global `OPERATOR_SITES`), lowering `lower_binary`/`lower_numeric_unary` rewrite to `Number::__elephc_arith/__elephc_compare`; inner `cmp < 0` must carry `Span::dummy()` or lowering recurses.
- Closed world: `Closure::fromCallable([$obj, $name])` (array literal) goes through the gradual conversion, not the eval bridge; named-arg static call to an absent class is `Class not found` like the positional route.
- DateTime instant-key compare accepts subclasses; the key consumes its operand (two prop gets release an owning temporary each).
- Offset timezones (`+02:00`) reach libc sign-flipped through `_php_tz_posix`.
## Tried and ruled out
- Making non-literal arithmetic `may_throw` in `optimize/effects.rs` (to keep `$n + 'x';`): breaks the typed-int callee try-pruning test. Bead filed.
## Watch out
- `elephc-scratch/elephc.toml` (Oct 16, lists php-src) breaks compiles of probes needing natives under elephc-scratch; use --check or another dir.
- `ELEPHC_DEFERRAL_MEMO=0` when re-testing a refusal: the memo replays old reasons ("remembered from an earlier build").
- leakmin: match 'leak summary: live_blocks=' (the alloc line also has live_blocks=0).
