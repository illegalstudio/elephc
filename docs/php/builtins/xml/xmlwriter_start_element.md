---
title: "xmlwriter_start_element()"
description: "Starts an element."
sidebar:
  order: 984
---

## xmlwriter_start_element()

```php
function xmlwriter_start_element(mixed $writer, string $name): bool
```

Starts an element.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_element.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_element.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_element` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_element.md).
