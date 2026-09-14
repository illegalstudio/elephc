---
title: "xml_get_current_line_number()"
description: "Returns the current line number of the parser."
sidebar:
  order: 975
---

## xml_get_current_line_number()

```php
function xml_get_current_line_number(mixed $parser): int
```

Returns the current line number of the parser.

**Parameters**:
- `$parser` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_get_current_line_number.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_get_current_line_number.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_get_current_line_number` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_get_current_line_number.md).
