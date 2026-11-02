---
title: "proc_open()"
description: "Executes a command and opens file pointers for input/output."
sidebar:
  order: 783
---

## proc_open()

```php
function proc_open(array|string $command, array $descriptor_spec, mixed $pipes, ?string $cwd = null, ?array $env_vars = null, ?array $options = null): mixed
```

Executes a command and opens file pointers for input/output.

**Parameters**:
- `$command` (`array|string`)
- `$descriptor_spec` (`array`)
- `$pipes` (`mixed`), passed by reference
- `$cwd` (`?string`), default `null`, optional
- `$env_vars` (`?array`), default `null`, optional
- `$options` (`?array`), default `null`, optional

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported through the compiler-injected proc prelude.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_open.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_open.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `proc_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/proc_open.md).
