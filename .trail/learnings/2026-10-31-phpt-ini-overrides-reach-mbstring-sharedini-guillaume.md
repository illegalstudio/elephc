---
title: PHPT INI overrides reach mbstring/SharedIni startup
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - src/mbstring_startup.rs
  - src/pipeline/backend.rs
  - src/pipeline.rs
  - src/lib.rs
  - src/main.rs
  - tests/codegen/support/compiler.rs
  - tests/codegen/filter.rs
  - scripts/filter/port_phpt.py
---

Moved the mbstring/Core startup argument resolver out of the binary-only pipeline into src/mbstring_startup.rs, shared by the production backend and the codegen fixture harness. The harness now mirrors pipeline::backend: when the lowered module uses mbstring, mbregex or the eval bridge, it computes mbstring_startup from the fixture TEST_INI_OVERRIDES and enables the MIME provider for mbstring.http_output_conv_mimetypes. This makes ini_get() read the PHPT --INI-- values, so bug42718.phpt uses its original ini_get('filter.default_flags') call and still reports 4. Added a direct harness regression test for filter.default and filter.default_flags.
