---
title: Closure calls through known bindings no longer keep every wrapper and the session prelude
author: guillaume
date: 2026-11-21
branch: bead-callable-bloat
touches:
  - src/optimize/reachability/usage/expressions.rs
  - src/optimize/reachability/usage.rs
  - src/prelude_prune/usage.rs
  - src/prelude_prune/callable_names.rs
  - src/session_prelude.rs
---

A closure call through a variable ($f = strlen(...); $f("abc")) made the program keep ~400 _eir_shared_callable_builtin wrappers, 270 extern wrappers and the whole CLI session prelude (339k asm lines vs 53k). Two gates fired on any dynamic call, not on the CLI/optimizer code the earlier hypothesis blamed:
1. src/session_prelude.rs forces the session group when usage.dynamic_function_call is true (main's CLI session surface; the pre-merge branch had none). The session functions then hold the whole function index alive through the reachability dynamic_function hazard.
2. src/optimize/reachability/usage/expressions.rs ClosureCall/ExprCall raised unresolved_callable_hazard unconditionally; with every prelude function declared, that kept all of them, and the lowering builds a runtime callable ladder (one wrapper per builtin) once it cannot bound the callee.
Fix: settle `$f(...)` through binding facts of the scope. Reachability Scanner::finish resolves a called variable bound only to closures, first-class callables, and copies/ternary/match/?? of those (alias fixpoint, params, by-ref args, global/$GLOBALS['x']= writes, extract() all make it opaque; literal-string bindings stay a hazard by contract of scan_literal_eval_keeps_the_fragments_own_hazards; `($f)()` ExprCall stays conservative per prune_expr_call_keeps_methods_on_live_classes). Usage.resolved_callable_calls lets the graph re-raise the hazard when another scope rebinds the name. prelude_prune: Usage::reaches_unseen_callable + CallableNames::has_unknown_target_through_copies feed the session gate. Note `$GLOBALS['f'] = x` parses as Assign to the pseudo-local "GLOBALS[f]", not ArrayAssign.
Not fixed: a callable PARAMETER ($callback()) is still an open ladder (all builtin wrappers); test_dead_code_elimination_models_constrained_unknown_rethrow_domain fails to link in the feature-less harness runtime because of it. Needs provenance across the relay() call boundary (callable_reachability / callable_parameter_inputs.prove). Also unrelated lib failures: prune_dynamic_globals_array_alias_widens_all_receiver_names (parser refuses computed $GLOBALS[$name]) and test_function_effects_keep_declared_int_return_verification (may_throw).
Regression tests: codegen test_closure_call_through_known_alias_keeps_only_its_target_wrappers (<=4 wrappers, no session prelude, <60k lines); reachability prune_closure_call_through_{known,opaque}_bindings_*; prelude_prune variable_calls_through_known_bindings_do_not_reach_unseen_callables.
