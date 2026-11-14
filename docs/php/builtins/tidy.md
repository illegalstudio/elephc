---
title: "Tidy builtins"
description: "Builtins in the Tidy category."
sidebar:
  order: 123
---

## Tidy builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`tidy_access_count()`](./tidy/tidy_access_count.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_clean_repair()`](./tidy/tidy_clean_repair.md) | `(mixed $tidy): bool` | `bool` | ✓ | ✓ |
| [`tidy_config_count()`](./tidy/tidy_config_count.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_diagnose()`](./tidy/tidy_diagnose.md) | `(mixed $tidy): bool` | `bool` | ✓ | ✓ |
| [`tidy_error_count()`](./tidy/tidy_error_count.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_get_body()`](./tidy/tidy_get_body.md) | `(mixed $tidy): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_get_config()`](./tidy/tidy_get_config.md) | `(mixed $tidy): array` | `array` | ✓ | ✓ |
| [`tidy_get_error_buffer()`](./tidy/tidy_get_error_buffer.md) | `(mixed $tidy): string|false` | `string|false` | ✓ | ✓ |
| [`tidy_get_head()`](./tidy/tidy_get_head.md) | `(mixed $tidy): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_get_html()`](./tidy/tidy_get_html.md) | `(mixed $tidy): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_get_html_ver()`](./tidy/tidy_get_html_ver.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_get_opt_doc()`](./tidy/tidy_get_opt_doc.md) | `(mixed $tidy, string $option): string|false` | `string|false` | ✓ | ✓ |
| [`tidy_get_output()`](./tidy/tidy_get_output.md) | `(mixed $tidy): string` | `string` | ✓ | ✓ |
| [`tidy_get_release()`](./tidy/tidy_get_release.md) | `(): string` | `string` | ✓ | ✓ |
| [`tidy_get_root()`](./tidy/tidy_get_root.md) | `(mixed $tidy): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_get_status()`](./tidy/tidy_get_status.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
| [`tidy_getopt()`](./tidy/tidy_getopt.md) | `(mixed $tidy, string $option): string|int|bool` | `string|int|bool` | ✓ | ✓ |
| [`tidy_is_xhtml()`](./tidy/tidy_is_xhtml.md) | `(mixed $tidy): bool` | `bool` | ✓ | ✓ |
| [`tidy_is_xml()`](./tidy/tidy_is_xml.md) | `(mixed $tidy): bool` | `bool` | ✓ | ✓ |
| [`tidy_parse_file()`](./tidy/tidy_parse_file.md) | `(string $filename, array|string|null $config = null, ?string $encoding = null, bool $useIncludePath = false): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_parse_string()`](./tidy/tidy_parse_string.md) | `(string $string, array|string|null $config = null, ?string $encoding = null): mixed` | `mixed` | ✓ | ✓ |
| [`tidy_repair_file()`](./tidy/tidy_repair_file.md) | `(string $filename, array|string|null $config = null, ?string $encoding = null, bool $useIncludePath = false): string|false` | `string|false` | ✓ | ✓ |
| [`tidy_repair_string()`](./tidy/tidy_repair_string.md) | `(string $string, array|string|null $config = null, ?string $encoding = null): string|false` | `string|false` | ✓ | ✓ |
| [`tidy_warning_count()`](./tidy/tidy_warning_count.md) | `(mixed $tidy): int` | `int` | ✓ | ✓ |
