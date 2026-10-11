---
id: gotcha-regex-exceptions-must-retire-omitted-matches-def-d275b8f6
type: gotcha
title: "Regex exceptions must retire omitted matches defaults"
description: "Named preg_match_all omitted matches owns an ArrayNew that normal EIR cleanup cannot retire after ValueError"
tags: [windows, ownership, pcre]
created: 2026-10-11
verified_by: "cargo +1.98 test -p elephc --features curl --test codegen_tests preg_match_all_third_review -- --nocapture"
sources:
  - path: src/codegen/lower_inst/builtins/regex.rs
    blob: 871c28d2b853acca26f80743f97c69129baeba47
    lines: 54-159
    snip: f7f4969ed4f6
    anchor: "super::ensure_arg_count_between(inst, \"preg_match_all\", 2, 4)?;"
  - path: tests/codegen/preg_match_all_review.rs
    blob: 9f810c14a17bd08fd93e0e3accc9c72356654e9c
    lines: 13-43
    snip: 8afbb75ac9b1
    anchor: "fn preg_match_all_third_review_omitted_invalid_flags() {"
---

# Regex exceptions must retire omitted matches defaults

## Fact

The named-argument planner supplies an owned ArrayNew for omitted matches. An invalid low-byte flags guard throws before normal EIR cleanup, so release that operand on the invalid branch. Supplied local or reference-cell matches are retired through canonical writeback instead, and duplicate SSA operands must not be released twice. Valid calls retain ordinary EIR cleanup. Native PHP clears supplied matches before flag ValueError and ignores unknown high flag bits; invalid regex precedence and the fifth offset remain separate open parity gaps.
