---
title: "stream_context_set_options()"
description: "Sets several options on the specified context from an array."
sidebar:
  order: 233
---

## stream_context_set_options()

```php
function stream_context_set_options(mixed $context, mixed $options): bool
```

Sets several options on the specified context from an array.

**Parameters**:
- `$context` (`mixed`)
- `$options` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_options.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_options.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_context_set_options` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_set_options.md).
