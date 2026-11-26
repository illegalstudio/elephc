---
title: "filter chv.13: boxed string offset reads in loops + 047/048 restoration"
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - src/codegen_support/runtime/objects/mixed_array_get.rs
  - scripts/filter/port_phpt.py
  - tests/codegen/strings/offset_warnings.rs
  - tests/codegen/strings/offset_writes.rs
  - docs/php/strings.md
---

The 047/048 helpers returned stale strings because a mixed parameter holding a string stored its bytes in a boxed Mixed cell, and __rt_mixed_array_get only dispatched array/hash/object/null tags. Tag 1 fell through to boxed null, so $s[$len] read as '' and the helpers rewrote only the last byte. Added a string branch to __rt_mixed_array_get on both targets: integer offsets, negative offsets from the end, PHP's Uninitialized string offset warning, empty-string results, and quiet boxed null for isset()/??. Mixed string parameters now support the full read-modify-write loop; 047.phpt and 048.phpt use their original sources/expectations. Non-integer string keys on a boxed string still return null; tracked in the new bead.
