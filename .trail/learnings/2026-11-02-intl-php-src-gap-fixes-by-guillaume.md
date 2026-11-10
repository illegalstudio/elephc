---
title: "Intl php-src gap fixes: by-ref, break iterators, ICU enumeration"
author: Guillaume
date: 2026-11-02
branch: feat/intl-managed-icu
cost: { minutes: 180 }
touches:
  - src/intl/mod.rs
  - src/native_deps/recipes/elephc_icu_intl_shim.cpp
  - src/types/checker/functions/reference_outputs.rs
  - src/types/checker/inference/objects/methods.rs
---

- PHP `mixed`/typed by-reference outputs with a *never-declared* caller variable need three checker paths to agree: the static-method pre-inference loop in `inference/objects/methods.rs`, `resolution/specialization.rs`, and the registry-builtin argument loop in `builtin_sigs`/`builtins/mod.rs`. Missing any one raises `Undefined variable` before the callee writes it.
- On the lowerer side, `promote_boxed_reference_local_argument` skips an uninitialized local (`local_is_promotable_to_ref_cell` requires `initialized_slots`). `local_is_declarable_ref_cell` plus a `boxed_reference_promotion_sites` entry at the argument span lets the call box null before binding.
- An array literal built from a nullable scalar (`[$offset]`) keeps the inline tagged-scalar representation; the bridge packer then classifies it as null/0. Route bridge args through a `mixed ...$values` helper (`__elephc_intl_args`) so each element is a boxed Mixed cell.
- `IntlBreakIterator` positions are UTF-8 byte offsets in PHP, not ICU UTF-16 indices. `IntlCodePointBreakIterator` is easier as its own UTF-8 cursor in the shim than as an ICU character iterator wrapper.
- ICU `optional_int(args, i, fallback, out)` returns true for null/missing. Filtering operations must test `args[i].type == Int` explicitly or a null filter turns into raw-offset 0.
- Native cache artifacts are keyed by `recipe_revision`. Replacing a static archive with `ar crs archive /tmp/newname.o` appends a second member; the linker keeps the old one. Compile the object under the original member name and rebuild the archive.
