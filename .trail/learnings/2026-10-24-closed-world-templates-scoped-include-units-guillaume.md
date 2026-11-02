---
title: "Closed-world templates: scoped include units, extract scope, multi-echo $this"
author: Guillaume
date: 2026-10-24
branch: reconcile/dirname-symfony
touches:
  - src/scoped_include_units.rs
---

## Decided
- A computed-path `include` of a template directory (Symfony `HtmlErrorRenderer::include()`: `extract($context); include dirname(__DIR__).'/Resources/'.$name;`) is answered in closed world by a SCOPED unit: `src/scoped_include_units.rs` builds, per template list, `__elephc_scoped_units_<fnv>(string $path): ?\Closure` = `match (realpath($path)) { '<file>' => function (array $__elephc_vars) { $v = $__elephc_vars['v'] ?? null; ...template...; return 1; }, default => null }`, and rewrites the site to `(($u = lookup($p = PATH)) !== null) ? \Closure::bind($u, $this, self::class)(get_defined_vars()) : __elephc_dynamic_include($p, false, req)`. All AST, no parsed PHP. Wired in `pipeline.rs` right after `literal_include_units`, closed world only.
- Candidate templates: every file under the directory the path's literal prefix names (`dirname('<lit>')` folded), max 64, not a dir holding composer.json. A file with no `<?` (css, svg) becomes one `echo` of its content.
- The rewrite is only done where no write-back can be observed: exactly one include in the body, not in a loop or closure, no RefAssign/global/static, no get_defined_vars/compact/func_get_args in the caller (extract before the site is fine), and no template variable name mentioned by the caller from the site's statement on.
- `get_defined_vars()` after a closed-world `extract()` adds the kept source array: `locals + (is_array(src) ? src : [])` (core_introspection.rs). The extract temporaries are now `#gen` names, so they no longer show in get_defined_vars.
- Closure `$this` detection (`static_closure.rs::stmt_uses_this`) must walk `StmtKind::Synthetic` — the parser turns `echo a, b;` into a Synthetic of Echo statements — plus IfDef, IncludeOnceGuard, DynamicStaticPropertyWrite.

## Tried and ruled out
- Seeding only "read-before-write" template variables: not flow-sensitive enough to be safe; every mentioned name is seeded.
- Unit functions (no scope) for templates: they cannot see the extracted context nor `$this`.

## Watch out
- Seeding makes template locals Mixed: `$x = explode(...)` after the seed hit "array_slice result PHP type Mixed" (fixed in reduce_sets.rs by boxing the concrete slice) and an OPEN leak with `end($x)` (slot freed as Mixed while holding a raw array).
- A full disk (each sfweb build keeps a ~1.3 GB .s) makes builds and tests fail with ENOSPC; check `df` and delete old .s.
