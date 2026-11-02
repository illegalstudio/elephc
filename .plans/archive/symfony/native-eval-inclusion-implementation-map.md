# Inclusion implementation inventory

Read-only inventory supporting specification revision 3; no implementation yet.

- `src/resolver/engine_includes.rs::resolve_include_stmt` has the canonical file
  path and parsed file program before occurrence-specific resolution, declaration
  stripping and return discarding. Source-unit capture must precede those losses.
- `src/resolver/state.rs::ResolveState` currently records declaration-file maps,
  namespace/import state and conditional defines. Its declaration-file maps are
  Reflection provenance, not active-symbol or included-file state.
- `src/resolver/mod.rs::resolve_collecting_includes_with_defines_and_sources`
  exports the resolved program, sorted manifest paths and declaration provenance.
  A source catalog must reach all real lowering entry paths, including test
  harnesses; do not silently wire only the CLI and leave other pipelines divergent.
- `src/ir_lower/program.rs::lower` initializes Module, populates metadata, lowers
  functions/methods/initializers/main and validates the module. Source entries and
  source/declaration IDs belong in typed metadata, not string label conventions.
- `src/ir/module.rs` currently has DataId and a Module/DataPool, but no SourceId
  definition was found in indexed src code. Avoid conflating existing data indices
  with the new source identity domain.
- `src/codegen/lower_inst/core_includes.rs` currently owns independent native
  comm guards. Their state must become the registry's native request-state view.
- `src/codegen/lower_inst/builtins/eval/native_scope.rs` provides existing native
  scope-binding/reference operations to investigate for compiled file entries.
  Their existence is not yet proof that every include-scope requirement is covered.
- `src/codegen/web.rs::emit_web_reset` already orders PHP-visible releases before
  heap reset, but currently clears only dynamic inclusion state. Native source and
  activation state must join the reset inventory before arena reset.

Remaining inventory: autoload source-unit capture, active class-like/constant
consumers, compiled file-entry ABI wrappers and all test-pipeline metadata wiring.
