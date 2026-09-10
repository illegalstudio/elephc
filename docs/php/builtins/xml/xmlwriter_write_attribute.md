---
title: "xmlwriter_write_attribute()"
description: "Writes a complete attribute."
sidebar:
  order: 1024
---

## xmlwriter_write_attribute()

```php
function xmlwriter_write_attribute(mixed $writer, string $name, string $value): bool
```

Writes a complete attribute.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)
- `$value` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_attribute.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_attribute.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_attribute` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_attribute.md).
