---
title: "xml_set_default_handler()"
description: "Sets the default handler that receives everything no other handler claims."
---

## xml_set_default_handler()

```php
function xml_set_default_handler(mixed $parser, mixed $handler): bool
```

Sets the default handler that receives everything no other handler claims.

**Parameters**:
- `$parser` (`mixed`)
- `$handler` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_default_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_default_handler.md).
