---
title: Shared unserialize diagnostic joins must survive Mach-O dead stripping
author: Guillaumeloulier
date: 2026-10-15
cost: { minutes: 0, tokens: 0 }
touches:
  - src/codegen_support/runtime/system/unserialize/diagnostics.rs
  - docs/internals/the-runtime.md
---

## Decided
When separate global runtime helpers jump into a common diagnostic body, emit the common join with label_shared on both architectures. For unserialize, the option-error entry closes its pre-parser context while the property-error entry leaves closure to the protected decoder. They share their message-building body without sharing a prologue or changing their caller ABI.

Under Mach-O dead stripping, a plain label becomes an L-local owned by the last global helper. The earlier helper's cross-atom branch cannot keep that target atom alive. A shared alternate entry remains a relocatable symbol and preserves the intended common body.

## Tried and ruled out
An assemble-only check is insufficient for cross-atom liveness: the old runtime assembled but failed the existing atom-ownership test for __rt_unser_type_error_ready. Validate the existing atom parser as well as real assembly. No timeout or CI filter adjustment fixes this symbol ownership defect.

## Watch out
Run functional typed-property/options errors and hydration exception cleanup as separate checks from symbol/assembly checks. Keep iOS/Linux assembly evidence distinct from execution. Focused external harnesses must match dependency package versions from Cargo metadata; after a feature-on docs build, selecting the newest sha2 rlib by name alone can choose the wrong major version. Costs were unmeasured; zero disables historical attribution.
