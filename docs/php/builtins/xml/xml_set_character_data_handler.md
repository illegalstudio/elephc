---
title: "xml_set_character_data_handler()"
description: "Sets the character data handler."
sidebar:
  order: 948
---

## xml_set_character_data_handler()

```php
function xml_set_character_data_handler(mixed $parser, mixed $handler): bool
```

Sets the character data handler.

**Parameters**:
- `$parser` (`mixed`)
- `$handler` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_character_data_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_character_data_handler.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_character_data_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_character_data_handler.md).
