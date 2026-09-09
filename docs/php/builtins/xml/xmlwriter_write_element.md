---
title: "xmlwriter_write_element()"
description: "Writes a complete element."
sidebar:
  order: 996
---

## xmlwriter_write_element()

```php
function xmlwriter_write_element(mixed $writer, string $name, ?string $content = null): bool
```

Writes a complete element.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)
- `$content` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_element.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_element.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_element` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_element.md).
