---
title: "xml_set_unparsed_entity_decl_handler()"
description: "Sets the unparsed (NDATA) entity declaration handler."
sidebar:
  order: 993
---

## xml_set_unparsed_entity_decl_handler()

```php
function xml_set_unparsed_entity_decl_handler(mixed $parser, mixed $handler): bool
```

Sets the unparsed (NDATA) entity declaration handler.

**Parameters**:
- `$parser` (`mixed`)
- `$handler` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_unparsed_entity_decl_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_unparsed_entity_decl_handler.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_unparsed_entity_decl_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_unparsed_entity_decl_handler.md).
