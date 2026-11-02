---
id: gotcha-widening-a-bare-array-contract-s-object-payload--802fa246
type: gotcha
title: "Widening a bare array contract's OBJECT payload breaks every Symfony route that takes a controller argument"
description: "The class constant read off a loop variable needs a static receiver; mixed sends it to the eval bridge as dynamic::CONST"
created: 2026-09-18
verified_by: "seq.sh over one worker: /greet/world and /echo answered 000 with 'Undefined constant dynamic::IS_INSTANCEOF' in the log, where the same routes were byte-identical to php -S before; excluding Object from the widened payloads restored them"
sources:
  - path: src/types/checker/functions/returns.rs
    blob: fadb1b166a255afd490050b064803ff18993a120
---

# Widening a bare array contract's OBJECT payload breaks every Symfony route that takes a controller argument

## Fact

`generic_array_return_contract` must NOT widen an OBJECT payload to `Mixed`, even though it does
widen array and hash payloads.

WHY IT IS SAFE TO KEEP: an object slot holds a pointer whose class is read from the object itself,
so every writer stores the same representation and the contract cannot drift from it. (The hash
write rule `hash_storage_represents_written_value` makes the same exception, for the same reason.)

WHY WIDENING IT BREAKS: Symfony's `ArgumentMetadataFactory::createArgumentMetadata(): array`
returns `ArgumentMetadata[]`, and `ArgumentResolver::getArguments()` reads

    $metadata->getAttributesOfType(ValueResolver::class, $metadata::IS_INSTANCEOF)

off the loop variable. `$metadata::CONST` on a `mixed` has no static receiver, so
`lower_dynamic_scoped_constant` takes the class-string path and its fallback asks the eval bridge
for the literal class name `dynamic` -- `Undefined constant dynamic::IS_INSTANCEOF`, thrown on
EVERY route that takes a controller argument, while `/` and `/plain` (no arguments) kept working.

THE TELL: a fatal naming the class `dynamic` always means a scoped constant or static whose
receiver lost its nominal type. Grep the failing constant across the vendor tree for a `$var::`
receiver, and look for what widened that variable.

## Why

It is the counter-example to 'widen the payload when in doubt', and it cost a full build cycle to find
