---
title: "Console --env=dev: two preloads, stale container dumps, method-body include_once"
author: Guillaume
date: 2026-10-25
branch: reconcile/dirname-symfony
---

## Decided
- A console that must answer `--env=dev` like php is built with two `opcache.preload` roots: Symfony's dev dump FIRST, prod LAST (`preloaded_declarations` merges every directive).
- Each preload's declarations keep the first declaration of each name (`first_declaration_of_each_name`): the script's own `require` of the container stub and the stub's include unit both bring the container class. A LAST preload hid it because its declarations are filtered against the entry's.
- `preload_directory_closure` does not `require_once` a declarations file that a unit (container stub) of the same directory includes itself.
- Discovery records where each discovered declaration came from (`DiscoveryOutput::into_include_discovery` -> `IncludeDiscovery::class_files`), merged into `IncludedDeclarationSources`. Discovery CLONES its `ResolveState` per method/branch, so recording into the state is lost; the output entries are what survives.
- Closed world: a declaration file with activation cells (interfaces, probe-only classes) gets a `__rt_closed_include_activate_<n>` routine in its record's unit slot, and is not primed as "compiler ran it", so the autoloader's include raises the cells (`interface_exists()` true after autoload, as php).
- `iterator_to_array()`/`iterator_count()` over an object of a class the world does not declare is accepted (php fails at the `new`).
- `isset/empty($o->$name)` is a quiet-fetch chain like `$o->name`.

## Tried and ruled out
- Making discovery entries non-repeatable to drop the duplicate: the repeatable duplicate is deliberate (it emulates php's redeclaration fatal for sequential plain includes).
- Compiling stale container dumps of other debug modes (`var/cache/dev/Container*` left by APP_DEBUG=1 runs): 598 MB binary. The build script now runs `php bin/console cache:clear` per env before building.

## Watch out
- `var/cache/<env>/` accumulates dumps; any build over it compiles all of them through the directory closure.
- `error_tests spl_builtins` has 4 constructor-type failures that already existed at c339.
