---
title: Docblock-adopted class generics must be marked doc_annotated
author: Guillaume
date: 2026-11-20
branch: bead-generics
cost: { minutes: 210, tokens: 2407894 }
touches:
  - src/docblock.rs
  - src/generics/classes.rs
  - src/docblock/erasure.rs
---

## Decided
docblock::adopt_generics builds the class GenericDecl with `GenericDecl::with_origin(.., true)`. The branch's `forget_doc_inheritance_of_non_templates` (generics/classes.rs) only fires on `doc_annotated` declarations; main's adoption used `GenericDecl::new`, so after the merge nothing was ever marked and `@extends P<T>` on a non-template P was refused ("declares no type parameters"). That broke Symfony type-info: `@template T of class-string` does not parse (class-string is no type of the grammar), so ObjectType is no template, and EnumType's `@extends ObjectType<T>` was refused. The erasure re-run (docblock::erasure) then hid it behind a whole second compile.
## Tried and ruled out
Erasure/deferral interaction was only the symptom: the refusal came from the first round. Parsing class-string<...> was not needed.
## Watch out
Repro: copy vendor/symfony/type-info, require every file from one script, --check. The failure shows as "is compiled with its written types" notes on stderr; the regression test asserts their absence.
