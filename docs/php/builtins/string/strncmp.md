---
title: "strncmp()"
description: "Compares the first n bytes of two strings."
sidebar:
  order: 471
---

## strncmp()

```php
function strncmp(string $string1, string $string2, int $length): int
```

Compares the first n bytes of two strings.

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

For how `strncmp` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/strncmp.md).
