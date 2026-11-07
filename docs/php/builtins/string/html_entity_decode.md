---
title: "html_entity_decode()"
description: "Converts HTML entities in a string back into their corresponding characters."
---

## html_entity_decode()

```php
function html_entity_decode(string $string, int $flags = 11, ?string $encoding = null): string
```

Converts HTML entities in a string back into their corresponding characters.

**Parameters**:
- `$string` (`string`)
- `$flags` (`int`), default `11`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `html_entity_decode` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/html_entity_decode.md).
