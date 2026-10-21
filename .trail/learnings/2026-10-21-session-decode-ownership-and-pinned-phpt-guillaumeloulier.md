---
title: Session decode ownership and pinned PHPT audit gates
author: Guillaumeloulier
date: 2026-10-21
branch: feat/session-php-src-compliance
touches:
  - ".artifacts/.gitignore"
  - ".plans/session-php-src-compliance.md"
  - crates/elephc-builtin-contract/src/catalog_data.rs
  - crates/elephc-builtin-contract/src/support.rs
  - crates/elephc-web/src/lib.rs
  - crates/elephc-web/src/session/file_io.rs
  - crates/elephc-web/src/session/mod.rs
  - crates/elephc-web/src/session/state.rs
  - crates/elephc-web/src/session/wire_format.rs
  - crates/elephc-web/src/session/wire_format/decode_array.rs
---

## Decided

Audit session behavior against php-src php-8.5.11 at 678778973bb4d4185c06ef1320de50f1a975d3c8, keeping all 263 original PHPTs and each adapter/compiler blocker visible. Test-only upload lifetime protection is published independently as PR #1965. Session compliance is still open.

For session decode, frame the php/php_binary buffer as one serialized array and shift reference IDs past the synthetic root. Use boxed Mixed storage for _SESSION consistently across checking, EIR, globals, eval synchronization, and request cleanup: php_serialize accepts scalar roots, while N; becomes an empty array. Reread the held session descriptor for session_reset. Keep generated-ID validation fail-closed; explicit public empty-ID behavior uses a separate clear operation. Engine session diagnostics pass through handler dispatch at engine levels.

Uppercase-R references to completed arrays can promote the published box in place with tag7/high1 and transfer the old payload owner into its child. Ordinary hash assignments update that shared child; constructor bucket replacement detaches the old entry before its release. A transferred ordinary Mixed input stays installed as the same cell. Value copies from another persistent reference detach its logical value. Native ARM64/x86_64 repeated mutation, whole-array replacement, scalar replacement and ordinary COW probes match PHP and balance heap owners.

## Tried and ruled out

Cloning and releasing the incoming ordinary Mixed box inside reference assignment invalidated the pointer still borrowed by nested write-context fetches. Using nested_call_reg for helper scratch clobbered callee-saved x19/r12. Use caller-saved symbol scratch or explicitly preserve callee-saved registers.

## Watch out

Source framing alone is not full graph compliance: scalar/string references, unfinished arrays, lowercase object numbering, nonempty-session merging and shared encoding remain separate gates. Do not equate clean heaps or assembly checks with complete PHP/native-target parity.

PHPT fatal expectations may legitimately exit 255. Preserve quoted INI values and keep the private temporary directory outside the test cwd, so open_basedir=. retains upstream behavior. Missing CGI/request adapters are explicit adapter errors. Freeze compiler/archives or verify end hashes: cargo test can replace the top-level CLI while a replay runs.

Trail check produced lexical overlaps in the plan and generated documentation, including old-session telemetry, source-map and prior ownership narratives. These do not describe a contradictory implementation here. Preserve exact-artifact evidence and the open gates rather than deleting context to satisfy token matching.
