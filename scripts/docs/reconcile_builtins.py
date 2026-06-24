"""One-shot helper to move files into their canonical per-area folders.

Reads the registry, and for every builtin:
  - finds all .md files with the same slug under any area folder
  - keeps only the one in the area matching the registry
  - moves any extras (e.g. leftovers from a prior area assignment) into the
    correct folder, overwriting if needed (the page content is generated so
    overwriting is safe)
"""
import json
import shutil
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
REGISTRY = REPO / "scripts" / "docs" / "builtin_registry.json"
USER_DIR = REPO / "docs" / "php" / "builtins"
INTERNALS_DIR = REPO / "docs" / "internals" / "builtins"


def slug(name: str) -> str:
    return name.replace("\\", "-").replace("::", "-")


def reconcile(root: Path, name_to_area: dict[str, str]) -> int:
    moved = 0
    # Build the set of canonical target paths (one per builtin)
    canonical_targets: set[Path] = set()
    for name, area in name_to_area.items():
        s = slug(name)
        target_sub = "_internal" if name.startswith("__elephc_") else area.lower()
        canonical_targets.add((root / target_sub / f"{s}.md").resolve())
    # Set of top-level area index files (e.g. <root>/date.md) — these are
    # generated as area indexes and must not be moved into a subfolder.
    top_level_indexes: set[Path] = set()
    for area in {a.lower() for a in name_to_area.values()}:
        top_level_indexes.add((root / f"{area}.md").resolve())
    for name, area in name_to_area.items():
        s = slug(name)
        target_sub = "_internal" if name.startswith("__elephc_") else area.lower()
        target = root / target_sub / f"{s}.md"
        # search for any duplicates under other area folders
        for path in root.rglob(f"{s}.md"):
            if path == target:
                continue
            if path.resolve() in top_level_indexes:
                # it's a top-level area index — leave it alone
                continue
            if path.resolve() in canonical_targets:
                # it's already in its canonical home
                continue
            # it's a duplicate — move it (overwriting is fine, content is generated)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(path), str(target))
            moved += 1
            try:
                path.parent.rmdir()
            except OSError:
                pass
    return moved


def main() -> int:
    raw = json.loads(REGISTRY.read_text(encoding="utf-8"))
    name_to_area = {b["name"]: b["area"] for b in raw if b["in_catalog"]}
    user_moved = reconcile(USER_DIR, name_to_area)
    int_moved = reconcile(INTERNALS_DIR, name_to_area)
    print(f"User pages moved: {user_moved}")
    print(f"Internals pages moved: {int_moved}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
