---
title: "xmlwriter_write_cdata()"
description: "Writes a complete CDATA section."
sidebar:
  order: 990
---

## xmlwriter_write_cdata()

```php
function xmlwriter_write_cdata(mixed $writer, string $content): bool
```

Writes a complete CDATA section.

**Parameters**:
- `$writer` (`mixed`)
- `$content` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_cdata.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_cdata.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_cdata` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_cdata.md).
