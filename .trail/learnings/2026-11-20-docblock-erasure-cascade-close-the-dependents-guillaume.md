---
title: "Docblock erasure cascade: close the dependents, name unquoted classes"
author: Guillaume
date: 2026-11-20
branch: bead-generics
cost: { minutes: 22, tokens: 19589 }
touches:
  - src/docblock/erasure.rs
  - tests/codegen/generics_docblock_methods.rs
---

## Decided
After the doc_annotated fix the Symfony Demo still erased one file per whole-compiler re-run (8 rounds, cap hit, refused). docblock::erasure now closes the erased set: `with_dependents` adds every adopting file that writes `Name<` for a class an erased file declares, to a fixpoint (textual scan, over-erasing only drops annotations php never reads). `class_names_in` also reads unquoted qualified names, because "Cannot instantiate interface: Ns\BuiltinType" (a bare new of a doc template) names the template unquoted and its file was never implicated. Demo --check: EXIT 0 (124 files erased in 4 rounds).
## Tried and ruled out
Raising MAX_ERASURE_ROUNDS: each round is a full compile (~10 min user on the Demo), so more rounds only moves the wall. Debugging the last round: the original diagnostic hides it, use ELEPHC_DOCBLOCK_SHOW_LAST=1.
## Watch out
The multi-file repro needs the autoloader shape (class_files map): plain require_once files are not implicated by name. Test sets ELEPHC_DOCBLOCK_ERASURE_ROUND=7 so a single re-run is allowed. Still 4 rounds / 2561 s user on a cold Demo build vs 50 s pre-merge; the erased set is memoized for the next build.
