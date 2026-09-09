---
title: "xmlwriter_output_memory()"
description: "Returns the buffered output of a memory writer."
sidebar:
  order: 947
---

## xmlwriter_output_memory()

```php
function xmlwriter_output_memory(mixed $writer, bool $flush = true): string
```

Returns the buffered output of a memory writer.

**Parameters**:
- `$writer` (`mixed`)
- `$flush` (`bool`), default `true`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_output_memory.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_output_memory.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_output_memory` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_output_memory.md).
