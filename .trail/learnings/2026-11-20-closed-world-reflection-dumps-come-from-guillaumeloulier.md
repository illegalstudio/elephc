---
title: Closed-world reflection dumps come from table rows
author: Guillaumeloulier
date: 2026-11-20
cost: { minutes: 421, tokens: 3499822 }
touches:
  - src/codegen/lower_inst/objects/reflection/member_table.rs
---

## Decided
- The 66 failures were a real closed-world (table-reflector) gap, not a harness bug: the CLI default is closed world, whose ReflectionMethod/Function/Parameter are built from member-table rows. Rows never carried the __toString text and the constructors never filled __string. The old harness linked the interpreter, so the per-class generated path hid it.
- Row text slots: STRING (method, function), PARAMETER_STRING=14 (parameter; STRING there is the default constant name). Dump text goes through literal_bytes (private-use markers to raw bytes), value strings in the table too.
- A method row is shared only between views that print the same dump (identity includes the text): inherits-vs-overwrites differ per reflecting class.
- ReflectionEnumUnitCase carries __backing_value so unit and backed case share one slot layout; the shared enum case row fills either class (new ReflectionEnumUnitCase(BackedEnum, ...) is valid php).
- A reflection of an absent class inside a function body defers to run time (php throws ReflectionException there); closed world has no interpreter to set the other deferral reasons.
## Tried and ruled out
- Assigning $this->name or a dynamic property in the table ctor for a user subclass of ReflectionClass: name is a virtual alias of __name, so the write never reaches the real slot (bead elephc-h17m.78).
## Watch out
- Tests that measure the per-class generated materializer must keep the interpreter linked (computed eval), since the harness now compiles closed world first.
- Open: bound closure this (h17m.76), hook ReflectionMethod getParameters crash (h17m.77), subclass name (h17m.78).
