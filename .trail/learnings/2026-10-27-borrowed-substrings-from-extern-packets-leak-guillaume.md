---
title: Borrowed substrings from extern packets leak through call-argument cleanup
author: Guillaume
date: 2026-10-27
branch: feat/gmp-extension
cost: { minutes: 25, tokens: 744189 }
touches:
  - src/types/return_alias.rs
  - src/ir_lower/expr/function_calls.rs
  - src/ir_lower/context.rs
  - src/gmp_prelude
---

GMP's memory test (bug67917) failed because every bridge helper leaked ~3 blocks per call. The chain: an extern returning char* is copied by __rt_cstr_to_str into an owned PHP string; passing that result straight to another call was suppressed because the callee (__elephc_gmp_value) returned string and the alias summary was Unknown (substr is MayAliasArguments), so the extern copy was never released. Two fixes: (1) extern calls never alias their arguments because cstr returns are copied, so their argument cleanup uses ReturnArgAlias::None; (2) GMP packet unwrapping now returns an owned concat (substr(...) . '') and return_alias treats BinOp::Concat as fresh, so __elephc_gmp_value's callers can release the extern packet safely. Also found that dynamic string dispatch of EIR-only builtins produced a null-returning wrapper; runtime_wrappers now routes BuiltinRuntimeFunctions::None + BuiltinLowering::Eir through lower_prepared_builtin_callable.
