"""Canonical keys for PHP source PHPT coverage records.

Coverage ledgers identify tests relative to the pinned PHP source tree.  The
repository stores that tree below ``php-src/``, but that storage prefix is not
part of a ledger key.  Keeping validation here prevents generators and the
strict checker from accepting subtly different spellings of one test.
"""

from __future__ import annotations

from pathlib import PurePosixPath


CANONICAL_PHPT_PREFIX = "ext"
PHPT_COMPONENTS = frozenset(("dom", "libxml", "simplexml"))


class PhptPathError(ValueError):
    """Describe a malformed, traversing, or ambiguously prefixed PHPT key."""

    def __init__(self, code: str, value: object) -> None:
        """Build an error carrying a stable diagnostic code and offending value."""
        self.code = code
        self.value = value
        super().__init__(f"{code}:{value}")


def canonical_phpt_key(value: object) -> str:
    """Validate and return one canonical ``ext/<component>/tests/*.phpt`` key."""
    if not isinstance(value, str) or not value:
        raise PhptPathError("UNSAFE_PATH", value)
    if "\\" in value:
        raise PhptPathError("UNSAFE_PATH", value)
    path = PurePosixPath(value)
    parts = path.parts
    if path.is_absolute() or any(part in ("", ".", "..") for part in parts):
        raise PhptPathError("UNSAFE_PATH", value)
    if parts[0] != CANONICAL_PHPT_PREFIX:
        raise PhptPathError("AMBIGUOUS_PHPT_PATH", value)
    if len(parts) < 4 or parts[1] not in PHPT_COMPONENTS or parts[2] != "tests":
        raise PhptPathError("UNSAFE_PATH", value)
    if not parts[-1].endswith(".phpt"):
        raise PhptPathError("UNSAFE_PATH", value)
    canonical = path.as_posix()
    if value != canonical:
        raise PhptPathError("AMBIGUOUS_PHPT_PATH", value)
    return canonical
