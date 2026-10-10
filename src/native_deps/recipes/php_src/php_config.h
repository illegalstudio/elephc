/* php_config.h — Elephc's configuration for hosting PHP extensions.
 *
 * php's own configure generates ~2200 lines here, almost all of which describe
 * php's .c files (which library is present, which syscall exists). Elephc
 * compiles only extensions and the engine's data-structure units (hash tables,
 * strings, argument parsing, objects), none of which probe the system, so the
 * headers consult a small fraction of it. This file is that fraction, derived
 * by compiling against an empty config and adding only what the compiler
 * demanded.
 *
 * That it is small is the point: it is the piece Elephc owns, and it is what
 * makes "extensions compiled against headers we control" literal.
 */

#ifndef PHP_CONFIG_H
#define PHP_CONFIG_H

/* zend_string.h calls free() before any Zend header has pulled in stdlib on
 * Unix; php's generated config gets there indirectly through one of its ~2200
 * defines. Since this file is ours, we guarantee it directly instead. */
#include <stdlib.h>
#include <string.h>

/* Symbol visibility. Extensions are compiled into a static archive that Elephc
 * links, so default visibility is what lets the linker see them. */
#if defined(__GNUC__) && __GNUC__ >= 4
# define ZEND_API __attribute__ ((visibility("default")))
# define ZEND_DLEXPORT __attribute__ ((visibility("default")))
#else
# define ZEND_API
# define ZEND_DLEXPORT
#endif
#define ZEND_DLIMPORT

/* Integer widths. zend_long.h and zend_types.h refuse to compile without these,
 * and they decide zval layout, so they are not optional. 64-bit only: Elephc's
 * targets are macos-aarch64, linux-aarch64 and linux-x86_64. */
#define SIZEOF_SIZE_T 8
#define SIZEOF_LONG 8
#define SIZEOF_INT 4
#define SIZEOF_OFF_T 8
#define SIZEOF_ZEND_LONG 8
#define ZEND_ENABLE_ZVAL_LONG64 1

/* Extensions branch on this (APCu does, at php_apc.c:779). Hosted extensions
 * are built in release mode: assertions off, no debug-only surface. */
#define ZEND_DEBUG 0

/* Allocator alignment. zend_alloc.h refuses outright without it, and it is
 * baked into zend_string sizing, so it is part of the ABI rather than a tuning
 * knob. 8 bytes with log2 3, matching every 64-bit target Elephc supports. */
#define ZEND_MM_ALIGNMENT (size_t)8
#define ZEND_MM_ALIGNMENT_LOG2 (size_t)3

/* Standard headers the Zend headers guard their includes on. */
#define HAVE_STDINT_H 1
#define HAVE_INTTYPES_H 1
#define HAVE_STDLIB_H 1
#define HAVE_STRING_H 1
#define HAVE_STRINGS_H 1
#define HAVE_UNISTD_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_SYS_STAT_H 1
/* zend_virtual_cwd.h declares DIR* members unconditionally on this. */
#define HAVE_DIRENT_H 1

/* ext/pcre's header includes PCRE2 from php-src's bundled copy (retained with
 * the headers) instead of a system pcre2.h. APCu reaches it for its iterator. */
#define HAVE_BUNDLED_PCRE 1
#define PCRE2_CODE_UNIT_WIDTH 8

#endif /* PHP_CONFIG_H */
