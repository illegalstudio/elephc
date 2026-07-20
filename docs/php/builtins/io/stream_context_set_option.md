---
title: "stream_context_set_option()"
description: "Sets an option on the specified context."
sidebar:
  order: 213
---

## stream_context_set_option()

```php
function stream_context_set_option(resource $context, string $wrapper_or_options, string $option_name = null, mixed $value = null): bool
```

Sets an option on the specified context.

**Parameters**:
- `$context` (`resource`)
- `$wrapper_or_options` (`string`)
- `$option_name` (`string`), default `null`, optional
- `$value` (`mixed`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_option.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_context_set_option.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_context_set_option` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_context_set_option.md).

