---
title: "xmlwriter_start_dtd_element()"
description: "Starts a DTD element declaration."
sidebar:
  order: 958
---

## xmlwriter_start_dtd_element()

```php
function xmlwriter_start_dtd_element(mixed $writer, string $qualifiedName): bool
```

Starts a DTD element declaration.

**Parameters**:
- `$writer` (`mixed`)
- `$qualifiedName` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd_element.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_dtd_element.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_dtd_element` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_dtd_element.md).
