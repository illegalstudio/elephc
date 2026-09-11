---
title: "xml_set_external_entity_ref_handler()"
description: "Sets the external entity reference handler."
sidebar:
  order: 988
---

## xml_set_external_entity_ref_handler()

```php
function xml_set_external_entity_ref_handler(mixed $parser, mixed $handler): bool
```

Sets the external entity reference handler.

**Parameters**:
- `$parser` (`mixed`)
- `$handler` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_external_entity_ref_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_external_entity_ref_handler.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_external_entity_ref_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_external_entity_ref_handler.md).
