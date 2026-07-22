---
title: "stream_context_get_default()"
description: "Retrieves the default stream context."
sidebar:
  order: 211
---

## stream_context_get_default()

```php
function stream_context_get_default(array $options = null): mixed
```

Retrieves the default stream context.

**Parameters**:
- `$options` (`array`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_default.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_get_default.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_context_get_default` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_get_default.md).
