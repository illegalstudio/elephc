---
title: "xml_set_object()"
description: "Binds an object whose methods are looked up for string handler names."
sidebar:
  order: 930
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

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_object.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_object.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_object` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_object.md).
