---
title: "libxml_disable_entity_loader()"
description: "Records whether the external entity loader is disabled and answers the previous flag."
---

## libxml_disable_entity_loader()

```php
function libxml_disable_entity_loader(bool $disable = true): bool
```

Records whether the external entity loader is disabled and answers the previous flag.

**Parameters**:
- `$disable` (`bool`), default `true`, optional

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `libxml_disable_entity_loader` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/libxml_disable_entity_loader.md).
