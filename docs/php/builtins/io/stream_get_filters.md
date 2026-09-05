---
title: "stream_get_filters()"
description: "Retrieves list of registered filters."
sidebar:
  order: 228
---

## stream_get_filters()

```php
function stream_get_filters(): array
```

Retrieves list of registered filters.

**Parameters**: none.

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/stream_get_filters.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/stream_get_filters.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_get_filters` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_get_filters.md).
