---
title: "xmlwriter_end_dtd_entity()"
description: "Ends the current DTD entity declaration."
sidebar:
  order: 965
---

## xmlwriter_end_dtd_entity()

```php
function xmlwriter_end_dtd_entity(mixed $writer): bool
```

Ends the current DTD entity declaration.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd_entity.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_dtd_entity.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_dtd_entity` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_dtd_entity.md).
