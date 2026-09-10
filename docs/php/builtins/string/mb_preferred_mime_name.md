---
title: "mb_preferred_mime_name()"
description: "Returns the preferred MIME encoding name, or false when none is registered."
sidebar:
  order: 833
---

## mb_preferred_mime_name()

```php
function mb_preferred_mime_name(string $encoding): string|false
```

Returns the preferred MIME encoding name, or false when none is registered.

**Parameters**:
- `$encoding` (`string`)

**Returns**: `string|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_preferred_mime_name.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_preferred_mime_name.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_preferred_mime_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_preferred_mime_name.md).
