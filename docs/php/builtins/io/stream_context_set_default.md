---
title: "stream_context_set_default()"
description: "Sets the default stream context."
sidebar:
  order: 214
---

## stream_context_set_default()

```php
function stream_context_set_default(array $options): mixed
```

Sets the default stream context.

**Parameters**:
- `$options` (`array`)

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_default.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_default.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_context_set_default` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_set_default.md).
