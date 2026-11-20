---
title: filter PHPT XFAIL modeling + parent bead closure
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - scripts/filter/port_phpt.py
  - tests/codegen/filter_phpt.rs
  - tests/fixtures/filter_phpt/cases.json
---

The port no longer rewrites EXPECT for php-src --XFAIL-- fixtures. cases.json now carries xfail/xfail_output: the original EXPECT stays verbatim, and the harness compares against the recorded actual php-src failure (bug42718: trailing \000; bug49184: current expected output; bug67167.02: bool(false)). The generator template owns the Case fields and expected-text selection, since port_phpt.py regenerates filter_phpt.rs. With 033/036/047/048 restored and every filter PHPT passing under this model, the parent elephc-chv bead is closed; general language gaps remain tracked as separate follow-ups.
