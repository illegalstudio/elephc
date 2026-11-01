---
id: bug_fix-an-override-that-narrows-its-return-keeps-its-pa-6a9e8323
type: bug_fix
title: "An override that narrows its return keeps its parent's return storage"
description: "A parent-typed vtable call read a boxed cell while a narrowed override returned a raw object"
created: 2026-09-23
sources:
  - path: src/types/checker/method_pass.rs
    blob: 075171ec3c798cc9d557740d77507e31a8c8d1a1
  - path: src/types/checker/schema/classes/methods.rs
    blob: 648cd5509bf2593e412a5485352125b8f22f9156
---

# An override that narrows its return keeps its parent's return storage

## Fact

PHP lets an override narrow its declared return (covariance). A call site typed at the ancestor that owns the vtable slot reads the result in the ancestor's representation. When the override's representation differs (ancestor mixed/union, override object/string/int), adopt the ancestor's return type as the override's signature return. The schema does it in adopt_ancestor_return_storage, and method_pass.rs must repeat it in ancestor_return_storage: the method pass re-resolves the declared hint and would narrow it straight back. A parent's private method and __construct are exempt. Repro: interface with load(): mixed, abstract class implementing it without declaring load, concrete subclass load(): Coll, called through the abstract-typed receiver.

## Why

FileLoader::import() called load() on a FileLoader-typed receiver; routing YamlFileLoader::load(): RouteCollection returned a raw pointer and the import came back null, so every route file under a glob vanished
