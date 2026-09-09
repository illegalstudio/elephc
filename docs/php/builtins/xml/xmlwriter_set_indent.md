---
title: "xmlwriter_set_indent()"
description: "Toggles indentation of the output."
sidebar:
  order: 948
---

## xmlwriter_set_indent()

```php
function xmlwriter_set_indent(mixed $writer, bool $enable): bool
```

Toggles indentation of the output.

**Parameters**:
- `$writer` (`mixed`)
- `$enable` (`bool`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_set_indent.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_set_indent.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_set_indent` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_set_indent.md).
