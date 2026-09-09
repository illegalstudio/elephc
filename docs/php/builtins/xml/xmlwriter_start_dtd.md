---
title: "xmlwriter_start_dtd()"
description: "Starts a DTD."
sidebar:
  order: 955
---

## xmlwriter_start_dtd()

```php
function xmlwriter_start_dtd(mixed $writer, string $qualifiedName, ?string $publicId = null, ?string $systemId = null): bool
```

Starts a DTD.

**Parameters**:
- `$writer` (`mixed`)
- `$qualifiedName` (`string`)
- `$publicId` (`?string`), default `null`, optional
- `$systemId` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_dtd` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_dtd.md).
