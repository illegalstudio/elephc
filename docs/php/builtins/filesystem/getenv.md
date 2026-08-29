---
title: "getenv()"
description: "Gets the value of an environment variable, or the whole environment."
sidebar:
  order: 133
---

## getenv()

```php
function getenv(string $name = null, bool $local_only = false): mixed
```

Gets the value of an environment variable, or the whole environment.

**Parameters**:
- `$name` (`string`), default `null`, optional
- `$local_only` (`bool`), default `false`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/network_env/getenv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/network_env/getenv.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._

## Internals

For how `getenv` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/getenv.md).
