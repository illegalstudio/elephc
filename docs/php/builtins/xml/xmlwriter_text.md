---
title: "xmlwriter_text()"
description: "Writes escaped text content."
sidebar:
  order: 1023
---

## xmlwriter_text()

```php
function xmlwriter_text(mixed $writer, string $content): bool
```

Writes escaped text content.

**Parameters**:
- `$writer` (`mixed`)
- `$content` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_text.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_text.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_text` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_text.md).
