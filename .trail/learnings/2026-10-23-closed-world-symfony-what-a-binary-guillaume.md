---
title: "Closed-world Symfony: what a binary with no interpreter must compile itself"
author: Guillaume
date: 2026-10-23
branch: reconcile/dirname-symfony
---

## Decided
- In --closed-world every runtime include is answered from the compiled include table (`closed_world::include_table`): Unit, Declarations or NotCompiled. Anything missing is a fatal, so the world must be built generically, never per framework:
  - entry self-include (`require $_SERVER['SCRIPT_FILENAME']`): `entry_return` now runs for CLI too; its idempotent prefix accepts the resolver's leading declaration block (`NamespaceBlock{name:None}`) and read-only throwing guards (`if (!is_dir(...)) throw`).
  - literal `require` left inside function/method bodies (the resolver rewrites them to `__elephc_dynamic_include(path,..)`): `opcache_preload_sources::literal_include_units`.
  - data files reached through computed paths (`Kernel::getBundlesPath()`, polyfill unidata): `project_data_units`, every pure `return <const>` file of the project, closed-world only.
  - `$classes[]` written in the preload script ITSELF (Symfony's generated preload) is read from `parsed`, not only from `resolved`.
  - class strings into a parameter sink (`$this->addIfExists(X::class)` -> `class_exists($class)`/`new $class`): `walk::collect_class_string_parameter_flows`.
  - names derived from `static::class` in inherited methods (`Extension::getConfiguration()`): `dynamic_contracts::late_static_class_string_candidates`.
- A refused construct demoted by `runtime_faults` can leave cascade errors in the same file (an `if` condition whose `||` assignment never got installed -> "Undefined variable $r"); the file is re-checked before it is deferred.
## Tried and ruled out
- Appending unit functions at the end of the program: the optimizer drops statements after a top-level `return` (bin/console ends with one). Splice them at the front.
- `read_to_string` on Composer's generated classmap: it holds a Latin-1 class name (`'\xa9'`), read bytes + from_utf8_lossy.
## Watch out
- lldb on a 220 MB binary: plain `run` with breakpoint commands silently never stops; use `process launch --stop-at-entry`, then a Python callback that writes to a file. Build with --keep-symbols to get `_rt_*` names.
- `entry_return`'s receiver check is textual on the Debug form; match whole words (`StaticEnvVarLoader` contains `Static`).
