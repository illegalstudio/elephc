---
id: bug_fix-an-untyped-property-of-objects-returned-through--dca80296
type: bug_fix
title: "An untyped property of OBJECTS returned through a bare array hint still miscompiles - the one payload the contract rule cannot widen"
description: "Same shape as the BufferingLogger row bug, but for object elements, where widening the contract breaks Symfony's argument resolver instead"
created: 2026-09-18
verified_by: "21-line reduction (scratchpad/objrows.php) against php -n 8.5: php prints Row:a;Row:b;2, the compiled binary prints :;:;2 and exits 139"
sources:
  - path: src/types/checker/functions/returns.rs
    blob: fadb1b166a255afd490050b064803ff18993a120
  - path: src/ir_lower/stmt/array_write_core.rs
    blob: 25aa1921f359ce30ac026c64e196003dbf5b4d3a
---

# An untyped property of OBJECTS returned through a bare array hint still miscompiles - the one payload the contract rule cannot widen

## Fact

NOT FIXED. Recorded with its reduction and with the reason a fix is not simply "widen it too".

    class Store {
        private $rows = [];                         // untyped, default []
        public function add($tag): void { $this->rows[] = new Row($tag); }
        public function all(): array { return $this->rows; }
    }
    foreach ((new Store())->all() as $row) { echo get_debug_type($row); }   // php: Row  elephc: "" + SIGSEGV

The property's ir type settles at `array<mixed>` (boxed rows) while the method's recorded return
contract keeps `array<Row>` (raw pointers) from an earlier method-pass round, so the CALL SITE reads
boxed cells as object pointers. Identical in shape to
`BufferingLogger::cleanLogs(): array`, which IS fixed: `generic_array_return_contract` now strips a
CONTAINER payload from a bare `array` hint.

WHY OBJECTS ARE EXCLUDED FROM THAT RULE: widening an object payload to `mixed` breaks Symfony.
`ArgumentMetadataFactory::createArgumentMetadata(): array` returns `ArgumentMetadata[]`, and
`ArgumentResolver` reads `$metadata::IS_INSTANCEOF` off the loop variable -- a class constant needs a
static receiver, so as `mixed` it went to the eval bridge as `dynamic::IS_INSTANCEOF` and EVERY route
with a controller argument died. Measured: `/greet/world` and `/echo` answered 000 until the
exclusion went back in.

SO THE REAL FIX IS UPSTREAM: make the recorded return contract agree with the property type the
method pass SETTLES on, instead of the one an earlier round saw. The contract is written in
`type_check_methods_until_stable` from `return_infos`; the property type keeps moving under it.
Until then, the object case stays open and the reduction above is the pin.

## Why

It is the half of the container-payload family that is still open, and the reason the fix stops where it does
