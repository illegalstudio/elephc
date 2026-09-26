# Regenerating mbstring data

This directory captures PHP 8.5.10 results and generates the tables and fixtures
used by `elephc-mbstring`. Run the commands from the repository root with PHP
8.5.10 and its mbstring extension. The regex captures that check the engine
version require Oniguruma 6.9.10. Build and runtime use the checked-in results;
they do not execute these scripts or invoke PHP.

`scripts/mbstring/php_surface.json` records the public functions, constants,
encoding order, aliases, and MIME names. The catalog and ABI tests load this exact
file with `include_str!`; `generate_encodings.py` and
`generate_output_fixtures.py` also read it. Capture it before generating tables
or output-handler fixtures.

## Capture and generate

Run these commands in order. The output comments give repository-relative paths.
Scripts without shell redirection write their own output files. PHP workers
called by the Python captures (`capture_ini_reentry.php`, `capture_regex.php`,
`capture_regex_request.php`, and `capture_regex_output.php`) are not separate steps.

```bash
php scripts/mbstring/capture_surface.php > scripts/mbstring/php_surface.json
php -d error_reporting=0 scripts/mbstring/capture_encoding_lookup.php > crates/elephc-mbstring/tests/fixtures/encoding_lookup.json

# -> crates/elephc-mbstring/src/encoding/data/{singlebyte.json,*.bin}
php -d memory_limit=256M scripts/mbstring/capture_singlebyte.php
# -> crates/elephc-mbstring/src/encoding/data/{doublebyte.json,*.bin}
php -d memory_limit=256M scripts/mbstring/capture_doublebyte.php
# -> crates/elephc-mbstring/src/encoding/data/utf8-mobile-*.bin, mobile_utf8.json
# -> crates/elephc-mbstring/tests/fixtures/mobile_utf8.json
php scripts/mbstring/capture_mobile_utf8.php
# -> crates/elephc-mbstring/src/encoding/data/gb18030*.bin, gb18030.json
# -> crates/elephc-mbstring/tests/fixtures/gb18030.json
php scripts/mbstring/capture_gb18030.php
# -> crates/elephc-mbstring/src/encoding/data/euc-tw-*.bin, euctw.json
# -> crates/elephc-mbstring/tests/fixtures/euctw.json
php scripts/mbstring/capture_euctw.php
# -> crates/elephc-mbstring/src/encoding/data/{jis.json,cp5022x-plane.bin,*-encode.bin,jis-kddi-*.bin}
# -> crates/elephc-mbstring/tests/fixtures/{jis.jsonl.gz,jis-hashes.json}
php scripts/mbstring/capture_jis.php
# -> crates/elephc-mbstring/src/encoding/data/{boundaries.json,*-boundaries.bin}
php scripts/mbstring/capture_boundaries.php
# -> crates/elephc-mbstring/src/encoding/catalog_data.rs
python3 scripts/mbstring/generate_encodings.py
# -> crates/elephc-mbstring/src/encoding/transfer/html/{data.rs,manifest.json}
python3 scripts/mbstring/capture_html.py

# -> crates/elephc-mbstring/tests/fixtures/languages.json
php scripts/mbstring/capture_languages.php
# -> crates/elephc-mbstring/src/state/language_data.rs
python3 scripts/mbstring/generate_languages.py
php scripts/mbstring/capture_unicode.php > crates/elephc-mbstring/tests/fixtures/unicode.json
php scripts/mbstring/capture_codecs.php > crates/elephc-mbstring/tests/fixtures/codecs.json
php scripts/mbstring/capture_text.php > crates/elephc-mbstring/tests/fixtures/text.json

# -> crates/elephc-mbstring/tests/fixtures/{operations.jsonl.gz,operations-excluded.json}
php scripts/mbstring/capture_operations.php
# -> crates/elephc-mbstring/src/unicode/data/kana*.bin and kana.json
# -> crates/elephc-mbstring/tests/fixtures/{kana.jsonl.gz,kana-hashes.json}
php scripts/mbstring/capture_kana.php
# -> crates/elephc-mbstring/tests/fixtures/batches.jsonl.gz
php scripts/mbstring/capture_batches.php
# -> crates/elephc-mbstring/tests/fixtures/transfer.jsonl.gz
php scripts/mbstring/capture_transfer.php
# -> crates/elephc-mbstring/tests/fixtures/entities.jsonl.gz
php scripts/mbstring/capture_entities.php
# -> crates/elephc-mbstring/tests/fixtures/mime_decode.jsonl.gz
php scripts/mbstring/capture_mime_decode.php
# -> crates/elephc-mbstring/tests/fixtures/mime_encode.jsonl.gz
php scripts/mbstring/capture_mime_encode.php
# -> crates/elephc-mbstring/tests/fixtures/info.jsonl.gz
php scripts/mbstring/capture_info.php
# -> crates/elephc-mbstring/tests/fixtures/http_input.jsonl.gz
php scripts/mbstring/capture_http_input.php
# -> crates/elephc-mbstring/tests/fixtures/detect_many.jsonl.gz
php scripts/mbstring/capture_detect_many.php
# -> crates/elephc-mbstring/tests/fixtures/parse_str.jsonl.gz
php scripts/mbstring/capture_parse_str.php
# -> crates/elephc-mbstring/tests/fixtures/{parse_str_reentry.jsonl.gz,parse_str_reentry_excluded.json}
php scripts/mbstring/capture_parse_str_reentry.php
php -d max_input_nesting_level=1 -d log_errors=0 scripts/mbstring/capture_parse_str_display.php > crates/elephc-mbstring/tests/fixtures/parse_str_display.json
# -> crates/elephc-mbstring/tests/fixtures/ini.jsonl.gz
php scripts/mbstring/capture_ini.php
# -> crates/elephc-mbstring/tests/fixtures/ini_startup.jsonl.gz
python3 scripts/mbstring/capture_ini_startup.py
# -> crates/elephc-mbstring/tests/fixtures/ini_reentry.jsonl.gz
python3 scripts/mbstring/capture_ini_reentry.py

# -> crates/elephc-mbstring/tests/fixtures/regex.jsonl.gz
python3 scripts/mbstring/capture_regex.py
# -> crates/elephc-mbstring/tests/fixtures/regex_request.jsonl.gz
python3 scripts/mbstring/capture_regex_request.py
# -> crates/elephc-mbstring/tests/fixtures/regex_split.jsonl.gz
python3 scripts/mbstring/capture_regex_split.py
# -> crates/elephc-mbstring/tests/fixtures/regex_replace.jsonl.gz
python3 scripts/mbstring/capture_regex_replace.py
# -> tests/codegen/strings/fixtures/{mbstring_replace_public.out,mbstring_replace_errors.out,mbstring_replace_errors.err}
python3 scripts/mbstring/capture_regex_replace_public.py
# -> crates/elephc-mbstring/tests/fixtures/regex_capture.jsonl.gz
python3 scripts/mbstring/capture_regex_capture.py
php scripts/mbstring/capture_regex_retarget.php > crates/elephc-mbstring/tests/fixtures/regex_retarget.json
# -> crates/elephc-mbstring/tests/fixtures/regex_output.jsonl.gz
python3 scripts/mbstring/capture_regex_output.py
# -> crates/elephc-mbstring/tests/fixtures/regex_worker.json
python3 scripts/mbstring/capture_regex_worker.py

# -> crates/elephc-mbstring/tests/fixtures/entity_maps.json
php scripts/mbstring/capture_entity_maps.php
# -> crates/elephc-mbstring/tests/fixtures/state.jsonl.gz
php scripts/mbstring/capture_state.php
# -> crates/elephc-mbstring/tests/fixtures/detect.jsonl.gz
php scripts/mbstring/capture_detect.php
# -> crates/elephc-mbstring/tests/fixtures/detected-conversion.jsonl.gz
php scripts/mbstring/capture_detected_conversion.php
# -> crates/elephc-mbstring/tests/fixtures/conversion-errors.jsonl.gz
php scripts/mbstring/capture_conversion_errors.php
# -> crates/elephc-mbstring/tests/fixtures/arrays.jsonl.gz
php scripts/mbstring/capture_arrays.php
# -> crates/elephc-mbstring/tests/fixtures/split-batches.jsonl.gz
php scripts/mbstring/capture_split_batches.php
# -> crates/elephc-mbstring/tests/fixtures/{coercions.jsonl.gz,coercion_arity.json}
php scripts/mbstring/capture_coercions.php
# -> crates/elephc-mbstring/tests/fixtures/coercion_order.json
php scripts/mbstring/capture_coercion_order.php
# -> crates/elephc-mbstring/tests/fixtures/{utf7.jsonl.gz,utf7-encode.json}
php scripts/mbstring/capture_utf7.php
# -> crates/elephc-mbstring/tests/fixtures/{hz.jsonl.gz,hz-encode.json}
php scripts/mbstring/capture_hz.php
# -> crates/elephc-mbstring/tests/fixtures/{iso2022kr.jsonl.gz,iso2022kr-encode.json}
php scripts/mbstring/capture_iso2022kr.php

# -> crates/elephc-mbstring/tests/fixtures/output_handler.jsonl.gz
python3 scripts/mbstring/generate_output_fixtures.py
```

The replacement-callback capture is a JSON-lines filter. Its inputs are already
stored in `regex_replace_callback.jsonl`. To regenerate that fixture, use `jq`
and hold the new rows in memory so the source is not truncated while it is read:

```bash
set -euo pipefail
callback_rows=$(jq -c '.input' crates/elephc-mbstring/tests/fixtures/regex_replace_callback.jsonl | php scripts/mbstring/capture_regex_replace_callback.php)
printf '%s\n' "$callback_rows" > crates/elephc-mbstring/tests/fixtures/regex_replace_callback.jsonl
```

For detection weights, supply PHP 8.5.10's
[`common_codepoints.txt`](https://raw.githubusercontent.com/php/php-src/php-8.5.10/ext/mbstring/common_codepoints.txt).
The generator checks its SHA-256 and writes
`crates/elephc-mbstring/src/detect/data/common.bin` and `manifest.json`:

```bash
python3 scripts/mbstring/generate_detection.py /path/to/common_codepoints.txt
```

For Unicode 17.0.0, supply `UnicodeData.txt`, `SpecialCasing.txt`,
`CaseFolding.txt`, `DerivedCoreProperties.txt`, and `EastAsianWidth.txt` from
the [Unicode 17.0.0 UCD](https://www.unicode.org/Public/17.0.0/ucd/).
`generate_unicode.py` verifies their pinned SHA-256 values and writes binary
tables and `manifest.json` to `crates/elephc-mbstring/src/unicode/data/`.
Keep the data licenses in distributions containing these tables.

```bash
python3 scripts/mbstring/generate_unicode.py /path/to/ucd
```

Verify the regenerated crate data and fixtures:

```bash
cargo test -p elephc-mbstring
```

## Capture exclusions and compatibility limits

- `capture_operations.php` records PHP cases that terminate its oracle worker in
  `operations-excluded.json`. `capture_parse_str_reentry.php` also retains two
  exclusions in `parse_str_reentry_excluded.json`; on PHP 8.5.10 both workers
  exhaust memory while serializing the final trace. Neither ledger is a
  successful compatibility comparison.
- `mb_send_mail` passes the configured transport and extra parameters as direct
  process arguments. Shell quoting and metacharacters in those parameters do not
  have PHP `php_mail` semantics (`crates/elephc-mbstring/src/abi/mail.rs`).
- The AOT `mb_ereg` output adapter rejects by-reference parameters and aliases
  redirected to object properties when it cannot track a managed `Mixed` local
  (`tests/codegen/strings/mbstring_regex_capture.rs`).
