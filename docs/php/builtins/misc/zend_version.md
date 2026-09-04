---
title: "zend_version()"
description: "Returns the Zend Engine version for the active PHP profile."
sidebar:
  order: 360
---

## zend_version()

```php
function zend_version(): string
```

Returns the Zend Engine version for the active PHP profile.

**Parameters**: none.

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected version prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/zend_version.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/zend_version.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `zend_version` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/zend_version.md).
