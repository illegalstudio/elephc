# Mbstring: retained probes and remaining qualification

- [x] Ship all 65 PHP baseline function bindings on both AOT and eval, with the shared engine and nine module constants.
- [x] Ship managed Oniguruma mbregex, encoding/Unicode catalogs, MIME, mail, references, INI and host adapters.
- [x] Replace the historical implementation log with current source pointers and a retained-probe inventory.
- [ ] Map every retained probe to an exact permanent regression or a current open issue.
- [ ] Requalify historical ownership, strictness and host-state observations before treating them as current defects.
- [ ] Preserve source-order coercion, alias visibility, callback ownership and error cleanup in accepted fixes.
- [ ] Complete target/host qualification for each remaining issue without inferring runtime correctness from binding counts.

## Current baseline

Reviewed against release candidate `df57a9800f` on 2026-10-19. The shared registry
contains 65 public mbstring functions, all with native and eval support,
including `mb_convert_variables`, `mb_ereg_replace_callback` and `mb_send_mail`.
The original 63/65 checkpoint and earlier statements that those functions are
missing are obsolete. Catalog coverage does not establish every edge case's
PHP behavior or prove that a retained ownership probe is solved.

| Surface | Source / permanent reference |
|---|---|
| Engine, codecs, Unicode, state and mbregex | `crates/elephc-mbstring/` |
| Pinned PHP function/encoding evidence | `scripts/mbstring/php_surface.json`, `scripts/mbstring/` |
| AOT contracts and semantic bindings | `crates/elephc-builtin-contract/`, `src/builtins/` |
| Native runtime/callable/reference adapters | `src/codegen_support/runtime/strings/mbstring/`, `src/codegen/lower_inst/` |
| Eval bindings and operand ownership | `crates/elephc-magician/src/interpreter/builtins/`, `crates/elephc-magician/src/interpreter/` |
| Generated signatures and support routes | `scripts/docs/builtin_registry.json`, `docs/php/builtins/string.md`, `docs/php/builtins/string/` |
| User and runtime contracts | [strings](../docs/php/strings.md), [runtime](../docs/internals/the-runtime.md), [eval runtime](../docs/internals/eval-runtime.md) |

The immutable encoding catalog is already shared. The retained log is not a
reason to reintroduce duplicate codecs, manual signature tables, native library
fallbacks or always-on bridge linking. Mbregex uses the project's managed
Oniguruma package; PCRE2 selection is a separate regex capability.

## Probe disposition

The complete inventory is [mbstring-probes/README.md](mbstring-probes/README.md).
Keep all 12 sources until their exact assertions and observable behavior have
been mapped; this cleanup did not execute them.

| Family | Existing related evidence | Remaining decision |
|---|---|---|
| Conversion input and detect-order references | `tests/codegen/strings/mbstring_conversion.rs`, `tests/codegen/strings/mbstring_detect_order.rs` contain the named historical classes | Compare complete sources, output, mutations and ownership assertions before dropping duplicate reproducers. |
| Binary escape bytes | `crates/elephc-magician/src/lexer/tests.rs::string_literal_bytes_preserve_raw_escapes_and_private_use_text` | Check the full opaque-eval path; the old claim that the current lexer always corrupts the byte is not current evidence. |
| Query ternary owners | `tests/codegen/runtime_gc/mbstring_capture_hash/query_register/input_ternary.php` | The retained source also has throwing destructors; related coverage is not an exact replacement. |
| Dynamic callable strict MIME | `dynamic-callable-strict-mime.php` | Map physical-source strictness and runtime callable selection to a permanent acceptance/rejection fixture. |
| Persistent nested references and parameter destruction | `persistent-reference-nested-slot-return.php`, `scope-destructor-parameter.php` | Reconcile with current eval reference/argument ownership and retained Core tasks. |
| Eval callback traces and source append | `test_mbstring_eval_callback_trace_ownership.php`, `test_mbstring_eval_append_compound_source_ownership.php` | Preserve INI-sensitive destructor order and source-building ownership when choosing permanent coverage. |
| Startup eval array and loop owners | `test_mbstring_startup_eval_array_ownership.php`, `test_mbstring_startup_eval_loop_ownership.php` | Distinguish startup state from general interpreter temporary ownership; do not fix the module adapter merely because it exposed the path. |

General reference/callable limits are coordinated in
[Core ownership](core-final-review-fixes.md); frontend/AOT boundaries are in
[the eval plan](elephc-eval-magician-plan.md). Code Journal tasks for eval
reference keys (`4136c2d0`), closure captures (`90e1d964`), static/constant owners
(`517895d9`) and scalar expression ownership should be refreshed rather than
recreated as unchecked copies of the old log.

## Acceptance for future corrections

Compare full output and mutation order, pending exceptions, physical-source
strictness, named/spread evaluation, live references, MIME/codec error shapes,
output-handler state, and heap ownership. Validate both optimizer settings and
preserve all supported target paths or their explicit capability diagnostics.
Host-response, startup INI and mail qualification need their actual host
fixtures; a symbol or unit test alone is insufficient.

No historical failure is relabeled as a current reproduction or a successful
replay. Earlier logs, commands and transient paths remain recoverable from
`54c5c3332e`; reproducible source cases remain in this repository. No tests,
PHP probes, native package installation or benchmarks ran during consolidation.
