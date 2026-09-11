---
title: "xml_get_current_byte_index()"
description: "Returns the current byte index of the parser."
sidebar:
  order: 913
---

## xml_get_current_byte_index()

```php
function xml_get_current_byte_index(mixed $parser): int
```

Returns the current byte index of the parser.

**Parameters**:
- `$parser` (`mixed`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xml_get_current_byte_index.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xml_get_current_byte_index.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xml_get_current_byte_index` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xml_get_current_byte_index.md).
