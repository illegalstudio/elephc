---
title: "xmlwriter_start_document()"
description: "Writes the XML declaration."
sidebar:
  order: 979
---

## xmlwriter_start_document()

```php
function xmlwriter_start_document(mixed $writer, ?string $version = '1.0', ?string $encoding = null, ?string $standalone = null): bool
```

Writes the XML declaration.

**Parameters**:
- `$writer` (`mixed`)
- `$version` (`?string`), default `'1.0'`, optional
- `$encoding` (`?string`), default `null`, optional
- `$standalone` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_document.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_document.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_document` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_document.md).
