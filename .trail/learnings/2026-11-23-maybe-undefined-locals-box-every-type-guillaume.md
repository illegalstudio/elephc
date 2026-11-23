---
title: Maybe-undefined locals box every type; code after a diverging operand lands in dead blocks
author: Guillaume
date: 2026-11-23
branch: bead-undef
touches:
  - src/ir_lower/local_definedness.rs
  - src/types/checker/binding_flow.rs
  - src/ir/builder.rs
  - src/ir/dead_blocks.rs
  - src/codegen/lower_inst/objects/typed_property_guards.rs
---

## Decided
- A read of a local that may be unassigned (flag-tracked by ir_lower/local_definedness) is a boxed mixed for EVERY plain slot type (int, float, bool, string, array, object), not only for mixed slots: PhpType::is_plain_binding_storage is the shared predicate. Before, only a mixed/void load could carry NULL, and an int slot read on the undefined path printed uninitialized storage (garbage int, bead elephc-08wi). The root of a probed chain (isset($box['k']), $box['k'] ?? d) keeps the typed load so ArrayAccess still gets offsetExists first.
- The checker must agree: Checker::binding_read_type joins Void into the read of a not-definitely-bound local of plain storage (superglobals excluded, they exist in every scope). That is what lets an inferred return type or an untyped parameter become ?T and print NULL instead of coercing the boxed NULL back to 0. Merge planning (merge_temps::materialized_expr_type_for_merge) asks local_definedness::read_lowers_as_boxed_mixed so a ?: or match arm is predicted mixed, not int.
- unset($x) now ends the variable for definedness (local_definedness::note_unset: run-time flag cleared, static marker dropped) and for the checker (definitely_bound_locals loses the name, also across an if/else join). Generators are no longer ineligible for the flags; method/static calls are no longer assumed to take every bare argument by reference (the builder already marks a by-reference bind of the slot as a definition).
- elephc-2kc4: a call on a receiver known to be null ends its block, and the cast, @, operator, argument list or interpolation around it kept emitting after the terminator (83 of 104 contexts panicked, not just @). The builder now opens a block named "dead" when it is asked to emit or terminate after a terminator; ir::prune_dead_blocks (run from ir_lower::program::validate_finished_module) turns the blocks only that dead code reaches into nops + unreachable before validation, because that code is not even well typed (Void operand). A block the dead chain reaches that something else also reaches stays: the join after a try is also the end of a catch, whose handler no terminator edge leads to.
- elephc-u9kr: the uninitialized typed property Error was raised through shared_static_throw with creation line 0; it now takes the instruction's line (or LINE_OF_CALLING_SITE inside a shared helper).

## Tried and ruled out
- Widening the checker read type only for the return statement: the same widening is needed for untyped parameters inferred from a call site, so it lives in binding_read_type.
- Neutralizing every block not reachable from the entry in prune_dead_blocks: catch handler blocks are entered by the unwinder, not by an edge, so the join after the try lost its code ("end" was never printed).

## Watch out
- Known residuals (not fixed): compound assignment or ++ on a variable that was never assigned anywhere in the function does not warn; `--$undefined` / `$null--` lacks php's "Decrement on type null" warning and yields -1; reading a property of a Mixed null receiver prints no "Attempt to read property on null" warning in some paths; an arrow fn does not warn when its body reads a variable the parent never defined; `use ($r)` of an unset variable is still a compile error; [$a, $b] = $maybeUndefinedArray in main loses its warning when ANOTHER maybe-undefined variable is read later in the same scope (the array local is not discovered, function scope is fine); sort() on a null variable names an internal helper in the TypeError.
- The checker's definitely_bound_locals is restored at every if branch exit, so a binding made in one branch never survives the join; only removals (unset) are carried through the join now.
