---
title: "xmlwriter_start_attribute()"
description: "Starts an attribute."
sidebar:
  order: 975
---

## xmlwriter_start_attribute()

```php
function xmlwriter_start_attribute(mixed $writer, string $name): bool
```

Starts an attribute.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_attribute.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_attribute.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_attribute` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_attribute.md).
