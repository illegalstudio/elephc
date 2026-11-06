---
title: "chr()"
description: "Returns a one-character string from the given byte code point."
---

## chr()

```php
function chr(int $codepoint): string
```

Returns a one-character string from the given byte code point.

**Parameters**:
- `$codepoint` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `chr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/chr.md).
