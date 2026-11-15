---
title: The unified monitor page composes the two existing reports instead of rewriting the graph
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 120 }
touches:
  - src/monitor/call_html.rs
  - src/monitor/calls.rs
  - src/monitor/command.rs
  - tests/codegen/cli.rs
  - docs/beyond-php/debugging.md
  - docs/compiling/cli-reference.md
  - .plans/native-debugger.md
---

## Decided

- Calls-mode `--html` now writes one shell page with **Profile**, **Calls**,
  **Dumps** and **Trace segments** tabs. The Profile and Calls bodies are the
  existing full pages embedded as `srcdoc` iframes:
  - the exact profile keeps its interactive call graph, timeline, source view,
    filters and theme;
  - the calls body keeps the nested tree, values, redaction/truncation badges,
    Dumps and Trace segments.
- `srcdoc` values are HTML-attribute escaped (`&`, `<`, `>`, `"`, `'`). The shell
  has no network assets; both frames are offline documents.
- Top-level tabs activate the matching frame. Dumps/Traces activate the Calls
  frame and scroll its `id="dumps"`/`id="traces"` section once the frame loads.
- Calls-mode `--html` implies the exact `--with-monitoring` capture, because the
  page's Profile tab must actually exist. The exact table still prints first.
  `monitor --exact` without `--calls` keeps its profile-only page.
- The shell summary carries target, capture mode, captured-call count and the
  event-ring dropped count. Value truncation remains visible in the calls frame
  and JSONL.
- A missing profile (capture failure) renders an explanatory profile panel
  instead of a blank iframe, and the command still fails as before.

## Tried and ruled out

- Merging the two documents' `<style><script><body>` strings into one document:
  both pages define generic `.summary`, `.badge`, table and control styles, so a
  flat merge would silently restyle one report. Iframes keep each view exactly
  as tested.
- Rewriting `call_graph::render_html_frames` to return fragments: it is a large
  self-contained JS/CSS application with a single-page contract. Extracting a
  fragment would touch every panel and its tests for no user-visible gain.
- Client-side JSON loading via `fetch`: `file://` pages are commonly blocked from
  reading sibling files, and the existing reports are offline-openable. `srcdoc`
  keeps the guarantee.
- A calls-only unified page when `--exact` was omitted: the acceptance and the
  page's Profile tab require performance data. Imply the exact capture instead of
  showing an empty Profile tab.
- Using `<object data="...">`: it needs a second file and breaks the one-page
  offline artifact.

## Watch out

- Iframe `srcdoc` increases the HTML size by roughly the sum of the two reports;
  both are already bounded/static.
- The top-level page needs JavaScript for tab switching. The frames themselves
  render even without it; Profile is the default panel.
- The Dumps/Traces scroll uses same-origin `contentDocument`; `srcdoc` iframes
  are same-origin, but a future remote-frame variant would need a postMessage
  bridge.
- Calls-mode `--html` on a binary requires both debug metadata and the monitoring
  marker; the error names `--calls --exact/--html`.
- The exact profile currently includes the debug helper functions among its
  function nodes; the call tree still filters them. Filtering the profile graph is
  a separate quality item.
- The e2e fixture records a 400-byte string, a dump and a `collect` segment; the
  test asserts the unified title, tabs, `call graph`, `captured calls`, call
  values, `144 bytes omitted`, `Dumps`, `Trace segments` and no network assets.
