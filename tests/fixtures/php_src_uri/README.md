# php-src ext/uri PHPT fixtures

These files are copied from the PHP 8.5 branch of
[php/php-src](https://github.com/php/php-src), directory `ext/uri/tests`, at the
commit used by Elephc's PHP 8.5.10 compatibility baseline.

They are the compliance oracle for Elephc's `Uri\Rfc3986\Uri` and
`Uri\WhatWg\Url` implementation. The `test_uri_php_src_phpt_bundle` integration
test reads each `--FILE--` section, compiles it with Elephc, runs it, and compares
the result with the `--EXPECT--` / `--EXPECTF--` section.

A few fixtures receive a small harness adaptation, documented at the mutation
site: `056.phpt` (closing-tag output and a string `die()`), `gh19892.phpt`
(initializes its by-reference output variable), and
`withPath_success_file_invalid_drive_letter.phpt` (initializes its by-reference
`$errors`, and lets `array_map()` see the runtime array).
`multiple_error_with_warnings.phpt` names the concrete exception type instead of
reading a property through an interface-typed catch variable. Fixtures
`100.phpt`–`102.phpt` use php-src's test-only `zend_test_uri_parser()`, which the
harness emulates on the public classes; `100.phpt`'s `"parse_url"` parser keeps its
PHP `parse_url()` raw/struct path and uses the RFC3986 getters only for normalization. The
`oom_*` fixtures run with their `--INI-- memory_limit` as the heap ceiling. All twelve pass
with php-src's fatal message: the URI prelude charges the path footprint the Rust bridge
retains into hidden PHP arrays, so those allocations count against the same limit as the
rest of the program.

The fixtures remain under the PHP License 3.01; see
<https://www.php.net/license/3_01.txt>.
