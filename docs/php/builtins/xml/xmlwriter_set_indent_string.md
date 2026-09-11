---
title: "xmlwriter_set_indent_string()"
description: "Sets the string used for one indentation level."
sidebar:
  order: 950
---

## xmlwriter_set_indent_string()

```php
function xmlwriter_set_indent_string(mixed $writer, string $indentation): bool
```

Sets the string used for one indentation level.

**Parameters**:
- `$writer` (`mixed`)
- `$indentation` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_set_indent_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_set_indent_string.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_set_indent_string` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_set_indent_string.md).
