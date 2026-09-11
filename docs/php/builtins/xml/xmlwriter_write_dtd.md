---
title: "xmlwriter_write_dtd()"
description: "Writes a complete DTD."
sidebar:
  order: 968
---

## xmlwriter_write_dtd()

```php
function xmlwriter_write_dtd(mixed $writer, string $name, ?string $publicId = null, ?string $systemId = null, ?string $content = null): bool
```

Writes a complete DTD.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)
- `$publicId` (`?string`), default `null`, optional
- `$systemId` (`?string`), default `null`, optional
- `$content` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_dtd.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_dtd.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_dtd` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_dtd.md).
