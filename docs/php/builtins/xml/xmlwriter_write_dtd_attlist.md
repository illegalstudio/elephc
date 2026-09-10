---
title: "xmlwriter_write_dtd_attlist()"
description: "Writes a complete DTD attribute list declaration."
sidebar:
  order: 1029
---

## xmlwriter_write_dtd_attlist()

```php
function xmlwriter_write_dtd_attlist(mixed $writer, string $name, string $content): bool
```

Writes a complete DTD attribute list declaration.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)
- `$content` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_dtd_attlist.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_write_dtd_attlist.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_write_dtd_attlist` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_write_dtd_attlist.md).
