---
title: "xml_set_processing_instruction_handler()"
description: "Sets the processing instruction handler."
sidebar:
  order: 930
---

## xml_set_processing_instruction_handler()

```php
function xml_set_processing_instruction_handler(mixed $parser, mixed $handler): bool
```

Sets the processing instruction handler.

**Parameters**:
- `$parser` (`mixed`)
- `$handler` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_processing_instruction_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_processing_instruction_handler.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_processing_instruction_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_processing_instruction_handler.md).
