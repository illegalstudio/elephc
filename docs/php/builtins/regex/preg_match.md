---
title: "preg_match()"
description: "Performs a regular expression match. `$flags` accepts `PREG_OFFSET_CAPTURE` and `PREG_UNMATCHED_AS_NULL`; `$offset` starts the search at a byte offset (negative counts from the end) while anchors and lookbehind still see the whole subject."
---

## preg_match()

```php
function preg_match(string $pattern, string $subject, array $matches = null, int $flags = 0, int $offset = 0): int|false
```

Performs a regular expression match. `$flags` accepts `PREG_OFFSET_CAPTURE` and `PREG_UNMATCHED_AS_NULL`; `$offset` starts the search at a byte offset (negative counts from the end) while anchors and lookbehind still see the whole subject.

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$matches` (`array`), passed by reference, default `null`, optional
- `$flags` (`int`), default `0`, optional
- `$offset` (`int`), default `0`, optional

**Returns**: `int|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `preg_match` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/preg_match.md).
