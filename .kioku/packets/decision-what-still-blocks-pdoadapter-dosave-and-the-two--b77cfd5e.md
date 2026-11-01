---
id: decision-what-still-blocks-pdoadapter-dosave-and-the-two--b77cfd5e
type: decision
title: "What still blocks PdoAdapter doSave, and the two ways to close it"
description: "PDO is absent from the build so the whole chain is Mixed and no class declares bindParam; the callable resolver then invents a one-parameter non-ref signature for a two-argument call"
created: 2026-09-17
sources:
  - path: src/types/checker/inference/expr/effects.rs
    blob: 178eae9cb1a3da1a97854b77616380368ad0c8d8
---

# What still blocks PdoAdapter doSave, and the two ways to close it

## Fact

What still blocks `PdoAdapter::doSave`, traced end to end, and the decision it needs.

Three of the four links are fixed. The chain, measured in the real `--web` build:

1. `PDO` is NOT in that build's closed world. `PdoAdapter::getConnection(): \\PDO` therefore degrades
   to gradual.
2. So `$conn` is `Mixed`, and `$conn->prepare($sql)` is a Mixed-receiver call whose result is
   `Mixed` — `$stmt` never gets the `PDOStatement|false` the isolated reproducer shows.
3. `instance_call_effect_signature` now answers for a `Mixed` receiver by asking the classes that
   declare the method (`mixed_receiver_by_ref_signature`). In this build NO class declares
   `bindParam`, because `PDOStatement` is absent too — so it correctly answers `None`.
4. The lookup then falls through to `resolve_first_class_callable_sig`, which returns
   `ref_params=[false]` — a ONE-parameter non-ref signature for a two-argument call. Nothing
   declares that; it is invented. `$id` is not bound, and the later read is refused.

    [byref-site] recv=Mixed sig=Some([false])

Link 4 is the same fabrication this codebase keeps producing for an absent dependency, and it is the
one still open. Two ways to close it, and the choice is a judgement call rather than a bug fix:

- **Synthesize instead of guess.** For a gradual receiver with no candidate, treat a bare-variable
  argument as possibly bound by reference and DEFINE it. PHP cannot know either; refusing is a false
  positive on correct code. The cost is losing `Undefined variable` for a genuine typo passed to a
  method nothing in the program declares.
- **Stop the resolver from inventing.** Find why `resolve_first_class_callable_sig` answers a
  one-parameter signature for a two-argument call on an unknown receiver, and make it answer `None`.
  That keeps the diagnostic and fixes the false positive at its source, but it is a change inside
  the callable-tracking machinery, whose other users were not surveyed.

The second is the better fix if the resolver is simply wrong here. I did not take either: the first
trades away a real diagnostic and the second needs a survey of callable tracking, and both deserve a
deliberate call rather than a late one.
