---
title: "Hosted extension review: teardown steps, strict writes, overrides, internals by name"
author: Guillaume
date: 2026-10-28
branch: fix/php-ext-followups
---

## Decided
- Request teardown runs one bailout frame per step (each module's RSHUTDOWN, then close resources, free held objects, restore INI, destroy resources, post_deactivate, sweep), as php_request_shutdown's zend_try blocks do. Modules join DEACTIVATING before RSHUTDOWN, so a fatal RSHUTDOWN cancels nothing; the exit hook then exits 255 when EG(exit_status) is 255.
- Engine objects wrappers still hold at deactivation are freed (free_obj, no destructor, memory after all free_obj) like zend_objects_store_free_object_storage; a late wrapper release finds no handle and is a no-op (handles are never reused).
- Program property accesses are a stack (nested callbacks); strictness rides the magic-scope cell (offset 16) from a DynamicPropSet carrying Immediate::Bool(true) in strict files; the wrapper accessor passes the taken scope ("!" prefix = strict) to elephc_php_ext_property_as.
- A parent-private name written from a subclass scope lives in a per-object side table (SHADOWING) because the engine keys declared props by plain name; dynamic creation deprecates/forbids per ce_flags.
- Wrapper __get/__set/__isset/__unset/__clone impose no signature on program overrides (checker exemption); clone of an overriding subclass calls __elephcClone first (clone_with.rs). Every magic call landing in a hosted class publishes; a program override (rewritten by src/php_ext/overrides.rs after autoload) takes the scope first, asks the engine (__elephc_php_ext_direct) and only runs its own body for names the engine does not answer, as php-src std handlers do.
- Compiler internals (__elephc*, elephc_php_ext_*) are dropped from runtime name lookups: string-callable cases, array-callable method targets, _classes_by_name, _instanceof_target_entries, dynamic new candidates.
## Tried and ruled out
- php-src mangled keys for private/protected engine props: right layout, but elephc's var_dump/print_r print __debugInfo keys raw (no unmangling), so hosted dumps would regress until the runtime unmangles (bead eun.14).
## Watch out
- Real-PHP oracle: scratchpad phpdemo/ (phpize build of tests/fixtures/php_ext/demo); rebuild after demo.c edits; expected files are php stdout with display_errors=stderr.
- Test builds need RUSTUP_TOOLCHAIN=1.98 CARGO_TARGET_DIR=target-chk (mago dev-deps need rustc 1.97); `cargo build --bin elephc` works on 1.95.
- Wrapper plumbing is still visible to get_class_methods, method_exists, class_implements, reflection (eun.17); get_object_vars/(array)/property_exists ignore engine props (eun.15). Trait-supplied accessors on hosted subclasses are not rewritten.
