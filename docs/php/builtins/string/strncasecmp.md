---
title: "strncasecmp()"
description: "Compares the first n bytes of two strings, ignoring ASCII case."
sidebar:
  order: 531
---

## strncasecmp()

```php
function strncasecmp(string $string1, string $string2, int $length): int
```

Compares the first n bytes of two strings, ignoring ASCII case.

**Parameters**:
- `$string1` (`string`)
- `$string2` (`string`)
- `$length` (`int`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: not available inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `strncasecmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strncasecmp.md).
