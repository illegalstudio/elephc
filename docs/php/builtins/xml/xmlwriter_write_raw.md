---
title: "xmlwriter_write_raw()"
description: "Writes raw, unescaped content."
sidebar:
  order: 999
---

## xmlwriter_write_raw()

```php
function xmlwriter_write_raw(mixed $writer, string $content): bool
```

Writes raw, unescaped content.

**Parameters**:
- `$writer` (`mixed`)
- `$content` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_raw.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_raw.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_raw` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_raw.md).
