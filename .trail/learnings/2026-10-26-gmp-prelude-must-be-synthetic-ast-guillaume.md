---
title: "GMP prelude must be synthetic AST, never parsed PHP text"
author: Guillaume
date: 2026-10-26
branch: feat/gmp-extension
cost: { minutes: 20 }
touches:
  - src/gmp_prelude.rs
  - src/gmp_prelude
  - files
---

Review correction: no PHP source may be parsed outside tests. The first GMP iteration used include_str!(prelude.php) + parser::parse, matching the curl prelude precedent but violating the AST-only rule. Rebuilt the whole surface under src/gmp_prelude/ with synthetic_class builders: dsl.rs (op codes + expression/statement vocabulary), class.rs (final GMP, Serializable, storage, casts, operators), validators.rs (packet unpacking, php-src 8.5 coercion/error messages), functions.rs (51 wrappers generated from family builders). Also learned that emit_user_call must receive the callee's exact return representation: array helpers return Heap(Array)/Heap(Hash), so the builtin result resolver must say Array(Mixed) or AssocArray{Str,Mixed}, not PhpType::php_array() (Heap(Mixed)), or var_dump prints pointer garbage. Dynamic GMP properties need AllowDynamicProperties for checker admission plus ClassContract::dynamic_properties_deprecated so codegen still emits PHP 8.5's deprecation.
