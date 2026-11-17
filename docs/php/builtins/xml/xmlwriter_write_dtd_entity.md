---
title: "xmlwriter_write_dtd_entity()"
description: "Writes a complete DTD entity declaration."
---

## xmlwriter_write_dtd_entity()

```php
function xmlwriter_write_dtd_entity(mixed $writer, string $name, string $content, bool $isParam = false, ?string $publicId = null, ?string $systemId = null, ?string $notationData = null): bool
```

Writes a complete DTD entity declaration.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)
- `$content` (`string`)
- `$isParam` (`bool`), default `false`, optional
- `$publicId` (`?string`), default `null`, optional
- `$systemId` (`?string`), default `null`, optional
- `$notationData` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_dtd_entity` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_dtd_entity.md).
