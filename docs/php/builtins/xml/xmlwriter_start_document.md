---
title: "xmlwriter_start_document()"
description: "Writes the XML declaration."
---

## xmlwriter_start_document()

```php
function xmlwriter_start_document(mixed $writer, ?string $version = '1.0', ?string $encoding = null, ?string $standalone = null): bool
```

Writes the XML declaration.

**Parameters**:
- `$writer` (`mixed`)
- `$version` (`?string`), default `'1.0'`, optional
- `$encoding` (`?string`), default `null`, optional
- `$standalone` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_document` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_document.md).
