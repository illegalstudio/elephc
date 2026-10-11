---
title: "preg_match_all()"
description: "Performs a global regular expression match, including terminal empty matches, optionally filling `$matches`, and returns the number of matches. AOT capture output supports local variables and local references; unsupported non-local destinations are diagnosed. With a valid pattern, invalid low-byte order flags raise `ValueError` after clearing a supplied capture destination on both backends. Unknown high flag bits are ignored, matching PHP. In set order, trailing unmatched groups are omitted unless `PREG_UNMATCHED_AS_NULL` is enabled. PHP's fifth offset argument and complete invalid-pattern/false-result behavior remain unsupported."
sidebar:
  order: 776
---

## preg_match_all()

```php
function preg_match_all(string $pattern, string $subject, mixed &$matches = [], int $flags = 0): int
```

Performs a global regular expression match, including terminal empty matches, optionally filling `$matches`, and returns the number of matches. AOT capture output supports local variables and local references; unsupported non-local destinations are diagnosed. With a valid pattern, invalid low-byte order flags raise `ValueError` after clearing a supplied capture destination on both backends. Unknown high flag bits are ignored, matching PHP. In set order, trailing unmatched groups are omitted unless `PREG_UNMATCHED_AS_NULL` is enabled. PHP's fifth offset argument and complete invalid-pattern/false-result behavior remain unsupported.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$matches` (`mixed`), passed by reference, default `[]`, optional
- `$flags` (`int`), default `0`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/preg_match_all.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/preg_match_all.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_match_all` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_match_all.md).
