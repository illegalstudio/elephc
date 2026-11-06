---
title: "xml_set_element_handler()"
description: "Sets the start and end element handlers."
---

## xml_set_element_handler()

```php
function xml_set_element_handler(mixed $parser, mixed $start_handler, mixed $end_handler): bool
```

Sets the start and end element handlers.

**Parameters**:
- `$parser` (`mixed`)
- `$start_handler` (`mixed`)
- `$end_handler` (`mixed`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_element_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_element_handler.md).
