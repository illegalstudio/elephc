---
title: "substr_count()"
description: "Counts the number of non-overlapping substring occurrences."
sidebar:
  order: 542
---

## substr_count()

```php
function substr_count(string $haystack, string $needle, int $offset = 0, mixed $length = null): int
```

Counts the number of non-overlapping substring occurrences.

**Parameters**:
- `$haystack` (`string`)
- `$needle` (`string`)
- `$offset` (`int`), default `0`, optional
- `$length` (`mixed`), default `null`, optional

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `substr_count` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/substr_count.md).
