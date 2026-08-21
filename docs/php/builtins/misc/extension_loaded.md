---
title: "extension_loaded()"
description: "Checks whether a named PHP extension is loaded."
sidebar:
  order: 334
---

## extension_loaded()

```php
function extension_loaded(string $extension): bool
```

Checks whether a named PHP extension is loaded.

**Parameters**:
- `$extension` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/extension_loaded.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/extension_loaded.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `extension_loaded` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/extension_loaded.md).
