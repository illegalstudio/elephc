/*
 * Compile php-src's pinned libxml2 selector adapter with the minimal
 * compatibility layer above instead of embedding or modifying its source.
 */

#include "php_compat/selectors_compat.h"
#include "dom/lexbor/selectors-adapted/selectors.c"
