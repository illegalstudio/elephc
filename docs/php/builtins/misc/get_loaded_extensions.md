---
title: "get_loaded_extensions()"
description: "Returns an array with the names of all loaded modules."
sidebar:
  order: 329
---

## get_loaded_extensions()

```php
function get_loaded_extensions(bool $zend_extensions = false): array
```

Returns an array with the names of all loaded modules.

**Parameters**:
- `$zend_extensions` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/get_loaded_extensions.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/get_loaded_extensions.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `get_loaded_extensions` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/get_loaded_extensions.md).
