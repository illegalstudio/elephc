---
title: "xmlwriter_start_cdata()"
description: "Starts a CDATA section."
sidebar:
  order: 953
---

## xmlwriter_start_cdata()

```php
function xmlwriter_start_cdata(mixed $writer): bool
```

Starts a CDATA section.

**Parameters**:
- `$writer` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_cdata.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_start_cdata.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_start_cdata` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_start_cdata.md).
