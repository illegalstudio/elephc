---
title: "xml_set_object()"
description: "Binds an object whose methods are looked up for string handler names."
---

## xml_set_object()

```php
function xml_set_object(mixed $parser, mixed $object): bool
```

Binds an object whose methods are looked up for string handler names.

**Parameters**:
- `$parser` (`mixed`)
- `$object` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_object` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_object.md).
