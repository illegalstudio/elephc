---
title: "simplexml_import_dom()"
description: "Imports a DOM node as a SimpleXML element."
sidebar:
  order: 1011
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

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/simplexml_import_dom.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/simplexml_import_dom.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `simplexml_import_dom` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/simplexml_import_dom.md).
