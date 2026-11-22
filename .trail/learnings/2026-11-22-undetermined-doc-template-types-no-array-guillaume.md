---
title: "An undetermined PHPDoc template types no array element"
author: Guillaume
date: 2026-11-22
branch: bead-refusals2
touches:
  - src/generics.rs
  - src/generics/erasure.rs
  - src/generics/splice.rs
  - src/generics/methods.rs
  - src/docblock.rs
---

## Decided
- After the 13 refusals of `bead-refusals` the Demo's two erasure re-runs still came from two doc-derived refusals: Symfony `ArgumentMetadata::getAttributesOfType()` (`@template T of object`, `@param class-string<T>`, `@return array<T>`) and Doctrine `InsertBatch::batchByEntityType()` (`@param list<TEntities>`, `@return list<self<TEntities>>`). `class-string<T>` and a plain `array` argument bind nothing, so `T` read as its bound `object` and the signature became `array<object>`: raw object pointers, where the body builds boxed cells and the caller passes a boxed array. "the element storage differs" / "expects array<object>, got array".
- `docblock_params_read_bare` (function and method templates only, not classes) adds the params bound to nothing but their own bound to the mixed-read ones; `erase_containers` then reads every array that mentions such a param as the plain `array` php declares (also `list<self<T>>`). A bare `T` keeps its bound reading. Class-level templates are untouched.
- `DocBlock::unreadable_template` (a `@template T of Builtin<Id::A>|Obj` the reader cannot parse) still makes the class a generic declaration for the people who mention it; before, `Builtin|Obj $type` warned "names a template" (type-info `GenericType`).

## Watch out
- `$a instanceof $name` with a dynamic `$name` pulls the pcre2 native package into a CLI test project: use `$a::class === $name` in tests.
- error_tests has 85 failures that are identical on the pre-change binary (merge integration, arg-count messages, nullable-flow strict tests); they are not caused by generics work.
