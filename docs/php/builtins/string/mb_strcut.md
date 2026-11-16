---
title: "mb_strcut()"
description: "Selects complete encoded characters within a byte offset and length."
---

## mb_strcut()

```php
function mb_strcut(string $string, int $start, ?int $length = null, ?string $encoding = null): string
```

Selects complete encoded characters within a byte offset and length.

**Parameters**:
- `$string` (`string`)
- `$start` (`int`)
- `$length` (`?int`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strcut` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strcut.md).
