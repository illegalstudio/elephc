---
title: "xmlwriter_start_dtd()"
description: "Starts a DTD."
---

## xmlwriter_start_dtd()

```php
function xmlwriter_start_dtd(mixed $writer, string $qualifiedName, ?string $publicId = null, ?string $systemId = null): bool
```

Starts a DTD.

**Parameters**:
- `$writer` (`mixed`)
- `$qualifiedName` (`string`)
- `$publicId` (`?string`), default `null`, optional
- `$systemId` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_dtd` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_dtd.md).
