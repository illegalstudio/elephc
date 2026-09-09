---
title: "xmlwriter_write_dtd_entity()"
description: "Writes a complete DTD entity declaration."
sidebar:
  order: 995
---

## xmlwriter_write_dtd_entity()

```php
function xmlwriter_write_dtd_entity(mixed $writer, string $name, string $content, bool $isParam = false, ?string $publicId = null, ?string $systemId = null, ?string $notationData = null): bool
```

Writes a complete DTD entity declaration.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)
- `$content` (`string`)
- `$isParam` (`bool`), default `false`, optional
- `$publicId` (`?string`), default `null`, optional
- `$systemId` (`?string`), default `null`, optional
- `$notationData` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_dtd_entity.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_dtd_entity.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_dtd_entity` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_dtd_entity.md).
