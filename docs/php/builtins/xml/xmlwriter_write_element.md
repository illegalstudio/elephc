---
title: "xmlwriter_write_element()"
description: "Writes a complete element."
---

## xmlwriter_write_element()

```php
function xmlwriter_write_element(mixed $writer, string $name, ?string $content = null): bool
```

Writes a complete element.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)
- `$content` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_element` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_element.md).
