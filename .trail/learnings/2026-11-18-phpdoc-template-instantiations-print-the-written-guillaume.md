---
title: PHPDoc template instantiations print the written class name
author: Guillaume
date: 2026-11-18
branch: bead-php-parity
cost: { minutes: 566, tokens: 6343347 }
touches:
  - src/codegen_support/runtime/data/user.rs
---

## Decided
A PHPDoc-only class template stays the written class for php: ClassInfo.written_name (set from the monomorphizer: thread-local set filled in instantiate_declaration, drained into check_result.classes at the end of monomorphize_body) feeds the runtime class-name table and ConstClassName. Native class Box<T> keeps Box<int> (elephc extension, not php).

## Tried and ruled out
A process-wide name registry: tests compile in one process and a native Box<int> in another test was renamed.

## Watch out
Still leaking: instanceof $obj / is_a with a different instantiation, class_exists("Box") (the marker is an interface), get_parent_class returns "" not false, eval-created names, Reflection names.
