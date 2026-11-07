---
title: "simplexml_import_dom()"
description: "Imports a DOM node as a SimpleXML element."
---

## simplexml_import_dom()

```php
function simplexml_import_dom(object $node, ?string $class_name = 'SimpleXMLElement'): ?SimpleXMLElement
```

Imports a DOM node as a SimpleXML element.

**Parameters**:
- `$node` (`object`)
- `$class_name` (`?string`), default `'SimpleXMLElement'`, optional

**Returns**: `?SimpleXMLElement`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `simplexml_import_dom` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/simplexml_import_dom.md).
