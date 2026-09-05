"""Which hand-written docs/php page documents each PHP module.

Shared by gen_module_sections.py (which writes the generated symbol sections into these
pages) and gen_php_comparison.py (which links each module row to that section).
"""
from __future__ import annotations

# PHP module (as the shared catalog spells it) -> page under docs/php/.
MODULE_PAGES: dict[str, str] = {
    "bcmath": "bcmath.md",
    "cairo": "image.md",
    "calendar": "calendar.md",
    "curl": "curl.md",
    "date": "datetime.md",
    "exif": "image.md",
    "gd": "image.md",
    "gmagick": "image.md",
    "iconv": "iconv.md",
    "imagick": "image.md",
    "mysqli": "mysqli.md",
    "pcre": "regex.md",
    "pdo": "pdo.md",
    "pdo_dblib": "pdo.md",
    "pdo_firebird": "pdo.md",
    "pdo_ibm": "pdo.md",
    "pdo_mysql": "pdo.md",
    "pdo_odbc": "pdo.md",
    "pdo_pgsql": "pdo.md",
    "pdo_sqlite": "pdo.md",
    "session": "sessions.md",
    "spl": "spl.md",
    "zend opcache": "opcache.md",
}

BEGIN_MARKER = "<!-- elephc:generated:symbols:begin -->"
END_MARKER = "<!-- elephc:generated:symbols:end -->"
SECTION_ANCHOR = "functions"
