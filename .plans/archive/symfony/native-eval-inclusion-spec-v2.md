# Native / dynamic inclusion semantics — working specification

Status: revision 2, proposed, not consensus-locked. Stock Symfony HTTP acceptance remains open.

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

## Measured state transitions

Oracle: php -n 8.5.6, target/include-state-oracle/php.stdout and its three fixtures.

| Event | Once registered | Declarations | Later include_once |
| --- | --- | --- | --- |
| Discovery only / skipped include | No | Metadata only | Must enter |
| Open fails | No | No new activation | Retries |
| Open succeeds, parsing fails | Yes | No new activation | Returns true, skips |
| Compilation succeeds, body throws | Yes | Eligible unconditional declarations already bound | Returns true, skips |
| Body returns normally | Yes | Early bindings plus declarations reached during execution | Returns true, skips |

Do not model a single rollback-on-failure state. Registration happens before
parsing/execution, after successful opening. Early binding is not equivalent to
executing each declaration statement. The oracle establishes early binding for
the simple parentless class/interface/function fixture, not every inheritance
or conditional-declaration case.

## Implementation decisions

1. **One request-local registration authority.** Native and dynamic includes use
   the same logical source registry. Compiled source descriptors own addressable
   once-state cells; runtime-only sources use dynamically allocated registry rows.
   Canonical lookup chooses exactly one row, never mirrored independent states.
   Existing native fast guards may read/write the descriptor's actual state cell;
   eval must query/update that same cell for compiled-known sources. Unknown paths
   do not acquire a second entry when later resolved to a compiled source.
2. **Typed identity, not encoded labels.** Introduce a SourceId and canonical source
   key in resolver/IR metadata, retaining path bytes and provenance. Guard symbols
   are implementation details, not reversible path storage or collision-prone sole
   identity. Registration describes available code but does not set its once bit.
3. **Separate active symbols.** Immutable compiler metadata holds declaration kind,
   canonical name, source identity, declaration site and implementation. Request
   state records activation independently. PHP existence/autoload checks consult
   active symbols, not mere signature availability. Builtin types are active from
   request initialization. User file declarations activate according to PHP early
   binding eligibility or their executed conditional declaration site. Constants,
   aliases and functions must not be implicitly activated just by source discovery.
4. **Compiled file entry.** Preserve each known user file's top-level body and its
   declaration events as a source unit before occurrence-specific include folding
   discards returns or hoists declarations. Emit a reusable EIR entry for that body;
   methods/functions remain shared implementations rather than duplicated source.
   Dynamic inclusion of a compiled-known source invokes this entry, not an eval of
   the original declarations and not a no-op. Native inline inclusion and the entry
   must share registration/activation semantics. Capture-dependent includes must
   not reuse an entry specialized to one caller's constant variable values.
5. **Entry ABI and scope.** The internal entry accepts the current execution context,
   the include caller's scope view, and an output/result cell, using the existing
   native-call exception/result boundary. It is not a new user PHP function frame.
   Reads/writes, references, new variables, bound this and class visibility use the
   caller scope. Explicit return exits only the included unit; fallthrough returns
   PHP's include value. Known code executes as EIR/native instructions; scope access
   helpers do not interpret its body. Native-only programs must not link Magician
   merely for ordinary static includes.
6. **Provider and ordering.** Preserve the existing AOT-frozen compiled-script model
   for bundled known sources; runtime-only sources use normal filesystem/stream
   opening. Do not infer that a compiled provider has been included. A once check
   precedes repeated entry; successful opening registers before compilation/entry.
   Ordinary include/require executes again regardless of the once bit. Autoload
   order is unchanged: known but inactive type metadata is not an autoload success.
7. **Lifetimes and failure.** Reset mutable registration and activation at each web
   request boundary; immutable descriptors survive. Retain registrations after
   parse/runtime failures per the oracle. Preserve genuine redeclaration errors
   and declaration effects already performed; do not roll back the entire file.
8. **Preserve the manifest set.** declared_once currently feeds the sorted OPcache
   manifest. No deletion/elimination behavior is established by its insert calls.
   Do not remove it based on the first peer reviews' unsupported claim. Any future
   compile-time once elimination requires a separate dominance/order proof.

Implement in dependency order: typed source units/registry, native/dynamic once
interop, declaration activation, compiled file entries and scope/return handling,
then full acceptance. Intermediate green subsets do not close r167. Before editing
each stage, inventory its actual producers/consumers and reuse existing mechanisms;
API spellings above are design roles, not claims that those types already exist.

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
