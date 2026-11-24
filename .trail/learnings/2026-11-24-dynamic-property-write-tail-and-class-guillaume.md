---
title: Dynamic property write tail and class-id tables
author: Guillaume
date: 2026-11-24
branch: bead-dynset
---

A write site is a call: declared-slot ladder in a per-name/scope-group helper, everything else in ONE module-wide tail (class-id to __set group table; the receiver's _class_dyn_hash_rows row gives hash offset, deprecation flag and refusal text at runtime). The tail must never be outlined into the global set helper (infinite recursion): inside_magic_set_tail guards try_lower_outlined_mixed_prop_set. The tail is used only above 8 __set/hash classes. Shared helpers raise runtime-composed throwables on the calling statement's line via _php_site. Runtime-name reads must try main's scope-grouped helper before shared_property_read (one whole ladder per lexical class). Synthetic harnesses did not reproduce the 8 MB erased-write helpers; DOM+SimpleXML+Reflection preludes are a fixed ~3M asm lines.
