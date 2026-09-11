---
title: "xml_set_element_handler()"
description: "Sets the start and end element handlers."
sidebar:
  order: 926
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_set_element_handler.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_set_element_handler.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_set_element_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_set_element_handler.md).
