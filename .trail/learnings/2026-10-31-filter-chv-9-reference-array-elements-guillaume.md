---
title: "filter chv.9: reference array elements, var_dump &, filter_var_array write-through"
author: Guillaume
date: 2026-10-31
branch: feat/filter-php-src-compliance
touches:
  - src/parser/expr/prefix.rs
  - src/parser/stmt/assign/postfix.rs
  - src/parser/ast/expr.rs
  - src/optimize/propagate/expr.rs
  - src/optimize/propagate/invalidation.rs
  - src/ir_lower/expr/assoc_array_literals.rs
  - src/ir_lower/expr/indexed_array_literals.rs
  - src/types/checker/inference/syntactic.rs
  - src/codegen_support/runtime/io/var_dump_walk.rs
  - src/codegen_support/runtime/data/fixed.rs
  - src/filter_prelude/input.rs
  - scripts/filter/port_phpt.py
  - tests/codegen/arrays/by_ref_places.rs
  - tests/codegen/filter.rs
  - tests/parser_tests/expressions/arrays_match.rs
  - tests/error_tests/reference_arrays.rs
  - docs/php/arrays.md
  - docs/php/types.md
---

Implemented PHP reference elements in array literals end to end. The parser accepts [&\$x], ['k' => &\$x] and array(&\$x) (variables only for now) and \$data['k'] = &\$v for complex array targets. Literals containing references are forced to hash storage; lowering stores the value, promotes the entry with LoadArrayElemRefCell, then binds the source variable to that cell. Constant propagation must not recurse into a Reference wrapper, and reference exposure marks the local volatile so no constant fact survives. var_dump now reads the raw hash iterator: a tag-11 entry whose cell owner count exceeds one sets a pending marker that __rt_vd_pad prints as '&' after the indentation. filter_var_array's scalar-filter path now copies the input array and filters each entry in place (instead of building a fresh array), so reference entries write through exactly like php-src's duplicate-then-filter behaviour. 036.phpt and filter_var_array_with_ref.phpt are restored to original source and expectations. The codegen harness mbstring INI expectation moved from 4/8 to 6/12 Core INI leaves because it now mirrors production startup configuration (three default encoding directives). Remaining: non-variable reference elements and bare references after spreads are rejected (elephc-x3f).
