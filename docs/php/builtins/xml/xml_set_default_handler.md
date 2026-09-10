---
title: "xml_set_default_handler()"
description: "Sets the default handler that receives everything no other handler claims."
sidebar:
  order: 985
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_default_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_default_handler.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_default_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_default_handler.md).
