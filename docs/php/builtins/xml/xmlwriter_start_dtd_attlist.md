---
title: "xmlwriter_start_dtd_attlist()"
description: "Starts a DTD attribute list declaration."
sidebar:
  order: 981
---

## xmlwriter_start_dtd_attlist()

```php
function xmlwriter_start_dtd_attlist(mixed $writer, string $name): bool
```

Starts a DTD attribute list declaration.

**Parameters**:
- `$writer` (`mixed`)
- `$name` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd_attlist.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd_attlist.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_dtd_attlist` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_dtd_attlist.md).
