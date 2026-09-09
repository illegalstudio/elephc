---
title: "xml_set_notation_decl_handler()"
description: "Sets the notation declaration handler."
sidebar:
  order: 953
---

## xml_set_notation_decl_handler()

```php
function xml_set_notation_decl_handler(mixed $parser, mixed $handler): bool
```

Sets the notation declaration handler.

**Parameters**:
- `$parser` (`mixed`)
- `$handler` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_notation_decl_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_notation_decl_handler.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_notation_decl_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_notation_decl_handler.md).
