# Repo memory

One verified fact per file in `packets/`, plain markdown, reviewed in PRs like code.
Nothing here is generated: the search index lives in `.git/kioku/` and is rebuildable
with `kioku index --full`, so it is never committed and never shows up in `git status`.

Read one with `kioku show <id>`, search with `kioku recall "<question>"`, and check that
every memory still matches the code with `kioku verify`.
