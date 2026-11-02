# Retained mbstring and eval reproducers

These 12 PHP files are source-only historical reproducers, outside the normal
test harness. Their comments record earlier observations, not a claim that the
current compiler fails them. No probe was executed during the 2026-10-19 cleanup.
The remaining module work is tracked in [mbstring-extension.md](../mbstring-extension.md).

## Inventory and related coverage

| Retained source | Related permanent source / classification |
|---|---|
| `conversion-input-reference.php` | `tests/codegen/strings/mbstring_conversion.rs` contains `ConversionReference`. Exact assertion mapping still required. |
| `detect-order-global-reference.php` | `tests/codegen/strings/mbstring_detect_order.rs` contains `DetectionReference`. Exact assertion mapping still required. |
| `detect-order-eval-global-array.php` | The same detect-order suite contains `DetectionListCow`. Preserve its eval/COW mutation checks. |
| `eval-binary-escapes.php` | Magician's `lexer/tests.rs` has a raw-escape byte regression. Full opaque-eval coverage must be checked before retirement. |
| `dynamic-callable-strict-mime.php` | Historical runtime-callable and physical-source strictness probe; no exact replacement established here. |
| `persistent-reference-nested-slot-return.php` | Historical reference-result lifetime probe; coordinate with the Core/eval plans. |
| `scope-destructor-parameter.php` | Historical argument/destructor-order probe; no current failure is asserted. |
| `test_mbstring_eval_callback_trace_ownership.php` | Historical Throwable argument-retention/destructor order under both `zend.exception_ignore_args` settings. |
| `test_mbstring_eval_append_compound_source_ownership.php` | Historical ownership of eval source built with `.=`; not equivalent to array-append behavior alone. |
| `test_mbstring_query_ternary_value_ownership.php` | Related fixture: `tests/codegen/runtime_gc/mbstring_capture_hash/query_register/input_ternary.php`; retained throwing-destructor variants need separate mapping. |
| `test_mbstring_startup_eval_array_ownership.php` | Historical eval-array owner report; separate startup state from generic eval cleanup. |
| `test_mbstring_startup_eval_loop_ownership.php` | Historical plain eval-loop owner report, independent of mbstring startup configuration. |

## Retirement rule

Map each exact source to a named permanent test, including expected output,
mutations, exception/destructor order and any heap assertions. A related class
name or lexer unit test alone does not establish equivalent coverage. Resolve
current behavior through the normal focused workflow when execution is
requested, then remove only sources whose evidence is preserved.

Keep generated assembly, binaries and logs outside this directory. Permanent
native regressions belong under `tests/codegen/`; interpreter/lexer regressions
belong in the corresponding Magician suite. The original historical context
is available from Git commit `54c5c3332e`.
