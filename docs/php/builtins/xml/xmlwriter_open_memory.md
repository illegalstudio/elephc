---
title: "xmlwriter_open_memory()"
description: "Creates a writer that buffers its output in memory."
sidebar:
  order: 1006
---

## xmlwriter_open_memory()

```php
function xmlwriter_open_memory(): mixed
```

Creates a writer that buffers its output in memory.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_open_memory.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/xml/xmlwriter_open_memory.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_open_memory` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_open_memory.md).
