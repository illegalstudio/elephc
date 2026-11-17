#!/usr/bin/env python3
"""Exclude irrelevant archive extras without changing specialized test execution settings."""

import argparse
from pathlib import Path
import re


def configure(text, variant):
    """PDO carries bridges; contract/frame tests need no codegen/native fixtures."""
    if variant not in ("pdo", "tools"):
        raise ValueError(f"Unknown archive configuration: {variant}")
    pattern = re.compile(r'^\[\[profile\.ci\.archive\.include\]\]\n.*?(?=^\[|\Z)', re.M | re.S)
    def keep(match):
        block = match[0]
        if variant == "tools" or 'path = "debug/elephc-oniguruma"' in block:
            return ""
        return block
    return pattern.sub(keep, text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("variant", choices=("pdo", "tools"))
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    args.destination.write_text(configure(Path(".config/nextest.toml").read_text(), args.variant))


if __name__ == "__main__":
    main()
