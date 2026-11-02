# Native / dynamic inclusion semantics — working specification

Status: proposed, not consensus-locked. Stock Symfony HTTP acceptance remains open.

## Measured failures

`native_eval_once` has four failing CLI-backed regressions. Native then dynamic
and dynamic then native both execute a side-effect-only file twice. An interface
file already included natively is redeclared dynamically. An interface discovered
under a skipped native branch is also rejected on its first dynamic inclusion.
Evidence: target/native-eval-include-once-red.log, four failures, 99.33 seconds.

Native once markers contain FNV-derived guard labels and update native globals.
Dynamic includes use canonical-path context/request sets. Declaration discovery
hoists interfaces into compiler metadata independently of file execution. Metadata
availability must therefore not serve as proof of PHP declaration activation.

## Invariants

- Never modify the application or its vendor sources to accommodate the compiler.
- Never skip a file merely because it contains a known class/interface name.
- Never mark all discovered or compiled files as already included.
- Preserve real redeclaration errors, include side effects, caller scope, include
  return values, autoload order, canonical path identity and request isolation.
- Preserve native execution for known compiled code; do not solve known-file
  redeclarations by forcing all their bodies through the interpreter.
- Handle classes, interfaces, traits, enums and function activation consistently;
  distinguish PHP early binding within a file from that file having been entered.

## Implementation direction to validate

1. Carry typed canonical source identity alongside native inclusion guards. Use
   existing resolver provenance rather than reverse-engineering hashes or encoding
   extra data inside labels. A shared request-local inclusion state must reflect
   actual entry, in both directions, with no eager activation of skipped branches.
2. Separate compiled declaration metadata from active PHP symbol state. Discovery
   contributes signatures/code/provenance, not an unconditional declaration event.
   Activation must agree with PHP file-entry/conditional declaration rules.
3. Investigate reusable compiled file-entry dispatch for dynamic includes targeting
   known sources. Preserve include caller scope and returns using existing native
   scope facilities. Do not replace a previously compiled file body with a no-op.
4. Keep genuinely runtime-generated sources on their existing dynamic path. Reject
   conflicting declarations; provenance is not permission to accept altered code.

The file-entry boundary and activation consumers require a source-level inventory
before implementation. No claim that registry synchronization alone fixes r167.

## Required focused evidence

- Existing four native_eval_once tests turn green without weakened expectations.
- Plain include/require repeats side effects while subsequent once forms skip.
- Alternate relative/absolute/canonical spellings identify the same loaded file.
- Missing/unreadable files and parse/execution failures follow measured PHP rules;
  do not assume all failures undo registration.
- A skipped native include does not activate its declarations or consume its body.
- Real duplicates from different files or repeated non-once declaration files fail.
- Includes inside functions share the correct caller variables and return values.
- Autoload callbacks retain PHP ordering, including compiled-known but inactive types.
- Web request reset does not retain the previous request's inclusion/activation state.
- Supported targets receive coherent ABI/emission changes; distinguish executable
  tests from emission-only evidence.
- Recompile stock Symfony in web mode and obtain its expected Welcome page with
  HTTP 404. Compilation or reduced fixtures alone are not final acceptance.

## Review boundary

Any reviewer lock must identify the exact specification hash. This draft has no
such locks and no requested GLM/Kimi consensus. Existing read-only reviews establish
local mechanisms only, not agreement on this architecture or completion.
