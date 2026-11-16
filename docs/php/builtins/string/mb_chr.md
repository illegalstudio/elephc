---
title: "mb_chr()"
description: "Encodes a Unicode codepoint, or returns false when it cannot be represented."
---

## mb_chr()

```php
function mb_chr(int $codepoint, ?string $encoding = null): string|false
```

Encodes a Unicode codepoint, or returns false when it cannot be represented.

**Parameters**:
- `$codepoint` (`int`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_chr` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_chr.md).
