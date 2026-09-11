---
title: "xmlwriter_end_cdata()"
description: "Ends the current CDATA section."
sidebar:
  order: 935
---

## xmlwriter_end_cdata()

```php
function xmlwriter_end_cdata(mixed $writer): bool
```

Ends the current CDATA section.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_cdata.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_end_cdata.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_end_cdata` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_end_cdata.md).
