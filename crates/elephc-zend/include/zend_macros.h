/* zend_macros.h — the macros of Elephc's PHP extension ABI.
 *
 * Everything else an extension sees (types, constants, function prototypes)
 * is generated from the elephc-zend crate's Rust source into
 * elephc_zend_abi.h. What remains here is what only the C preprocessor can
 * express: macros an extension expands in its own code (RETURN_STR,
 * ZEND_PARSE_PARAMETERS_START, ZEND_HASH_FOREACH_*, arginfo arrays, ZVAL_*,
 * the module and INI declarations) and the names PHP's headers alias.
 *
 * Names and behaviour follow PHP 8.5's headers, because extension source is
 * written against them; every definition here is Elephc's own.
 */

#ifndef ELEPHC_ZEND_MACROS_H
#define ELEPHC_ZEND_MACROS_H

#include <assert.h>
#include <ctype.h>
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <limits.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <time.h>
#include <unistd.h>
/* ------------------------------------------------------------ versions */

/* ------------------------------------------------------------ language glue */

#ifdef __cplusplus
# define BEGIN_EXTERN_C() extern "C" {
# define END_EXTERN_C() }
#else
# define BEGIN_EXTERN_C()
# define END_EXTERN_C()
#endif
#define ZEND_BEGIN_EXTERN_C BEGIN_EXTERN_C
#define ZEND_END_EXTERN_C END_EXTERN_C
#define ZEND_API
#define ZEND_DLEXPORT
#define ZEND_DLIMPORT
#define PHPAPI
#define PHP_JSON_API
#define ZEND_FASTCALL
#define ZEND_COLD
#define ZEND_HOT
#define ZEND_NORETURN __attribute__((noreturn))
#define ZEND_ATTRIBUTE_UNUSED __attribute__((unused))
#define ZEND_ATTRIBUTE_MALLOC
#define ZEND_ATTRIBUTE_ALLOC_SIZE(x)
#define ZEND_ATTRIBUTE_ALLOC_SIZE2(x, y)
#define ZEND_ATTRIBUTE_NONNULL
#define ZEND_ATTRIBUTE_NONNULL_ARGS(...)
#define ZEND_ATTRIBUTE_FORMAT(type, idx, first) __attribute__((format(type, idx, first)))
#define ZEND_ATTRIBUTE_PTR_FORMAT(type, idx, first)
#define ZEND_ATTRIBUTE_DEPRECATED
#define ZEND_FALLTHROUGH
#define ZEND_INDIRECT_RETURN
#define ZEND_ELEMENT_COUNT(x)
#define ZEND_NONSTRING
#define zend_always_inline inline __attribute__((always_inline))
#define zend_never_inline __attribute__((noinline))
#define EXPECTED(c) __builtin_expect(!!(c), 1)
#define UNEXPECTED(c) __builtin_expect(!!(c), 0)
#define ZEND_ASSERT(c) ((void)0)
#define ZEND_UNREACHABLE() __builtin_unreachable()
#define ZEND_IGNORE_VALUE(x) ((void)(x))
#define ZEND_STRL(str) (str), (sizeof(str) - 1)
#define ZEND_STRS(str) (str), (sizeof(str))
#define ZEND_NORMALIZE_BOOL(n) ((n) > 0 ? 1 : (((n) < 0) ? -1 : 0))
#define ZEND_TRUTH(x) ((x) ? 1 : 0)
#define ZEND_LOG_XOR(a, b) (ZEND_TRUTH(a) ^ ZEND_TRUTH(b))
#define ZEND_MM_ALIGNMENT_MASK ~(ZEND_MM_ALIGNMENT - 1)
#define ZEND_MM_ALIGNED_SIZE(size) (((size) + ZEND_MM_ALIGNMENT - 1) & ~(ZEND_MM_ALIGNMENT - 1))
#define ZEND_MM_ALIGNED_SIZE_EX(size, alignment) (((size) + ((alignment) - 1)) & ~((alignment) - 1))
#define XtOffsetOf(s_type, field) offsetof(s_type, field)
#define ZEND_FILE_LINE_D
#define ZEND_FILE_LINE_DC
#define ZEND_FILE_LINE_ORIG_D
#define ZEND_FILE_LINE_ORIG_DC
#define ZEND_FILE_LINE_RELAY_C
#define ZEND_FILE_LINE_RELAY_CC
#define ZEND_FILE_LINE_C
#define ZEND_FILE_LINE_CC
#define ZEND_FILE_LINE_EMPTY_C
#define ZEND_FILE_LINE_EMPTY_CC
#define ZEND_FILE_LINE_ORIG_RELAY_C
#define ZEND_FILE_LINE_ORIG_RELAY_CC
#define EMPTY_SWITCH_DEFAULT_CASE() default: ZEND_UNREACHABLE(); break;
#define ZEND_TSRMLS_CACHE_DEFINE()
#define ZEND_TSRMLS_CACHE_EXTERN()
#define ZEND_TSRMLS_CACHE_UPDATE()
#define ZEND_TSRMLS_CACHE
#define TSRMLS_D void
#define TSRMLS_DC
#define TSRMLS_C
#define TSRMLS_CC
#define TSRMLS_FETCH()
/* Zeroing the optimizer may not drop (a dead store before free()):
 * explicit_bzero/memset_s are not on every libc this targets (macOS
 * lacks the first, glibc and musl the second), so every byte is
 * written through a volatile pointer, which compilers must keep. */
static inline void elephc_secure_zero(void *var, size_t size) {
    volatile unsigned char *p = (volatile unsigned char *)var;
    while (size--) {
        *p++ = 0;
    }
}
#define ZEND_SECURE_ZERO(var, size) elephc_secure_zero((var), (size))
#define MIN(a, b) (((a) < (b)) ? (a) : (b))
#define MAX(a, b) (((a) > (b)) ? (a) : (b))
#define ZEND_BIT_TEST(bits, bit) (((bits)[(bit) / (sizeof((bits)[0]) * 8)] >> ((bit) & (sizeof((bits)[0]) * 8 - 1))) & 1)
#define ZEND_SLIDE_TO_ALIGNED(base, ptr, alignment) (ptr)
#define ZEND_SIZE_T_MAX SIZE_MAX
#define ZEND_THREEWAY_COMPARE(a, b) ((a) == (b) ? 0 : (((a) < (b)) ? -1 : 1))
#define ZEND_LONG_CAN_OVFL_INT 1
#define ZEND_LONG_INT_OVFL(zlong) ((zlong) > (zend_long)INT_MAX)
#define ZEND_LONG_INT_UDFL(zlong) ((zlong) < (zend_long)INT_MIN)
#define ZEND_SIZE_T_INT_OVFL(size) ((size) > (size_t)INT_MAX)
#define ZEND_SIZE_T_UINT_OVFL(size) ((size) > (size_t)UINT_MAX)
#define zend_finite(a) isfinite(a)
#define zend_isinf(a) isinf(a)
#define zend_isnan(a) isnan(a)
#define ZEND_DOUBLE_FITS_LONG(d) (!((d) >= (double)ZEND_LONG_MAX || (d) < (double)ZEND_LONG_MIN))

/* ------------------------------------------------------------ scalar types */

#define ZEND_LONG_FMT "%" PRId64
#define ZEND_ULONG_FMT "%" PRIu64
#define ZEND_XLONG_FMT "%" PRIx64
#define ZEND_LONG_FMT_SPEC PRId64
#define ZEND_ULONG_FMT_SPEC PRIu64
#define Z_L(i) INT64_C(i)
#define Z_UL(i) UINT64_C(i)
#define ZEND_STRTOL(s0, s1, base) strtoll((s0), (s1), (base))
#define ZEND_STRTOUL(s0, s1, base) strtoull((s0), (s1), (base))
#define ZEND_STRTOL_PTR strtoll
#define ZEND_ABS llabs
#define ZEND_ATOL(s) atoll((s))
#define ZEND_LONG_FMT_LEN 20

/* ------------------------------------------------------------ value layout */

/* ------------------------------------------------------------ type codes */

#define OBJ_FLAGS(obj) GC_FLAGS(obj)
#define GC_REFCOUNT(p) ((p)->gc.refcount)
#define GC_SET_REFCOUNT(p, rc) ((p)->gc.refcount = (rc))
#define GC_ADDREF(p) (++(p)->gc.refcount)
#define GC_DELREF(p) (--(p)->gc.refcount)
#define GC_ADDREF_EX(p, rc) ((p)->gc.refcount += (rc))
#define GC_DELREF_EX(p, rc) ((p)->gc.refcount -= (rc))
#define GC_TRY_ADDREF(p) do { if (!(GC_FLAGS(p) & GC_IMMUTABLE)) { GC_ADDREF(p); } } while (0)
#define GC_TRY_DELREF(p) do { if (!(GC_FLAGS(p) & GC_IMMUTABLE)) { GC_DELREF(p); } } while (0)
#define GC_TYPE_INFO(p) ((p)->gc.u.type_info)
#define GC_TYPE(p) ((zend_uchar)(GC_TYPE_INFO(p) & GC_TYPE_MASK))
#define GC_FLAGS(p) (GC_TYPE_INFO(p) & GC_FLAGS_MASK)
#define GC_INFO(p) (GC_TYPE_INFO(p) >> GC_INFO_SHIFT)
#define GC_ADD_FLAGS(p, flags) do { GC_TYPE_INFO(p) |= (flags); } while (0)
#define GC_DEL_FLAGS(p, flags) do { GC_TYPE_INFO(p) &= ~(flags); } while (0)
#define GC_IS_RECURSIVE(p) (GC_FLAGS(p) & GC_PROTECTED)
#define GC_PROTECT_RECURSION(p) GC_ADD_FLAGS(p, GC_PROTECTED)
#define GC_UNPROTECT_RECURSION(p) GC_DEL_FLAGS(p, GC_PROTECTED)
#define GC_TRY_PROTECT_RECURSION(p) do { if (!(GC_FLAGS(p) & GC_IMMUTABLE)) GC_PROTECT_RECURSION(p); } while (0)
#define GC_TRY_UNPROTECT_RECURSION(p) do { if (!(GC_FLAGS(p) & GC_IMMUTABLE)) GC_UNPROTECT_RECURSION(p); } while (0)
#define GC_MAY_LEAK(p) ((void)0)
#define GC_MAKE_PERSISTENT_LOCAL(p) ((void)0)

/* ------------------------------------------------------------ zval access */

#define Z_TYPE(zval) ((zval).u1.v.type)
#define Z_TYPE_P(zval_p) Z_TYPE(*(zval_p))
#define Z_TYPE_FLAGS(zval) ((zval).u1.v.type_flags)
#define Z_TYPE_FLAGS_P(zval_p) Z_TYPE_FLAGS(*(zval_p))
#define Z_TYPE_EXTRA(zval) ((zval).u1.v.u.extra)
#define Z_TYPE_EXTRA_P(zval_p) Z_TYPE_EXTRA(*(zval_p))
#define Z_TYPE_INFO(zval) ((zval).u1.type_info)
#define Z_TYPE_INFO_P(zval_p) Z_TYPE_INFO(*(zval_p))
#define Z_NEXT(zval) ((zval).u2.next)
#define Z_NEXT_P(zval_p) Z_NEXT(*(zval_p))
#define Z_EXTRA(zval) ((zval).u2.extra)
#define Z_EXTRA_P(zval_p) Z_EXTRA(*(zval_p))
#define Z_COUNTED(zval) ((zval).value.counted)
#define Z_COUNTED_P(zval_p) Z_COUNTED(*(zval_p))
#define Z_REFCOUNTED(zval) (Z_TYPE_FLAGS(zval) != 0)
#define Z_REFCOUNTED_P(zval_p) Z_REFCOUNTED(*(zval_p))
#define Z_COLLECTABLE(zval) ((Z_TYPE_FLAGS(zval) & IS_TYPE_COLLECTABLE) != 0)
#define Z_COLLECTABLE_P(zval_p) Z_COLLECTABLE(*(zval_p))
#define Z_COPYABLE(zval) (Z_TYPE(zval) == IS_ARRAY)
#define Z_COPYABLE_P(zval_p) Z_COPYABLE(*(zval_p))
#define Z_IMMUTABLE(zval) (Z_TYPE_INFO(zval) == IS_ARRAY)
#define Z_IMMUTABLE_P(zval_p) Z_IMMUTABLE(*(zval_p))
#define Z_OPT_TYPE(zval) (Z_TYPE_INFO(zval) & Z_TYPE_MASK)
#define Z_OPT_TYPE_P(zval_p) Z_OPT_TYPE(*(zval_p))
#define Z_OPT_REFCOUNTED(zval) Z_REFCOUNTED(zval)
#define Z_OPT_REFCOUNTED_P(zval_p) Z_OPT_REFCOUNTED(*(zval_p))
#define Z_OPT_ISREF(zval) (Z_OPT_TYPE(zval) == IS_REFERENCE)
#define Z_OPT_ISREF_P(zval_p) Z_OPT_ISREF(*(zval_p))
#define Z_ISREF(zval) (Z_TYPE(zval) == IS_REFERENCE)
#define Z_ISREF_P(zval_p) Z_ISREF(*(zval_p))
#define Z_ISUNDEF(zval) (Z_TYPE(zval) == IS_UNDEF)
#define Z_ISUNDEF_P(zval_p) Z_ISUNDEF(*(zval_p))
#define Z_ISNULL(zval) (Z_TYPE(zval) == IS_NULL)
#define Z_ISNULL_P(zval_p) Z_ISNULL(*(zval_p))
#define Z_ISERROR(zval) (Z_TYPE(zval) == _IS_ERROR)
#define Z_ISERROR_P(zval_p) Z_ISERROR(*(zval_p))
#define Z_LVAL(zval) (zval).value.lval
#define Z_LVAL_P(zval_p) Z_LVAL(*(zval_p))
#define Z_DVAL(zval) (zval).value.dval
#define Z_DVAL_P(zval_p) Z_DVAL(*(zval_p))
#define Z_STR(zval) (zval).value.str
#define Z_STR_P(zval_p) Z_STR(*(zval_p))
#define Z_STRVAL(zval) ZSTR_VAL(Z_STR(zval))
#define Z_STRVAL_P(zval_p) Z_STRVAL(*(zval_p))
#define Z_STRLEN(zval) ZSTR_LEN(Z_STR(zval))
#define Z_STRLEN_P(zval_p) Z_STRLEN(*(zval_p))
#define Z_STRHASH(zval) ZSTR_HASH(Z_STR(zval))
#define Z_STRHASH_P(zval_p) Z_STRHASH(*(zval_p))
#define Z_ARR(zval) (zval).value.arr
#define Z_ARR_P(zval_p) Z_ARR(*(zval_p))
#define Z_ARRVAL(zval) Z_ARR(zval)
#define Z_ARRVAL_P(zval_p) Z_ARRVAL(*(zval_p))
#define Z_OBJ(zval) (zval).value.obj
#define Z_OBJ_P(zval_p) Z_OBJ(*(zval_p))
#define Z_OBJ_HT(zval) Z_OBJ(zval)->handlers
#define Z_OBJ_HT_P(zval_p) Z_OBJ_HT(*(zval_p))
#define Z_OBJ_HANDLER(zval, hf) Z_OBJ_HT((zval))->hf
#define Z_OBJ_HANDLER_P(zv_p, hf) Z_OBJ_HANDLER(*(zv_p), hf)
#define Z_OBJ_HANDLE(zval) (Z_OBJ((zval)))->handle
#define Z_OBJ_HANDLE_P(zval_p) Z_OBJ_HANDLE(*(zval_p))
#define Z_OBJCE(zval) (Z_OBJ(zval)->ce)
#define Z_OBJCE_P(zval_p) Z_OBJCE(*(zval_p))
#define Z_OBJPROP(zval) Z_OBJ_HT((zval))->get_properties(Z_OBJ(zval))
#define Z_OBJPROP_P(zval_p) Z_OBJPROP(*(zval_p))
#define Z_RES(zval) (zval).value.res
#define Z_RES_P(zval_p) Z_RES(*(zval_p))
#define Z_RES_HANDLE(zval) Z_RES(zval)->handle
#define Z_RES_HANDLE_P(zval_p) Z_RES_HANDLE(*(zval_p))
#define Z_RES_TYPE(zval) Z_RES(zval)->type
#define Z_RES_TYPE_P(zval_p) Z_RES_TYPE(*(zval_p))
#define Z_RES_VAL(zval) Z_RES(zval)->ptr
#define Z_RES_VAL_P(zval_p) Z_RES_VAL(*(zval_p))
#define Z_REF(zval) (zval).value.ref
#define Z_REF_P(zval_p) Z_REF(*(zval_p))
#define Z_REFVAL(zval) &Z_REF(zval)->val
#define Z_REFVAL_P(zval_p) Z_REFVAL(*(zval_p))
#define Z_AST(zval) (zval).value.ast
#define Z_AST_P(zval_p) Z_AST(*(zval_p))
#define Z_INDIRECT(zval) (zval).value.zv
#define Z_INDIRECT_P(zval_p) Z_INDIRECT(*(zval_p))
#define Z_CE(zval) (zval).value.ce
#define Z_CE_P(zval_p) Z_CE(*(zval_p))
#define Z_FUNC(zval) (zval).value.func
#define Z_FUNC_P(zval_p) Z_FUNC(*(zval_p))
#define Z_PTR(zval) (zval).value.ptr
#define Z_PTR_P(zval_p) Z_PTR(*(zval_p))
#define Z_REFCOUNT(z) Z_REFCOUNT_P(&(z))
#define Z_REFCOUNT_P(pz) GC_REFCOUNT(Z_COUNTED_P(pz))
#define Z_SET_REFCOUNT(z, rc) Z_SET_REFCOUNT_P(&(z), rc)
#define Z_SET_REFCOUNT_P(pz, rc) GC_SET_REFCOUNT(Z_COUNTED_P(pz), rc)
#define Z_ADDREF(z) Z_ADDREF_P(&(z))
#define Z_ADDREF_P(pz) GC_ADDREF(Z_COUNTED_P(pz))
#define Z_DELREF(z) Z_DELREF_P(&(z))
#define Z_DELREF_P(pz) GC_DELREF(Z_COUNTED_P(pz))
#define Z_TRY_ADDREF(z) Z_TRY_ADDREF_P(&(z))
#define Z_TRY_ADDREF_P(pz) do { if (Z_REFCOUNTED_P((pz))) { Z_ADDREF_P((pz)); } } while (0)
#define Z_TRY_DELREF(z) Z_TRY_DELREF_P(&(z))
#define Z_TRY_DELREF_P(pz) do { if (Z_REFCOUNTED_P((pz))) { Z_DELREF_P((pz)); } } while (0)
#define Z_PROTECT_RECURSION(zval) Z_PROTECT_RECURSION_P(&(zval))
#define Z_PROTECT_RECURSION_P(zv) GC_PROTECT_RECURSION(Z_COUNTED_P(zv))
#define Z_UNPROTECT_RECURSION(zval) Z_UNPROTECT_RECURSION_P(&(zval))
#define Z_UNPROTECT_RECURSION_P(zv) GC_UNPROTECT_RECURSION(Z_COUNTED_P(zv))
#define Z_IS_RECURSIVE(zval) Z_IS_RECURSIVE_P(&(zval))
#define Z_IS_RECURSIVE_P(zv) GC_IS_RECURSIVE(Z_COUNTED_P(zv))

/* ------------------------------------------------------------ zval writes */

#define ZVAL_UNDEF(z) do { Z_TYPE_INFO_P(z) = IS_UNDEF; } while (0)
#define ZVAL_NULL(z) do { Z_TYPE_INFO_P(z) = IS_NULL; } while (0)
#define ZVAL_FALSE(z) do { Z_TYPE_INFO_P(z) = IS_FALSE; } while (0)
#define ZVAL_TRUE(z) do { Z_TYPE_INFO_P(z) = IS_TRUE; } while (0)
#define ZVAL_BOOL(z, b) do { Z_TYPE_INFO_P(z) = (b) ? IS_TRUE : IS_FALSE; } while (0)
#define ZVAL_LONG(z, l) do { zval *__z = (z); Z_LVAL_P(__z) = (l); Z_TYPE_INFO_P(__z) = IS_LONG; } while (0)
#define ZVAL_DOUBLE(z, d) do { zval *__z = (z); Z_DVAL_P(__z) = (d); Z_TYPE_INFO_P(__z) = IS_DOUBLE; } while (0)
#define ZVAL_ERROR(z) do { Z_TYPE_INFO_P(z) = _IS_ERROR; } while (0)
#define ZVAL_STR(z, s) do { zval *__z = (z); zend_string *__s = (s); Z_STR_P(__z) = __s; \
        Z_TYPE_INFO_P(__z) = ZSTR_IS_INTERNED(__s) ? IS_INTERNED_STRING_EX : IS_STRING_EX; } while (0)
#define ZVAL_INTERNED_STR(z, s) do { zval *__z = (z); Z_STR_P(__z) = (s); Z_TYPE_INFO_P(__z) = IS_INTERNED_STRING_EX; } while (0)
#define ZVAL_NEW_STR(z, s) do { zval *__z = (z); Z_STR_P(__z) = (s); Z_TYPE_INFO_P(__z) = IS_STRING_EX; } while (0)
#define ZVAL_STR_COPY(z, s) do { zval *__z = (z); zend_string *__s = (s); Z_STR_P(__z) = __s; \
        if (ZSTR_IS_INTERNED(__s)) { Z_TYPE_INFO_P(__z) = IS_INTERNED_STRING_EX; } \
        else { GC_ADDREF(__s); Z_TYPE_INFO_P(__z) = IS_STRING_EX; } } while (0)
#define ZVAL_ARR(z, a) do { zval *__z = (z); Z_ARR_P(__z) = (a); Z_TYPE_INFO_P(__z) = IS_ARRAY_EX; } while (0)
#define ZVAL_NEW_ARR(z) do { zval *__z = (z); Z_ARR_P(__z) = zend_new_array(0); Z_TYPE_INFO_P(__z) = IS_ARRAY_EX; } while (0)
#define ZVAL_NEW_PERSISTENT_ARR(z) ZVAL_NEW_ARR(z)
#define ZVAL_OBJ(z, o) do { zval *__z = (z); Z_OBJ_P(__z) = (o); Z_TYPE_INFO_P(__z) = IS_OBJECT_EX; } while (0)
#define ZVAL_OBJ_COPY(z, o) do { zval *__z = (z); zend_object *__o = (o); GC_ADDREF(__o); Z_OBJ_P(__z) = __o; Z_TYPE_INFO_P(__z) = IS_OBJECT_EX; } while (0)
#define ZVAL_RES(z, r) do { zval *__z = (z); Z_RES_P(__z) = (r); Z_TYPE_INFO_P(__z) = IS_RESOURCE_EX; } while (0)
#define ZVAL_REF(z, r) do { zval *__z = (z); Z_REF_P(__z) = (r); Z_TYPE_INFO_P(__z) = IS_REFERENCE_EX; } while (0)
#define ZVAL_NEW_EMPTY_REF(z) do { zend_reference *_ref = zend_elephc_new_reference(NULL); ZVAL_UNDEF(&_ref->val); ZVAL_REF(z, _ref); } while (0)
#define ZVAL_NEW_REF(z, r) do { zend_reference *_ref = zend_elephc_new_reference(r); ZVAL_REF(z, _ref); } while (0)
#define ZVAL_MAKE_REF_EX(z, refcount) do { zval *_z = (z); zend_reference *_ref = zend_elephc_new_reference(_z); \
        GC_SET_REFCOUNT(_ref, (refcount)); ZVAL_REF(_z, _ref); } while (0)
#define ZVAL_MAKE_REF(zv) do { zval *__zv = (zv); if (!Z_ISREF_P(__zv)) { ZVAL_NEW_REF(__zv, __zv); } } while (0)
#define ZVAL_PTR(z, p) do { Z_PTR_P(z) = (p); Z_TYPE_INFO_P(z) = IS_PTR; } while (0)
#define ZVAL_INDIRECT(z, v) do { Z_INDIRECT_P(z) = (v); Z_TYPE_INFO_P(z) = IS_INDIRECT; } while (0)
#define ZVAL_CE(z, c) do { Z_CE_P(z) = (c); Z_TYPE_INFO_P(z) = IS_PTR; } while (0)
#define ZVAL_FUNC(z, f) do { Z_FUNC_P(z) = (f); Z_TYPE_INFO_P(z) = IS_PTR; } while (0)
#define ZVAL_ALIAS_PTR(z, p) do { Z_PTR_P(z) = (p); Z_TYPE_INFO_P(z) = IS_ALIAS_PTR; } while (0)
#define ZVAL_EMPTY_STRING(z) ZVAL_INTERNED_STR(z, ZSTR_EMPTY_ALLOC())
#define ZVAL_EMPTY_ARRAY(z) ZVAL_ARR(z, zend_new_array(0))
#define ZVAL_STRINGL(z, s, l) do { ZVAL_NEW_STR(z, zend_string_init(s, l, 0)); } while (0)
#define ZVAL_STRING(z, s) do { const char *_s = (s); ZVAL_STRINGL(z, _s, strlen(_s)); } while (0)
#define ZVAL_STRINGL_FAST(z, s, l) ZVAL_STRINGL(z, s, l)
#define ZVAL_STRING_FAST(z, s) ZVAL_STRING(z, s)
#define ZVAL_PSTRINGL(z, s, l) do { ZVAL_NEW_STR(z, zend_string_init(s, l, 1)); } while (0)
#define ZVAL_PSTRING(z, s) do { const char *_s = (s); ZVAL_PSTRINGL(z, _s, strlen(_s)); } while (0)
#define ZVAL_CHAR(z, c) ZVAL_INTERNED_STR(z, ZSTR_CHAR((zend_uchar)(c)))
#define ZVAL_COPY_VALUE(z, v) do { zval *_z1 = (z); const zval *_z2 = (v); \
        _z1->value = _z2->value; Z_TYPE_INFO_P(_z1) = Z_TYPE_INFO_P(_z2); } while (0)
#define ZVAL_COPY_VALUE_EX(z, v, gc, t) ZVAL_COPY_VALUE(z, v)
#define ZVAL_COPY(z, v) do { zval *_zc_dst = (z); const zval *_zc_src = (v); ZVAL_COPY_VALUE(_zc_dst, _zc_src); \
        if (Z_REFCOUNTED_P(_zc_dst)) { GC_ADDREF(Z_COUNTED_P(_zc_dst)); } } while (0)
#define ZVAL_DUP(z, v) zend_elephc_zval_dup((z), (v))
#define ZVAL_COPY_OR_DUP(z, v) ZVAL_COPY(z, v)
#define ZVAL_COPY_VALUE_PROP(z, v) ZVAL_COPY_VALUE(z, v)
#define ZVAL_COPY_PROP(z, v) ZVAL_COPY(z, v)
#define ZVAL_COPY_OR_DUP_PROP(z, v) ZVAL_COPY(z, v)
#define ZVAL_DEREF(z) do { if (UNEXPECTED(Z_ISREF_P(z))) { (z) = Z_REFVAL_P(z); } } while (0)
#define ZVAL_DEINDIRECT(z) do { if (Z_TYPE_P(z) == IS_INDIRECT) { (z) = Z_INDIRECT_P(z); } } while (0)
#define ZVAL_OPT_DEREF(z) ZVAL_DEREF(z)
#define ZVAL_MAKE_REF_EX_INIT(z) ZVAL_MAKE_REF(z)
#define ZVAL_COPY_DEREF(z, v) do { zval *_z3 = (v); if (Z_OPT_REFCOUNTED_P(_z3)) { \
        if (UNEXPECTED(Z_OPT_ISREF_P(_z3))) { _z3 = Z_REFVAL_P(_z3); if (Z_OPT_REFCOUNTED_P(_z3)) { Z_ADDREF_P(_z3); } } \
        else { Z_ADDREF_P(_z3); } } ZVAL_COPY_VALUE(z, _z3); } while (0)
#define ZVAL_UNREF(z) do { zval *_z = (z); zend_reference *_ref = Z_REF_P(_z); ZVAL_COPY_VALUE(_z, &_ref->val); \
        if (GC_DELREF(_ref) == 0) { efree(_ref); } else if (Z_REFCOUNTED_P(_z)) { Z_ADDREF_P(_z); } } while (0)
#define ZVAL_COPY_UNREF(z, v) ZVAL_COPY_DEREF(z, v)
#define SEPARATE_ARRAY(zv) do { zval *__zv = (zv); zend_array *_arr = Z_ARR_P(__zv); \
        if (UNEXPECTED(GC_REFCOUNT(_arr) > 1)) { ZVAL_ARR(__zv, zend_array_dup(_arr)); GC_TRY_DELREF(_arr); } } while (0)
#define SEPARATE_ZVAL_IF_NOT_REF(zv) do { zval *_zv = (zv); if (Z_TYPE_P(_zv) == IS_ARRAY) { SEPARATE_ARRAY(_zv); } } while (0)
#define SEPARATE_ZVAL_NOREF(zv) SEPARATE_ZVAL_IF_NOT_REF(zv)
#define SEPARATE_ZVAL(zv) do { zval *_szv = (zv); if (Z_ISREF_P(_szv)) { zend_reference *_r = Z_REF_P(_szv); \
        ZVAL_COPY_VALUE(_szv, &_r->val); if (GC_DELREF(_r) == 0) { efree(_r); } else if (Z_REFCOUNTED_P(_szv)) { Z_ADDREF_P(_szv); } } \
        SEPARATE_ZVAL_IF_NOT_REF(_szv); } while (0)

/* ------------------------------------------------------------ return values */

#define RETVAL_BOOL(b) ZVAL_BOOL(return_value, b)
#define RETVAL_NULL() ZVAL_NULL(return_value)
#define RETVAL_LONG(l) ZVAL_LONG(return_value, l)
#define RETVAL_DOUBLE(d) ZVAL_DOUBLE(return_value, d)
#define RETVAL_STR(s) ZVAL_STR(return_value, s)
#define RETVAL_INTERNED_STR(s) ZVAL_INTERNED_STR(return_value, s)
#define RETVAL_NEW_STR(s) ZVAL_NEW_STR(return_value, s)
#define RETVAL_STR_COPY(s) ZVAL_STR_COPY(return_value, s)
#define RETVAL_STRING(s) ZVAL_STRING(return_value, s)
#define RETVAL_STRINGL(s, l) ZVAL_STRINGL(return_value, s, l)
#define RETVAL_STRING_FAST(s) ZVAL_STRING(return_value, s)
#define RETVAL_STRINGL_FAST(s, l) ZVAL_STRINGL(return_value, s, l)
#define RETVAL_EMPTY_STRING() ZVAL_EMPTY_STRING(return_value)
#define RETVAL_CHAR(c) ZVAL_CHAR(return_value, c)
#define RETVAL_RES(r) ZVAL_RES(return_value, r)
#define RETVAL_ARR(r) ZVAL_ARR(return_value, r)
#define RETVAL_EMPTY_ARRAY() ZVAL_EMPTY_ARRAY(return_value)
#define RETVAL_OBJ(r) ZVAL_OBJ(return_value, r)
#define RETVAL_OBJ_COPY(r) ZVAL_OBJ_COPY(return_value, r)
#define RETVAL_COPY(zv) ZVAL_COPY(return_value, zv)
#define RETVAL_COPY_VALUE(zv) ZVAL_COPY_VALUE(return_value, zv)
#define RETVAL_COPY_DEREF(zv) ZVAL_COPY_DEREF(return_value, zv)
#define RETVAL_ZVAL(zv, copy, dtor) ZVAL_ZVAL(return_value, zv, copy, dtor)
#define RETVAL_FALSE ZVAL_FALSE(return_value)
#define RETVAL_TRUE ZVAL_TRUE(return_value)
#define RETURN_BOOL(b) do { RETVAL_BOOL(b); return; } while (0)
#define RETURN_NULL() do { RETVAL_NULL(); return; } while (0)
#define RETURN_LONG(l) do { RETVAL_LONG(l); return; } while (0)
#define RETURN_DOUBLE(d) do { RETVAL_DOUBLE(d); return; } while (0)
#define RETURN_STR(s) do { RETVAL_STR(s); return; } while (0)
#define RETURN_INTERNED_STR(s) do { RETVAL_INTERNED_STR(s); return; } while (0)
#define RETURN_NEW_STR(s) do { RETVAL_NEW_STR(s); return; } while (0)
#define RETURN_STR_COPY(s) do { RETVAL_STR_COPY(s); return; } while (0)
#define RETURN_STRING(s) do { RETVAL_STRING(s); return; } while (0)
#define RETURN_STRINGL(s, l) do { RETVAL_STRINGL(s, l); return; } while (0)
#define RETURN_STRING_FAST(s) do { RETVAL_STRING_FAST(s); return; } while (0)
#define RETURN_STRINGL_FAST(s, l) do { RETVAL_STRINGL_FAST(s, l); return; } while (0)
#define RETURN_EMPTY_STRING() do { RETVAL_EMPTY_STRING(); return; } while (0)
#define RETURN_CHAR(c) do { RETVAL_CHAR(c); return; } while (0)
#define RETURN_RES(r) do { RETVAL_RES(r); return; } while (0)
#define RETURN_ARR(r) do { RETVAL_ARR(r); return; } while (0)
#define RETURN_EMPTY_ARRAY() do { RETVAL_EMPTY_ARRAY(); return; } while (0)
#define RETURN_OBJ(r) do { RETVAL_OBJ(r); return; } while (0)
#define RETURN_OBJ_COPY(r) do { RETVAL_OBJ_COPY(r); return; } while (0)
#define RETURN_COPY(zv) do { RETVAL_COPY(zv); return; } while (0)
#define RETURN_COPY_VALUE(zv) do { RETVAL_COPY_VALUE(zv); return; } while (0)
#define RETURN_COPY_DEREF(zv) do { RETVAL_COPY_DEREF(zv); return; } while (0)
#define RETURN_ZVAL(zv, copy, dtor) do { RETVAL_ZVAL(zv, copy, dtor); return; } while (0)
#define RETURN_FALSE do { RETVAL_FALSE; return; } while (0)
#define RETURN_TRUE do { RETVAL_TRUE; return; } while (0)
#define RETURN_THROWS() do { ZEND_ASSERT(EG(exception)); (void)return_value; return; } while (0)
#define ZVAL_ZVAL(z, zv, copy, dtor) do { zval *__z = (z); zval *__zv = (zv); \
        if (EXPECTED(!Z_ISREF_P(__zv))) { if ((copy) && !(dtor)) { ZVAL_COPY(__z, __zv); } else { ZVAL_COPY_VALUE(__z, __zv); } } \
        else { ZVAL_COPY(__z, Z_REFVAL_P(__zv)); if ((dtor) || !(copy)) { zval_ptr_dtor(__zv); } } } while (0)

/* ------------------------------------------------------------ memory */

#define ZEND_ALLOCATOR(size) emalloc(size)
#define emalloc_rel(size) emalloc(size)
#define erealloc_rel(ptr, size) erealloc(ptr, size)
#define erealloc_recoverable(ptr, size) erealloc(ptr, size)
#define erealloc2(ptr, size, copy) erealloc(ptr, size)
#define efree_rel(ptr) efree(ptr)
#define efree_large(ptr) efree(ptr)
#define efree_huge(ptr) efree(ptr)
#define ecalloc_rel(n, s) ecalloc(n, s)
#define estrdup_rel(s) estrdup(s)
#define estrndup_rel(s, l) estrndup(s, l)
#define safe_erealloc_rel(p, n, s, o) safe_erealloc(p, n, s, o)
#define safe_emalloc_rel(n, s, o) safe_emalloc(n, s, o)
#define pemalloc(size, persistent) emalloc(size)
#define pecalloc(nmemb, size, persistent) ecalloc(nmemb, size)
#define perealloc(ptr, size, persistent) erealloc(ptr, size)
#define perealloc2(ptr, size, copy, persistent) erealloc(ptr, size)
#define perealloc_recoverable(ptr, size, persistent) erealloc(ptr, size)
#define pefree(ptr, persistent) efree(ptr)
#define pefree_size(ptr, size, persistent) efree(ptr)
#define pestrdup(s, persistent) estrdup(s)
#define pestrndup(s, length, persistent) estrndup(s, length)
#define safe_pemalloc(nmemb, size, offset, persistent) safe_emalloc(nmemb, size, offset)
#define safe_perealloc(ptr, nmemb, size, offset, persistent) safe_erealloc(ptr, nmemb, size, offset)
#define pemalloc_rel(size, persistent) emalloc(size)
#define pefree_rel(ptr, persistent) efree(ptr)
#define emalloc_8(s) emalloc(8)
#define emalloc_16(s) emalloc(16)
#define emalloc_24(s) emalloc(24)
#define emalloc_32(s) emalloc(32)
#define zend_mm_set_custom_handlers(...) ((void)0)

/* ------------------------------------------------------------ strings */

#define ZSTR_VAL(zstr) (zstr)->val
#define ZSTR_LEN(zstr) (zstr)->len
#define ZSTR_H(zstr) (zstr)->h
#define ZSTR_HASH(zstr) zend_string_hash_val(zstr)
#define ZSTR_IS_INTERNED(s) (GC_FLAGS(s) & IS_STR_INTERNED)
#define ZSTR_IS_VALID_UTF8(s) (GC_FLAGS(s) & IS_STR_VALID_UTF8)
#define ZSTR_EMPTY_ALLOC() zend_empty_string
#define ZSTR_CHAR(c) zend_one_char_string[c]
#define ZSTR_KNOWN(idx) zend_known_strings[idx]
#define _ZSTR_HEADER_SIZE offsetof(zend_string, val)
#define _ZSTR_STRUCT_SIZE(len) (_ZSTR_HEADER_SIZE + (len) + 1)
#define ZSTR_MAX_OVERHEAD (ZEND_MM_ALIGNED_SIZE(_ZSTR_HEADER_SIZE + 1))
#define ZSTR_MAX_LEN (SIZE_MAX - ZSTR_MAX_OVERHEAD)
#define ZSTR_ALLOCA_ALLOC(str, _len, use_heap) do { (str) = zend_string_alloc((_len), 0); (void)(use_heap); } while (0)
#define ZSTR_ALLOCA_INIT(str, s, len, use_heap) do { (str) = zend_string_init((s), (len), 0); (void)(use_heap); } while (0)
#define ZSTR_ALLOCA_FREE(str, use_heap) zend_string_free(str)
#define ZSTR_COPY_CONCAT_PROPERTIES(out, in) ((void)0)
#define ZSTR_SET_VALID_UTF8(s) GC_ADD_FLAGS(s, IS_STR_VALID_UTF8)
#define STR_EMPTY_ALLOC() ZSTR_EMPTY_ALLOC()
#define ZSTR_INIT_LITERAL(s, persistent) zend_string_init((s), sizeof(s) - 1, (persistent))
#define zend_string_equals_literal(str, literal) zend_string_equals_cstr(str, literal, sizeof(literal) - 1)
#define zend_string_equals_literal_ci(str, c) zend_string_equals_cstr_ci(str, c, sizeof(c) - 1)
#define zend_string_starts_with_literal(str, prefix) zend_string_starts_with_cstr(str, prefix, sizeof(prefix) - 1)
#define zend_string_starts_with_literal_ci(str, prefix) zend_string_starts_with_cstr_ci(str, prefix, sizeof(prefix) - 1)
#define zend_string_equal_content(s1, s2) zend_string_equals(s1, s2)
#define zend_string_tolower(str) zend_string_tolower_ex(str, 0)
#define zend_string_toupper(str) zend_string_toupper_ex(str, 0)

/* ------------------------------------------------------------ hash tables */

#define HT_FLAGS(ht) (ht)->u.flags
#define HT_INVALIDATE(ht) ((void)0)
#define HT_IS_INITIALIZED(ht) ((HT_FLAGS(ht) & HASH_FLAG_UNINITIALIZED) == 0)
#define HT_IS_PACKED(ht) 0
#define HT_HASH_EX(data, idx) ((uint32_t *)(data))[(int32_t)(idx)]
#define HT_HASH(ht, idx) HT_HASH_EX((ht)->arHash, idx)
#define HT_SIZE_TO_MASK(nSize) ((uint32_t)(-((nSize) + (nSize))))
#define HT_HASH_SIZE(nTableMask) (((size_t)-(uint32_t)(nTableMask)) * sizeof(uint32_t))
#define HT_DATA_SIZE(nTableSize) ((size_t)(nTableSize) * sizeof(Bucket))
#define HT_SIZE_EX(nTableSize, nTableMask) (HT_DATA_SIZE((nTableSize)) + HT_HASH_SIZE((nTableMask)))
#define HT_SIZE(ht) HT_SIZE_EX((ht)->nTableSize, (ht)->nTableMask)
#define HT_USED_SIZE(ht) (HT_HASH_SIZE((ht)->nTableMask) + ((size_t)(ht)->nNumUsed * sizeof(Bucket)))
#define HT_PACKED_DATA_SIZE(nTableSize) ((size_t)(nTableSize) * sizeof(zval))
#define HT_PACKED_SIZE_EX(nTableSize, nTableMask) (HT_PACKED_DATA_SIZE((nTableSize)) + HT_HASH_SIZE((nTableMask)))
#define HT_PACKED_SIZE(ht) HT_PACKED_SIZE_EX((ht)->nTableSize, (ht)->nTableMask)
#define HT_PACKED_USED_SIZE(ht) (HT_HASH_SIZE((ht)->nTableMask) + ((size_t)(ht)->nNumUsed * sizeof(zval)))
#define HT_HASH_RESET(ht) memset(&HT_HASH(ht, (ht)->nTableMask), HT_INVALID_IDX, HT_HASH_SIZE((ht)->nTableMask))
#define HT_HASH_RESET_PACKED(ht) do { HT_HASH(ht, -2) = HT_INVALID_IDX; HT_HASH(ht, -1) = HT_INVALID_IDX; } while (0)
#define HT_HASH_TO_BUCKET_EX(data, idx) ((data) + (idx))
#define HT_HASH_TO_BUCKET(ht, idx) HT_HASH_TO_BUCKET_EX((ht)->arData, idx)
#define HT_IDX_TO_HASH(idx) (idx)
#define HT_HASH_TO_IDX(idx) (idx)
#define HT_SET_DATA_ADDR(ht, ptr) do { (ht)->arData = (Bucket *)(((char *)(ptr)) + HT_HASH_SIZE((ht)->nTableMask)); } while (0)
#define HT_GET_DATA_ADDR(ht) ((char *)((ht)->arData) - HT_HASH_SIZE((ht)->nTableMask))
#define ALLOC_HASHTABLE(ht) (ht) = (HashTable *)emalloc(sizeof(HashTable))
#define FREE_HASHTABLE(ht) efree(ht)
#define ALLOC_HASHTABLE_REL(ht) ALLOC_HASHTABLE(ht)
#define FREE_HASHTABLE_REL(ht) FREE_HASHTABLE(ht)
#define HT_IS_WITHOUT_HOLES(ht) ((ht)->nNumUsed == (ht)->nNumOfElements)
#define HT_HAS_STATIC_KEYS_ONLY(ht) 0
#define HT_HAS_ITERATORS(ht) 0
#define HT_ALLOW_COW_VIOLATION(ht) ((void)0)
#define zend_hash_num_elements(ht) (ht)->nNumOfElements
#define zend_hash_next_free_element(ht) (ht)->nNextFreeElement
#define ZEND_HASH_APPLY_PROTECTION(ht) 0
#define zend_hash_init(ht, nSize, pHashFunction, pDestructor, persistent) _zend_hash_init((ht), (nSize), (pDestructor), (persistent))
#define zend_hash_init_ex(ht, nSize, pHashFunction, pDestructor, persistent, bApplyProtection) _zend_hash_init((ht), (nSize), (pDestructor), (persistent))
#define zend_hash_graceful_destroy(ht) zend_hash_destroy(ht)
#define zend_hash_graceful_reverse_destroy(ht) zend_hash_destroy(ht)
#define zend_hash_symtable_clean(ht) zend_hash_clean(ht)
#define zend_new_array(size) _zend_new_array(size)
#define zend_new_array_0() _zend_new_array_0()
#define ZEND_HANDLE_NUMERIC_STR(key, length, idx) _zend_handle_numeric_str(key, length, &idx)
#define ZEND_HANDLE_NUMERIC(key, idx) ZEND_HANDLE_NUMERIC_STR(ZSTR_VAL(key), ZSTR_LEN(key), idx)
#define zend_hash_has_more_elements_ex(ht, pos) \
    (zend_hash_get_current_key_type_ex(ht, pos) == HASH_KEY_NON_EXISTENT ? FAILURE : SUCCESS)
#define zend_hash_has_more_elements(ht) zend_hash_has_more_elements_ex(ht, &(ht)->nInternalPointer)
#define zend_hash_move_forward(ht) zend_hash_move_forward_ex(ht, &(ht)->nInternalPointer)
#define zend_hash_move_backwards(ht) zend_hash_move_backwards_ex(ht, &(ht)->nInternalPointer)
#define zend_hash_get_current_key(ht, str_index, num_index) zend_hash_get_current_key_ex(ht, str_index, num_index, &(ht)->nInternalPointer)
#define zend_hash_get_current_key_zval(ht, key) zend_hash_get_current_key_zval_ex(ht, key, &(ht)->nInternalPointer)
#define zend_hash_get_current_key_type(ht) zend_hash_get_current_key_type_ex(ht, &(ht)->nInternalPointer)
#define zend_hash_get_current_data(ht) zend_hash_get_current_data_ex(ht, &(ht)->nInternalPointer)
#define zend_hash_internal_pointer_reset(ht) zend_hash_internal_pointer_reset_ex(ht, &(ht)->nInternalPointer)
#define zend_hash_internal_pointer_end(ht) zend_hash_internal_pointer_end_ex(ht, &(ht)->nInternalPointer)
#define zend_hash_get_current_data_ptr(ht) zend_hash_get_current_data_ptr_ex(ht, &(ht)->nInternalPointer)
#define ZEND_HASH_ELEMENT(ht, idx) (&(ht)->arData[idx].val)
#define ZEND_HASH_ELEMENT_EX(ht, idx, size) ZEND_HASH_ELEMENT(ht, idx)
#define ZEND_HASH_ELEMENT_SIZE(ht) sizeof(Bucket)
#define _ZEND_HASH_FOREACH_VAL(ht) do { \
        const HashTable *__ht = (ht); \
        Bucket *_p = __ht->arData; \
        Bucket *_end = _p + __ht->nNumUsed; \
        for (; _p != _end; _p++) { \
            zval *_z = &_p->val; \
            if (UNEXPECTED(Z_TYPE_P(_z) == IS_UNDEF)) continue;
#define _ZEND_HASH_REVERSE_FOREACH_VAL(ht) do { \
        const HashTable *__ht = (ht); \
        uint32_t _idx = __ht->nNumUsed; \
        Bucket *_p = __ht->arData + _idx; \
        while (_idx-- > 0) { \
            zval *_z; \
            _p--; \
            _z = &_p->val; \
            if (UNEXPECTED(Z_TYPE_P(_z) == IS_UNDEF)) continue;
#define _ZEND_HASH_FOREACH_VAL_IND(ht) do { \
        const HashTable *__ht = (ht); \
        Bucket *_p = __ht->arData; \
        Bucket *_end = _p + __ht->nNumUsed; \
        for (; _p != _end; _p++) { \
            zval *_z = &_p->val; \
            if (Z_TYPE_P(_z) == IS_INDIRECT) _z = Z_INDIRECT_P(_z); \
            if (UNEXPECTED(Z_TYPE_P(_z) == IS_UNDEF)) continue;
#define ZEND_HASH_FOREACH_END() } } while (0)
#define ZEND_HASH_FOREACH_END_DEL() ZEND_HASH_FOREACH_END()
#define ZEND_HASH_MAP_FOREACH_END() ZEND_HASH_FOREACH_END()
#define ZEND_HASH_PACKED_FOREACH_END() ZEND_HASH_FOREACH_END()
#define ZEND_HASH_FOREACH_BUCKET(ht, _bucket) _ZEND_HASH_FOREACH_VAL(ht) _bucket = _p;
#define ZEND_HASH_FOREACH_BUCKET_FROM(ht, _bucket, _from) do { \
        const HashTable *__ht = (ht); \
        Bucket *_p = __ht->arData + (_from); \
        Bucket *_end = __ht->arData + __ht->nNumUsed; \
        for (; _p != _end; _p++) { \
            zval *_z = &_p->val; \
            if (UNEXPECTED(Z_TYPE_P(_z) == IS_UNDEF)) continue; \
            _bucket = _p;
#define ZEND_HASH_FOREACH_VAL(ht, _val) _ZEND_HASH_FOREACH_VAL(ht) _val = _z;
#define ZEND_HASH_FOREACH_VAL_IND(ht, _val) _ZEND_HASH_FOREACH_VAL_IND(ht) _val = _z;
#define ZEND_HASH_FOREACH_PTR(ht, _ptr) _ZEND_HASH_FOREACH_VAL(ht) _ptr = Z_PTR_P(_z);
#define ZEND_HASH_FOREACH_PTR_FROM(ht, _ptr, _from) ZEND_HASH_FOREACH_BUCKET_FROM(ht, _p, _from) _ptr = Z_PTR_P(_z);
#define ZEND_HASH_FOREACH_NUM_KEY(ht, _h) _ZEND_HASH_FOREACH_VAL(ht) _h = _p->h;
#define ZEND_HASH_FOREACH_STR_KEY(ht, _key) _ZEND_HASH_FOREACH_VAL(ht) _key = _p->key;
#define ZEND_HASH_FOREACH_KEY(ht, _h, _key) _ZEND_HASH_FOREACH_VAL(ht) _h = _p->h; _key = _p->key;
#define ZEND_HASH_FOREACH_NUM_KEY_VAL(ht, _h, _val) _ZEND_HASH_FOREACH_VAL(ht) _h = _p->h; _val = _z;
#define ZEND_HASH_FOREACH_STR_KEY_VAL(ht, _key, _val) _ZEND_HASH_FOREACH_VAL(ht) _key = _p->key; _val = _z;
#define ZEND_HASH_FOREACH_KEY_VAL(ht, _h, _key, _val) _ZEND_HASH_FOREACH_VAL(ht) _h = _p->h; _key = _p->key; _val = _z;
#define ZEND_HASH_FOREACH_STR_KEY_VAL_IND(ht, _key, _val) _ZEND_HASH_FOREACH_VAL_IND(ht) _key = _p->key; _val = _z;
#define ZEND_HASH_FOREACH_KEY_VAL_IND(ht, _h, _key, _val) _ZEND_HASH_FOREACH_VAL_IND(ht) _h = _p->h; _key = _p->key; _val = _z;
#define ZEND_HASH_FOREACH_NUM_KEY_PTR(ht, _h, _ptr) _ZEND_HASH_FOREACH_VAL(ht) _h = _p->h; _ptr = Z_PTR_P(_z);
#define ZEND_HASH_FOREACH_STR_KEY_PTR(ht, _key, _ptr) _ZEND_HASH_FOREACH_VAL(ht) _key = _p->key; _ptr = Z_PTR_P(_z);
#define ZEND_HASH_FOREACH_KEY_PTR(ht, _h, _key, _ptr) _ZEND_HASH_FOREACH_VAL(ht) _h = _p->h; _key = _p->key; _ptr = Z_PTR_P(_z);
#define ZEND_HASH_REVERSE_FOREACH_VAL(ht, _val) _ZEND_HASH_REVERSE_FOREACH_VAL(ht) _val = _z;
#define ZEND_HASH_REVERSE_FOREACH_PTR(ht, _ptr) _ZEND_HASH_REVERSE_FOREACH_VAL(ht) _ptr = Z_PTR_P(_z);
#define ZEND_HASH_REVERSE_FOREACH_KEY_VAL(ht, _h, _key, _val) _ZEND_HASH_REVERSE_FOREACH_VAL(ht) _h = _p->h; _key = _p->key; _val = _z;
#define ZEND_HASH_REVERSE_FOREACH_STR_KEY_VAL(ht, _key, _val) _ZEND_HASH_REVERSE_FOREACH_VAL(ht) _key = _p->key; _val = _z;
#define ZEND_HASH_REVERSE_FOREACH_BUCKET(ht, _bucket) _ZEND_HASH_REVERSE_FOREACH_VAL(ht) _bucket = _p;
#define ZEND_HASH_MAP_FOREACH_VAL ZEND_HASH_FOREACH_VAL
#define ZEND_HASH_MAP_FOREACH_VAL_IND ZEND_HASH_FOREACH_VAL_IND
#define ZEND_HASH_MAP_FOREACH_PTR ZEND_HASH_FOREACH_PTR
#define ZEND_HASH_MAP_FOREACH_PTR_FROM ZEND_HASH_FOREACH_PTR_FROM
#define ZEND_HASH_MAP_FOREACH_BUCKET ZEND_HASH_FOREACH_BUCKET
#define ZEND_HASH_MAP_FOREACH_BUCKET_FROM ZEND_HASH_FOREACH_BUCKET_FROM
#define ZEND_HASH_MAP_FOREACH_NUM_KEY ZEND_HASH_FOREACH_NUM_KEY
#define ZEND_HASH_MAP_FOREACH_STR_KEY ZEND_HASH_FOREACH_STR_KEY
#define ZEND_HASH_MAP_FOREACH_KEY ZEND_HASH_FOREACH_KEY
#define ZEND_HASH_MAP_FOREACH_NUM_KEY_VAL ZEND_HASH_FOREACH_NUM_KEY_VAL
#define ZEND_HASH_MAP_FOREACH_STR_KEY_VAL ZEND_HASH_FOREACH_STR_KEY_VAL
#define ZEND_HASH_MAP_FOREACH_KEY_VAL ZEND_HASH_FOREACH_KEY_VAL
#define ZEND_HASH_MAP_FOREACH_STR_KEY_VAL_IND ZEND_HASH_FOREACH_STR_KEY_VAL_IND
#define ZEND_HASH_MAP_FOREACH_KEY_VAL_IND ZEND_HASH_FOREACH_KEY_VAL_IND
#define ZEND_HASH_MAP_FOREACH_NUM_KEY_PTR ZEND_HASH_FOREACH_NUM_KEY_PTR
#define ZEND_HASH_MAP_FOREACH_STR_KEY_PTR ZEND_HASH_FOREACH_STR_KEY_PTR
#define ZEND_HASH_MAP_FOREACH_KEY_PTR ZEND_HASH_FOREACH_KEY_PTR
#define ZEND_HASH_MAP_REVERSE_FOREACH_VAL ZEND_HASH_REVERSE_FOREACH_VAL
#define ZEND_HASH_MAP_REVERSE_FOREACH_PTR ZEND_HASH_REVERSE_FOREACH_PTR
#define ZEND_HASH_MAP_REVERSE_FOREACH_KEY_VAL ZEND_HASH_REVERSE_FOREACH_KEY_VAL
#define ZEND_HASH_PACKED_FOREACH_VAL ZEND_HASH_FOREACH_VAL
#define ZEND_HASH_PACKED_FOREACH_PTR ZEND_HASH_FOREACH_PTR
#define ZEND_HASH_PACKED_FOREACH_KEY ZEND_HASH_FOREACH_NUM_KEY
#define ZEND_HASH_PACKED_FOREACH_KEY_VAL ZEND_HASH_FOREACH_NUM_KEY_VAL
#define ZEND_HASH_PACKED_REVERSE_FOREACH_VAL ZEND_HASH_REVERSE_FOREACH_VAL
#define ZEND_HASH_FILL_PACKED(ht) do { HashTable *__fill_ht = (ht);
#define ZEND_HASH_FILL_PACKED_EX(ht) ZEND_HASH_FILL_PACKED(ht)
#define ZEND_HASH_FILL_SET(_val) do { zval __fill_tmp; ZVAL_COPY_VALUE(&__fill_tmp, _val); zend_hash_next_index_insert_new(__fill_ht, &__fill_tmp); } while (0)
#define ZEND_HASH_FILL_SET_NULL() do { zval __fill_tmp; ZVAL_NULL(&__fill_tmp); zend_hash_next_index_insert_new(__fill_ht, &__fill_tmp); } while (0)
#define ZEND_HASH_FILL_SET_LONG(_val) do { zval __fill_tmp; ZVAL_LONG(&__fill_tmp, _val); zend_hash_next_index_insert_new(__fill_ht, &__fill_tmp); } while (0)
#define ZEND_HASH_FILL_SET_DOUBLE(_val) do { zval __fill_tmp; ZVAL_DOUBLE(&__fill_tmp, _val); zend_hash_next_index_insert_new(__fill_ht, &__fill_tmp); } while (0)
#define ZEND_HASH_FILL_SET_STR(_val) do { zval __fill_tmp; ZVAL_STR(&__fill_tmp, _val); zend_hash_next_index_insert_new(__fill_ht, &__fill_tmp); } while (0)
#define ZEND_HASH_FILL_SET_STR_COPY(_val) do { zval __fill_tmp; ZVAL_STR_COPY(&__fill_tmp, _val); zend_hash_next_index_insert_new(__fill_ht, &__fill_tmp); } while (0)
#define ZEND_HASH_FILL_SET_INTERNED_STR(_val) ZEND_HASH_FILL_SET_STR(_val)
#define ZEND_HASH_FILL_ADD(_val) ZEND_HASH_FILL_SET(_val)
#define ZEND_HASH_FILL_NEXT() ((void)0)
#define ZEND_HASH_FILL_GROW() ((void)0)
#define ZEND_HASH_FILL_FINISH() ((void)0)
#define ZEND_HASH_FILL_END() } while (0)

/* ------------------------------------------------------------ references, dtors */

/* As php-src's inline functions: an array is duplicated, any other
 * refcounted value gains a reference (the _opt form reads the type
 * through the Z_OPT_* macros, as php-src does). */
#define zval_copy_ctor(zv) do { zval *__cc = (zv); \
        if (Z_TYPE_P(__cc) == IS_ARRAY) { ZVAL_ARR(__cc, zend_array_dup(Z_ARR_P(__cc))); } \
        else if (Z_REFCOUNTED_P(__cc)) { Z_ADDREF_P(__cc); } } while (0)
#define zval_opt_copy_ctor(zv) do { zval *__cc = (zv); \
        if (Z_OPT_TYPE_P(__cc) == IS_ARRAY) { ZVAL_ARR(__cc, zend_array_dup(Z_ARR_P(__cc))); } \
        else if (Z_OPT_REFCOUNTED_P(__cc)) { Z_ADDREF_P(__cc); } } while (0)
#define zval_internal_dtor(zv) zval_ptr_dtor(zv)
#define zval_ptr_dtor_nogc(zv) zval_ptr_dtor(zv)
#define i_zval_ptr_dtor(zv) zval_ptr_dtor(zv)
#define zval_get_type(zv) Z_TYPE_P(zv)
#define ZVAL_PTR_DTOR zval_ptr_dtor
#define ZVAL_INTERNAL_PTR_DTOR zval_internal_ptr_dtor
#define ZEND_REF_HAS_TYPE_SOURCES(ref) 0
#define ZEND_REF_FIRST_SOURCE(ref) NULL
#define ZEND_REF_ADD_TYPE_SOURCE(ref, source) ((void)0)
#define ZEND_REF_DEL_TYPE_SOURCE(ref, source) ((void)0)
#define ZEND_REF_FOREACH_TYPE_SOURCES(ref, prop) do { (void)(ref); if (0) { prop = NULL;
#define ZEND_REF_FOREACH_TYPE_SOURCES_END() } } while (0)
#define gc_check_possible_root(ref) ((void)0)
#define GC_REMOVE_FROM_BUFFER(p) ((void)0)
#define gc_possible_root(ref) ((void)0)
#define gc_remove_from_buffer(ref) ((void)0)
#define zend_gc_collect_cycles() 0
#define _ZEND_TRY_ASSIGN(zv, is_ref, assign) do { \
        zval *_zv = (zv); \
        if ((is_ref) || UNEXPECTED(Z_ISREF_P(_zv))) { _zv = Z_REFVAL_P(_zv); } \
        zval_ptr_dtor(_zv); \
        assign; \
    } while (0)
#define ZEND_TRY_ASSIGN_NULL(zv) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_NULL(_zv))
#define ZEND_TRY_ASSIGN_REF_NULL(zv) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_NULL(_zv))
#define ZEND_TRY_ASSIGN_FALSE(zv) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_FALSE(_zv))
#define ZEND_TRY_ASSIGN_REF_FALSE(zv) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_FALSE(_zv))
#define ZEND_TRY_ASSIGN_TRUE(zv) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_TRUE(_zv))
#define ZEND_TRY_ASSIGN_REF_TRUE(zv) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_TRUE(_zv))
#define ZEND_TRY_ASSIGN_BOOL(zv, bval) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_BOOL(_zv, bval))
#define ZEND_TRY_ASSIGN_REF_BOOL(zv, bval) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_BOOL(_zv, bval))
#define ZEND_TRY_ASSIGN_LONG(zv, lval) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_LONG(_zv, lval))
#define ZEND_TRY_ASSIGN_REF_LONG(zv, lval) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_LONG(_zv, lval))
#define ZEND_TRY_ASSIGN_DOUBLE(zv, dval) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_DOUBLE(_zv, dval))
#define ZEND_TRY_ASSIGN_REF_DOUBLE(zv, dval) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_DOUBLE(_zv, dval))
#define ZEND_TRY_ASSIGN_EMPTY_STRING(zv) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_EMPTY_STRING(_zv))
#define ZEND_TRY_ASSIGN_REF_EMPTY_STRING(zv) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_EMPTY_STRING(_zv))
#define ZEND_TRY_ASSIGN_STR(zv, str) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_STR(_zv, str))
#define ZEND_TRY_ASSIGN_REF_STR(zv, str) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_STR(_zv, str))
#define ZEND_TRY_ASSIGN_NEW_STR(zv, str) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_NEW_STR(_zv, str))
#define ZEND_TRY_ASSIGN_REF_NEW_STR(zv, str) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_NEW_STR(_zv, str))
#define ZEND_TRY_ASSIGN_STRING(zv, string) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_STRING(_zv, string))
#define ZEND_TRY_ASSIGN_REF_STRING(zv, string) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_STRING(_zv, string))
#define ZEND_TRY_ASSIGN_STRINGL(zv, string, len) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_STRINGL(_zv, string, len))
#define ZEND_TRY_ASSIGN_REF_STRINGL(zv, string, len) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_STRINGL(_zv, string, len))
#define ZEND_TRY_ASSIGN_ARR(zv, arr) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_ARR(_zv, arr))
#define ZEND_TRY_ASSIGN_REF_ARR(zv, arr) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_ARR(_zv, arr))
#define ZEND_TRY_ASSIGN_RES(zv, res) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_RES(_zv, res))
#define ZEND_TRY_ASSIGN_REF_RES(zv, res) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_RES(_zv, res))
#define ZEND_TRY_ASSIGN_TMP(zv, other_zv) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_COPY_VALUE(_zv, other_zv))
#define ZEND_TRY_ASSIGN_REF_TMP(zv, other_zv) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_COPY_VALUE(_zv, other_zv))
#define ZEND_TRY_ASSIGN_VALUE(zv, other_zv) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_COPY_VALUE(_zv, other_zv))
#define ZEND_TRY_ASSIGN_REF_VALUE(zv, other_zv) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_COPY_VALUE(_zv, other_zv))
#define ZEND_TRY_ASSIGN_COPY(zv, other_zv) _ZEND_TRY_ASSIGN(zv, 0, ZVAL_COPY(_zv, other_zv))
#define ZEND_TRY_ASSIGN_REF_COPY(zv, other_zv) _ZEND_TRY_ASSIGN(zv, 1, ZVAL_COPY(_zv, other_zv))
#define ZEND_TRY_ASSIGN_COPY_EX(zv, other_zv, strict) ZEND_TRY_ASSIGN_COPY(zv, other_zv)
#define ZEND_TRY_ASSIGN_REF_COPY_EX(zv, other_zv, strict) ZEND_TRY_ASSIGN_REF_COPY(zv, other_zv)

/* ------------------------------------------------------------ conversions */

#define convert_to_string(op) do { if (Z_TYPE_P(op) != IS_STRING) { _convert_to_string((op)); } } while (0)
#define try_convert_to_string(op) (Z_TYPE_P(op) == IS_STRING ? 1 : _try_convert_to_string(op))
#define convert_to_long_ex(zv) convert_to_long(zv)
#define convert_to_double_ex(zv) convert_to_double(zv)
#define convert_to_boolean_ex(zv) convert_to_boolean(zv)
#define convert_to_string_ex(zv) convert_to_string(zv)
#define convert_to_array_ex(zv) convert_to_array(zv)
#define convert_to_null_ex(zv) convert_to_null(zv)
#define convert_scalar_to_number_ex(zv) convert_scalar_to_number(zv)
#define zval_is_true(op) zend_is_true(op)
#define _is_numeric_string_ex(str, length, lval, dval, allow_errors, oflow_info, trailing_data) \
    is_numeric_string_ex((str), (length), (lval), (dval), (allow_errors), (oflow_info), (trailing_data))
#define zend_dval_to_lval_safe(d) zend_dval_to_lval(d)

/* ------------------------------------------------------------ error levels */

/* ------------------------------------------------------------ type masks */

#define ZEND_TYPE_IS_SET(t) (((t).type_mask & _ZEND_TYPE_MASK) != 0)
#define ZEND_TYPE_IS_COMPLEX(t) (((t).type_mask & _ZEND_TYPE_KIND_MASK) != 0)
#define ZEND_TYPE_HAS_NAME(t) (((t).type_mask & _ZEND_TYPE_NAME_BIT) != 0)
#define ZEND_TYPE_HAS_LITERAL_NAME(t) (((t).type_mask & _ZEND_TYPE_LITERAL_NAME_BIT) != 0)
#define ZEND_TYPE_HAS_LIST(t) (((t).type_mask & _ZEND_TYPE_LIST_BIT) != 0)
#define ZEND_TYPE_IS_UNION(t) (((t).type_mask & _ZEND_TYPE_UNION_BIT) != 0)
#define ZEND_TYPE_IS_INTERSECTION(t) (((t).type_mask & _ZEND_TYPE_INTERSECTION_BIT) != 0)
#define ZEND_TYPE_NAME(t) ((zend_string *)(t).ptr)
#define ZEND_TYPE_LITERAL_NAME(t) ((const char *)(t).ptr)
#define ZEND_TYPE_LIST(t) ((zend_type_list *)(t).ptr)
#define ZEND_TYPE_PURE_MASK(t) ((t).type_mask & _ZEND_TYPE_MAY_BE_MASK)
#define ZEND_TYPE_FULL_MASK(t) ((t).type_mask)
#define ZEND_TYPE_ALLOW_NULL(t) (((t).type_mask & _ZEND_TYPE_NULLABLE_BIT) != 0)
#define ZEND_TYPE_CONTAINS_CODE(t, code) (((t).type_mask & (1u << (code))) != 0)
#define ZEND_TYPE_SET_PTR(t, _ptr) ((t).ptr = (_ptr))
#define ZEND_TYPE_SET_LIST(t, list) do { (t).ptr = (list); (t).type_mask |= _ZEND_TYPE_LIST_BIT; } while (0)
#define ZEND_TYPE_INIT_NONE(extra_flags) { NULL, (extra_flags) }
#define ZEND_TYPE_INIT_MASK(_type_mask) { NULL, (_type_mask) }
#define ZEND_TYPE_INIT_CODE(code, allow_null, extra_flags) \
    ZEND_TYPE_INIT_MASK(((code) == _IS_BOOL ? MAY_BE_BOOL : ((code) == IS_ITERABLE ? _ZEND_TYPE_ITERABLE_MASK : \
        ((code) == IS_MIXED ? MAY_BE_ANY : (1u << (code))))) | ((allow_null) ? _ZEND_TYPE_NULLABLE_BIT : 0) | (extra_flags))
#define ZEND_TYPE_INIT_PTR(ptr, type_kind, allow_null, extra_flags) \
    { (void *)(ptr), (type_kind) | ((allow_null) ? _ZEND_TYPE_NULLABLE_BIT : 0) | (extra_flags) }
#define ZEND_TYPE_INIT_PTR_MASK(ptr, type_mask) { (void *)(ptr), (type_mask) }
#define ZEND_TYPE_INIT_UNION(ptr, extra_flags) { (void *)(ptr), (_ZEND_TYPE_LIST_BIT | _ZEND_TYPE_UNION_BIT) | (extra_flags) }
#define ZEND_TYPE_INIT_INTERSECTION(ptr, extra_flags) { (void *)(ptr), (_ZEND_TYPE_LIST_BIT | _ZEND_TYPE_INTERSECTION_BIT) | (extra_flags) }
#define ZEND_TYPE_INIT_CLASS(class_name, allow_null, extra_flags) \
    ZEND_TYPE_INIT_PTR(class_name, _ZEND_TYPE_NAME_BIT, allow_null, extra_flags)
#define ZEND_TYPE_INIT_CLASS_MASK(class_name, type_mask) ZEND_TYPE_INIT_PTR_MASK(class_name, _ZEND_TYPE_NAME_BIT | (type_mask))
#define ZEND_TYPE_INIT_CLASS_CONST(class_name, allow_null, extra_flags) \
    ZEND_TYPE_INIT_PTR(class_name, _ZEND_TYPE_LITERAL_NAME_BIT, allow_null, extra_flags)
#define ZEND_TYPE_INIT_CLASS_CONST_MASK(class_name, type_mask) \
    ZEND_TYPE_INIT_PTR_MASK(class_name, (_ZEND_TYPE_LITERAL_NAME_BIT | (type_mask)))
#define ZEND_TYPE_LIST_SIZE(num_types) (sizeof(zend_type_list) + ((num_types) - 1) * sizeof(zend_type))
#define ZEND_TYPE_LIST_FOREACH(list, type_ptr) do { \
        zend_type *_list = (list)->types; \
        zend_type *_end = _list + (list)->num_types; \
        for (; _list < _end; _list++) { \
            type_ptr = _list;
#define ZEND_TYPE_LIST_FOREACH_END() } } while (0)

/* ------------------------------------------------------------ functions */

#define INTERNAL_FUNCTION_PARAMETERS zend_execute_data *execute_data, zval *return_value
#define INTERNAL_FUNCTION_PARAM_PASSTHRU execute_data, return_value
#define USED_RET() 1
#define ZEND_USER_CODE(type) ((type) != ZEND_INTERNAL_FUNCTION)
#define ZEND_FN_SCOPE_NAME(function) ((function) && (function)->common.scope ? ZSTR_VAL((function)->common.scope->name) : "")

/* ------------------------------------------------------------ arginfo */

#define ZEND_ARG_SEND_MODE(arg_info) ((ZEND_TYPE_FULL_MASK((arg_info)->type) >> _ZEND_SEND_MODE_SHIFT) & 3)
#define ZEND_ARG_IS_VARIADIC(arg_info) ((ZEND_TYPE_FULL_MASK((arg_info)->type) & _ZEND_IS_VARIADIC_BIT) != 0)
#define ZEND_ARG_IS_PROMOTED(arg_info) ((ZEND_TYPE_FULL_MASK((arg_info)->type) & _ZEND_IS_PROMOTED_BIT) != 0)
#define ZEND_ARG_TYPE_IS_TENTATIVE(arg_info) ((ZEND_TYPE_FULL_MASK((arg_info)->type) & _ZEND_IS_TENTATIVE_BIT) != 0)
#define _ZEND_ARG_INFO_FLAGS(pass_by_ref, is_variadic, is_tentative) \
    (((pass_by_ref) << _ZEND_SEND_MODE_SHIFT) | ((is_variadic) ? _ZEND_IS_VARIADIC_BIT : 0) | \
     ((is_tentative) ? _ZEND_IS_TENTATIVE_BIT : 0))
#define ZEND_ARG_INFO(pass_by_ref, name) \
    { #name, ZEND_TYPE_INIT_NONE(_ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), NULL },
#define ZEND_ARG_INFO_WITH_DEFAULT_VALUE(pass_by_ref, name, default_value) \
    { #name, ZEND_TYPE_INIT_NONE(_ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), default_value },
#define ZEND_ARG_VARIADIC_INFO(pass_by_ref, name) \
    { #name, ZEND_TYPE_INIT_NONE(_ZEND_ARG_INFO_FLAGS(pass_by_ref, 1, 0)), NULL },
#define ZEND_ARG_TYPE_INFO(pass_by_ref, name, type_hint, allow_null) \
    { #name, ZEND_TYPE_INIT_CODE(type_hint, allow_null, _ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), NULL },
#define ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(pass_by_ref, name, type_hint, allow_null, default_value) \
    { #name, ZEND_TYPE_INIT_CODE(type_hint, allow_null, _ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), default_value },
#define ZEND_ARG_VARIADIC_TYPE_INFO(pass_by_ref, name, type_hint, allow_null) \
    { #name, ZEND_TYPE_INIT_CODE(type_hint, allow_null, _ZEND_ARG_INFO_FLAGS(pass_by_ref, 1, 0)), NULL },
#define ZEND_ARG_TYPE_MASK(pass_by_ref, name, type_mask, default_value) \
    { #name, ZEND_TYPE_INIT_MASK((type_mask) | _ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), default_value },
#define ZEND_ARG_OBJ_INFO(pass_by_ref, name, classname, allow_null) \
    { #name, ZEND_TYPE_INIT_CLASS_CONST(#classname, allow_null, _ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), NULL },
#define ZEND_ARG_OBJ_INFO_WITH_DEFAULT_VALUE(pass_by_ref, name, classname, allow_null, default_value) \
    { #name, ZEND_TYPE_INIT_CLASS_CONST(#classname, allow_null, _ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), default_value },
#define ZEND_ARG_OBJ_TYPE_MASK(pass_by_ref, name, class_name, type_mask, default_value) \
    { #name, ZEND_TYPE_INIT_CLASS_CONST_MASK(#class_name, (type_mask) | _ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), default_value },
#define ZEND_ARG_VARIADIC_OBJ_INFO(pass_by_ref, name, classname, allow_null) \
    { #name, ZEND_TYPE_INIT_CLASS_CONST(#classname, allow_null, _ZEND_ARG_INFO_FLAGS(pass_by_ref, 1, 0)), NULL },
#define ZEND_ARG_VARIADIC_OBJ_TYPE_MASK(pass_by_ref, name, class_name, type_mask) \
    { #name, ZEND_TYPE_INIT_CLASS_CONST_MASK(#class_name, (type_mask) | _ZEND_ARG_INFO_FLAGS(pass_by_ref, 1, 0)), NULL },
#define ZEND_ARG_ARRAY_INFO(pass_by_ref, name, allow_null) \
    { #name, ZEND_TYPE_INIT_CODE(IS_ARRAY, allow_null, _ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), NULL },
#define ZEND_ARG_CALLABLE_INFO(pass_by_ref, name, allow_null) \
    { #name, ZEND_TYPE_INIT_CODE(IS_CALLABLE, allow_null, _ZEND_ARG_INFO_FLAGS(pass_by_ref, 0, 0)), NULL },
#define ZEND_BEGIN_ARG_WITH_RETURN_OBJ_INFO_EX(name, return_reference, required_num_args, class_name, allow_null) \
    static const zend_internal_arg_info name[] = { \
        { (const char *)(uintptr_t)(required_num_args), \
          ZEND_TYPE_INIT_CLASS_CONST(#class_name, allow_null, _ZEND_ARG_INFO_FLAGS(return_reference, 0, 0)), NULL },
#define ZEND_BEGIN_ARG_WITH_RETURN_OBJ_INFO(name, class_name, allow_null) \
    ZEND_BEGIN_ARG_WITH_RETURN_OBJ_INFO_EX(name, 0, -1, class_name, allow_null)
#define ZEND_BEGIN_ARG_WITH_RETURN_OBJ_TYPE_MASK_EX(name, return_reference, required_num_args, class_name, type) \
    static const zend_internal_arg_info name[] = { \
        { (const char *)(uintptr_t)(required_num_args), \
          ZEND_TYPE_INIT_CLASS_CONST_MASK(#class_name, type | _ZEND_ARG_INFO_FLAGS(return_reference, 0, 0)), NULL },
#define ZEND_BEGIN_ARG_WITH_RETURN_TYPE_MASK_EX(name, return_reference, required_num_args, type) \
    static const zend_internal_arg_info name[] = { \
        { (const char *)(uintptr_t)(required_num_args), \
          ZEND_TYPE_INIT_MASK(type | _ZEND_ARG_INFO_FLAGS(return_reference, 0, 0)), NULL },
#define ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(name, return_reference, required_num_args, type, allow_null) \
    static const zend_internal_arg_info name[] = { \
        { (const char *)(uintptr_t)(required_num_args), \
          ZEND_TYPE_INIT_CODE(type, allow_null, _ZEND_ARG_INFO_FLAGS(return_reference, 0, 0)), NULL },
#define ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO(name, type, allow_null) \
    ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(name, 0, -1, type, allow_null)
#define ZEND_BEGIN_ARG_WITH_TENTATIVE_RETURN_OBJ_INFO_EX(name, return_reference, required_num_args, class_name, allow_null) \
    static const zend_internal_arg_info name[] = { \
        { (const char *)(uintptr_t)(required_num_args), \
          ZEND_TYPE_INIT_CLASS_CONST(#class_name, allow_null, _ZEND_ARG_INFO_FLAGS(return_reference, 0, 1)), NULL },
#define ZEND_BEGIN_ARG_WITH_TENTATIVE_RETURN_OBJ_TYPE_MASK_EX(name, return_reference, required_num_args, class_name, type) \
    static const zend_internal_arg_info name[] = { \
        { (const char *)(uintptr_t)(required_num_args), \
          ZEND_TYPE_INIT_CLASS_CONST_MASK(#class_name, type | _ZEND_ARG_INFO_FLAGS(return_reference, 0, 1)), NULL },
#define ZEND_BEGIN_ARG_WITH_TENTATIVE_RETURN_TYPE_MASK_EX(name, return_reference, required_num_args, type) \
    static const zend_internal_arg_info name[] = { \
        { (const char *)(uintptr_t)(required_num_args), \
          ZEND_TYPE_INIT_MASK(type | _ZEND_ARG_INFO_FLAGS(return_reference, 0, 1)), NULL },
#define ZEND_BEGIN_ARG_WITH_TENTATIVE_RETURN_TYPE_INFO_EX(name, return_reference, required_num_args, type, allow_null) \
    static const zend_internal_arg_info name[] = { \
        { (const char *)(uintptr_t)(required_num_args), \
          ZEND_TYPE_INIT_CODE(type, allow_null, _ZEND_ARG_INFO_FLAGS(return_reference, 0, 1)), NULL },
#define ZEND_BEGIN_ARG_INFO_EX(name, _unused, return_reference, required_num_args) \
    static const zend_internal_arg_info name[] = { \
        { (const char *)(uintptr_t)(required_num_args), \
          ZEND_TYPE_INIT_NONE(_ZEND_ARG_INFO_FLAGS(return_reference, 0, 0)), NULL },
#define ZEND_BEGIN_ARG_INFO(name, _unused) ZEND_BEGIN_ARG_INFO_EX(name, {}, ZEND_RETURN_VALUE, -1)
#define ZEND_END_ARG_INFO() };
#define ZEND_RAW_FENTRY(zend_name, name, arg_info, flags, frameless_function_infos, doc_comment) \
    { zend_name, name, arg_info, (uint32_t)(sizeof(arg_info) / sizeof(struct _zend_internal_arg_info) - 1), flags, \
      frameless_function_infos, doc_comment },
#define ZEND_RAW_NAMED_FE(zend_name, name, arg_info) ZEND_RAW_FENTRY(#zend_name, name, arg_info, 0, NULL, NULL)
#define ZEND_FENTRY(zend_name, name, arg_info, flags) ZEND_RAW_FENTRY(#zend_name, name, arg_info, flags, NULL, NULL)
#define ZEND_FE(name, arg_info) ZEND_RAW_FENTRY(#name, zif_##name, arg_info, 0, NULL, NULL)
#define ZEND_DEP_FE(name, arg_info) ZEND_RAW_FENTRY(#name, zif_##name, arg_info, ZEND_ACC_DEPRECATED, NULL, NULL)
#define ZEND_FALIAS(name, alias, arg_info) ZEND_RAW_FENTRY(#name, zif_##alias, arg_info, 0, NULL, NULL)
#define ZEND_DEP_FALIAS(name, alias, arg_info) ZEND_RAW_FENTRY(#name, zif_##alias, arg_info, ZEND_ACC_DEPRECATED, NULL, NULL)
#define ZEND_NAMED_FE(zend_name, name, arg_info) ZEND_RAW_FENTRY(#zend_name, name, arg_info, 0, NULL, NULL)
#define ZEND_SUPPORTS_COMPILE_TIME_EVAL_FE(name, arg_info) \
    ZEND_RAW_FENTRY(#name, zif_##name, arg_info, ZEND_ACC_COMPILE_TIME_EVAL, NULL, NULL)
#define ZEND_FRAMELESS_FE(name, arg_info, flags, frameless_function_infos, doc_comment) \
    { #name, zif_##name, arg_info, (uint32_t)(sizeof(arg_info) / sizeof(struct _zend_internal_arg_info) - 1), flags, \
      frameless_function_infos, doc_comment },
#define ZEND_NS_FENTRY(ns, zend_name, name, arg_info, flags) \
    ZEND_RAW_FENTRY(ZEND_NS_NAME(ns, #zend_name), name, arg_info, flags, NULL, NULL)
#define ZEND_NS_RAW_FENTRY(ns, zend_name, name, arg_info, flags) \
    ZEND_RAW_FENTRY(ZEND_NS_NAME(ns, zend_name), name, arg_info, flags, NULL, NULL)
#define ZEND_NS_NAMED_FE(ns, zend_name, name, arg_info) ZEND_NS_FENTRY(ns, zend_name, name, arg_info, 0)
#define ZEND_NS_FE(ns, name, arg_info) ZEND_NS_FENTRY(ns, name, zif_##name, arg_info, 0)
#define ZEND_NS_DEP_FE(ns, name, arg_info) ZEND_NS_FENTRY(ns, name, zif_##name, arg_info, ZEND_ACC_DEPRECATED)
#define ZEND_NS_FALIAS(ns, name, alias, arg_info) ZEND_NS_FENTRY(ns, name, zif_##alias, arg_info, 0)
#define ZEND_NS_DEP_FALIAS(ns, name, alias, arg_info) ZEND_NS_FENTRY(ns, name, zif_##alias, arg_info, ZEND_ACC_DEPRECATED)
#define ZEND_ME(classname, name, arg_info, flags) ZEND_RAW_FENTRY(#name, zim_##classname##_##name, arg_info, flags, NULL, NULL)
#define ZEND_DEP_ME(classname, name, arg_info, flags) ZEND_ME(classname, name, arg_info, (flags) | ZEND_ACC_DEPRECATED)
#define ZEND_ABSTRACT_ME(classname, name, arg_info) ZEND_RAW_FENTRY(#name, NULL, arg_info, ZEND_ACC_PUBLIC | ZEND_ACC_ABSTRACT, NULL, NULL)
#define ZEND_ABSTRACT_ME_WITH_FLAGS(classname, name, arg_info, flags) ZEND_RAW_FENTRY(#name, NULL, arg_info, flags, NULL, NULL)
#define ZEND_MALIAS(classname, name, alias, arg_info, flags) ZEND_RAW_FENTRY(#name, zim_##classname##_##alias, arg_info, flags, NULL, NULL)
#define ZEND_ME_MAPPING(name, func_name, arg_info, flags) ZEND_RAW_FENTRY(#name, zif_##func_name, arg_info, flags, NULL, NULL)
#define ZEND_FE_END { NULL, NULL, NULL, 0, 0, NULL, NULL }
#define ZEND_NS_NAME(ns, name) ns "\\" name
#define ZEND_FN(name) zif_##name
#define ZEND_MN(name) zim_##name
#define ZEND_NAMED_FUNCTION(name) void ZEND_FASTCALL name(INTERNAL_FUNCTION_PARAMETERS)
#define ZEND_FUNCTION(name) ZEND_NAMED_FUNCTION(zif_##name)
#define ZEND_METHOD(classname, name) ZEND_NAMED_FUNCTION(zim_##classname##_##name)
#define PHP_FN ZEND_FN
#define PHP_MN ZEND_MN
#define PHP_NAMED_FUNCTION ZEND_NAMED_FUNCTION
#define PHP_FUNCTION ZEND_FUNCTION
#define PHP_METHOD ZEND_METHOD
#define PHP_RAW_NAMED_FE ZEND_RAW_NAMED_FE
#define PHP_NAMED_FE ZEND_NAMED_FE
#define PHP_FE ZEND_FE
#define PHP_DEP_FE ZEND_DEP_FE
#define PHP_FALIAS ZEND_FALIAS
#define PHP_DEP_FALIAS ZEND_DEP_FALIAS
#define PHP_ME ZEND_ME
#define PHP_ABSTRACT_ME ZEND_ABSTRACT_ME
#define PHP_ME_MAPPING ZEND_ME_MAPPING
#define PHP_MALIAS ZEND_MALIAS
#define PHP_FE_END ZEND_FE_END
#define PHP_NS_FE ZEND_NS_FE

/* ------------------------------------------------------------ call frames */

#define ZEND_CALL_FRAME_SLOT ((int)((sizeof(zend_execute_data) + sizeof(zval) - 1) / sizeof(zval)))
#define ZEND_CALL_VAR_NUM(call, n) (((zval *)(call)) + (ZEND_CALL_FRAME_SLOT + ((int)(n))))
#define ZEND_CALL_ARG(call, n) ZEND_CALL_VAR_NUM(call, ((int)(n)) - 1)
#define ZEND_CALL_NUM_ARGS(call) (call)->This.u2.num_args
#define ZEND_CALL_INFO(call) Z_TYPE_INFO((call)->This)
#define EX(element) ((execute_data)->element)
#define EX_CALL_INFO() ZEND_CALL_INFO(execute_data)
#define EX_NUM_ARGS() ZEND_CALL_NUM_ARGS(execute_data)
#define EX_VAR_NUM(n) ZEND_CALL_VAR_NUM(execute_data, n)
#define ZEND_NUM_ARGS() EX_NUM_ARGS()
#define ZEND_THIS (&EX(This))
#define getThis() ((Z_TYPE_P(ZEND_THIS) == IS_OBJECT) ? ZEND_THIS : NULL)
#define ZEND_THIS_OBJECT Z_OBJ_P(ZEND_THIS)
#define ZEND_IS_METHOD_CALL() (EX(func)->common.scope != NULL)
#define WRONG_PARAM_COUNT ZEND_WRONG_PARAM_COUNT()
#define ZEND_WRONG_PARAM_COUNT() do { zend_wrong_param_count(); return; } while (0)
#define ARG_SHOULD_BE_SENT_BY_REF(zf, arg_num) 0
#define ZEND_ARG_USES_STRICT_TYPES() 0
#define ZEND_RET_USES_STRICT_TYPES() 0
#define EX_USES_STRICT_TYPES() 0
#define ZEND_CALL_USES_STRICT_TYPES(call) 0
#define zend_active_function() zend_active_function_ex(EG(current_execute_data))

/* ------------------------------------------------------------ argument parsing */

#define zend_parse_parameters_none() \
    (EXPECTED(ZEND_NUM_ARGS() == 0) ? SUCCESS : (zend_wrong_parameters_none_error(), FAILURE))
#define zend_parse_parameters_none_throw() zend_parse_parameters_none()
#define zend_parse_parameters_throw(num_args, ...) zend_parse_parameters((num_args), __VA_ARGS__)
#define ZEND_PARSE_PARAMETERS_NONE() do { \
        if (UNEXPECTED(ZEND_NUM_ARGS() != 0)) { zend_wrong_parameters_none_error(); RETURN_THROWS(); } \
    } while (0)
#define ZEND_PARSE_PARAMETERS_START_EX(flags, min_num_args, max_num_args) do { \
        const int _flags = (flags); \
        uint32_t _min_num_args = (min_num_args); \
        uint32_t _max_num_args = (uint32_t)(max_num_args); \
        uint32_t _num_args = EX_NUM_ARGS(); \
        uint32_t _i = 0; \
        zval *_real_arg, *_arg = NULL; \
        zend_expected_type _expected_type = Z_EXPECTED_LONG; \
        char *_error = NULL; \
        bool _dummy = 0; \
        bool _optional = 0; \
        int _error_code = ZPP_ERROR_OK; \
        ((void)_i); ((void)_real_arg); ((void)_arg); ((void)_expected_type); ((void)_error); \
        ((void)_optional); ((void)_dummy); \
        do { \
            if (UNEXPECTED(_num_args < _min_num_args) || UNEXPECTED(_num_args > _max_num_args)) { \
                if (!(_flags & ZEND_PARSE_PARAMS_QUIET)) { \
                    zend_wrong_parameters_count_error(_min_num_args, _max_num_args); \
                } \
                _error_code = ZPP_ERROR_FAILURE; \
                break; \
            } \
            _real_arg = ZEND_CALL_ARG(execute_data, 0);
#define ZEND_PARSE_PARAMETERS_START(min_num_args, max_num_args) ZEND_PARSE_PARAMETERS_START_EX(0, min_num_args, max_num_args)
#define ZEND_PARSE_PARAMETERS_END_EX(failure) \
        } while (0); \
        if (UNEXPECTED(_error_code != ZPP_ERROR_OK)) { \
            if (!(_flags & ZEND_PARSE_PARAMS_QUIET)) { \
                zend_wrong_parameter_error(_error_code, _i, _error, _expected_type, _arg); \
            } \
            failure; \
        } \
    } while (0)
#define ZEND_PARSE_PARAMETERS_END() ZEND_PARSE_PARAMETERS_END_EX(return)
#define Z_PARAM_OPTIONAL _optional = 1;
#define Z_PARAM_PROLOGUE(deref, separate) \
    ++_i; \
    if (_optional) { \
        if (UNEXPECTED(_i > _num_args)) break; \
    } \
    _real_arg++; \
    _arg = _real_arg; \
    if (deref) { \
        if (EXPECTED(Z_ISREF_P(_arg))) { _arg = Z_REFVAL_P(_arg); } \
    } \
    if (separate) { SEPARATE_ZVAL_NOREF(_arg); }
#define ZEND_PARSE_PARAMETERS_SET_FAILURE(code, expected) \
    { _expected_type = (expected); _error_code = (code); break; }
#define Z_PARAM_ARRAY_EX2(dest, check_null, deref, separate) \
    Z_PARAM_PROLOGUE(deref, separate); \
    if (UNEXPECTED(!zend_parse_arg_array(_arg, &dest, check_null, 0))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_ARRAY_OR_NULL : Z_EXPECTED_ARRAY)
#define Z_PARAM_ARRAY_EX(dest, check_null, separate) Z_PARAM_ARRAY_EX2(dest, check_null, separate, separate)
#define Z_PARAM_ARRAY(dest) Z_PARAM_ARRAY_EX(dest, 0, 0)
#define Z_PARAM_ARRAY_OR_NULL(dest) Z_PARAM_ARRAY_EX(dest, 1, 0)
#define Z_PARAM_ARRAY_OR_OBJECT_EX2(dest, check_null, deref, separate) \
    Z_PARAM_PROLOGUE(deref, separate); \
    if (UNEXPECTED(!zend_parse_arg_array(_arg, &dest, check_null, 1))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_ARRAY_OR_NULL : Z_EXPECTED_ARRAY)
#define Z_PARAM_ARRAY_OR_OBJECT_EX(dest, check_null, separate) Z_PARAM_ARRAY_OR_OBJECT_EX2(dest, check_null, separate, separate)
#define Z_PARAM_ARRAY_OR_OBJECT(dest) Z_PARAM_ARRAY_OR_OBJECT_EX(dest, 0, 0)
#define Z_PARAM_ARRAY_HT_EX2(dest, check_null, deref, separate) \
    Z_PARAM_PROLOGUE(deref, separate); \
    if (UNEXPECTED(!zend_parse_arg_array_ht(_arg, &dest, check_null, 0, separate))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_ARRAY_OR_NULL : Z_EXPECTED_ARRAY)
#define Z_PARAM_ARRAY_HT_EX(dest, check_null, separate) Z_PARAM_ARRAY_HT_EX2(dest, check_null, separate, separate)
#define Z_PARAM_ARRAY_HT(dest) Z_PARAM_ARRAY_HT_EX(dest, 0, 0)
#define Z_PARAM_ARRAY_HT_OR_NULL(dest) Z_PARAM_ARRAY_HT_EX(dest, 1, 0)
#define Z_PARAM_ARRAY_HT_OR_LONG_EX(dest_ht, dest_long, is_null, allow_null) \
    Z_PARAM_PROLOGUE(0, 0); \
    if (UNEXPECTED(!zend_parse_arg_array_ht_or_long(_arg, &dest_ht, &dest_long, &is_null, allow_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, allow_null ? Z_EXPECTED_ARRAY_OR_LONG_OR_NULL : Z_EXPECTED_ARRAY_OR_LONG)
#define Z_PARAM_ARRAY_HT_OR_LONG(dest_ht, dest_long) Z_PARAM_ARRAY_HT_OR_LONG_EX(dest_ht, dest_long, _dummy, 0)
#define Z_PARAM_ARRAY_HT_OR_LONG_OR_NULL(dest_ht, dest_long, is_null) Z_PARAM_ARRAY_HT_OR_LONG_EX(dest_ht, dest_long, is_null, 1)
#define Z_PARAM_ARRAY_OR_OBJECT_HT_EX2(dest, check_null, deref, separate) \
    Z_PARAM_PROLOGUE(deref, separate); \
    if (UNEXPECTED(!zend_parse_arg_array_ht(_arg, &dest, check_null, 1, separate))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_ARRAY_OR_NULL : Z_EXPECTED_ARRAY)
#define Z_PARAM_ARRAY_OR_OBJECT_HT_EX(dest, check_null, separate) Z_PARAM_ARRAY_OR_OBJECT_HT_EX2(dest, check_null, separate, separate)
#define Z_PARAM_ARRAY_OR_OBJECT_HT(dest) Z_PARAM_ARRAY_OR_OBJECT_HT_EX(dest, 0, 0)
#define Z_PARAM_ARRAY_HT_OR_STR_EX(dest_ht, dest_str, allow_null) \
    Z_PARAM_PROLOGUE(0, 0); \
    if (UNEXPECTED(!zend_parse_arg_array_ht_or_str(_arg, &dest_ht, &dest_str, allow_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, allow_null ? Z_EXPECTED_ARRAY_OR_STRING_OR_NULL : Z_EXPECTED_ARRAY_OR_STRING)
#define Z_PARAM_ARRAY_HT_OR_STR(dest_ht, dest_str) Z_PARAM_ARRAY_HT_OR_STR_EX(dest_ht, dest_str, 0)
#define Z_PARAM_ARRAY_HT_OR_STR_OR_NULL(dest_ht, dest_str) Z_PARAM_ARRAY_HT_OR_STR_EX(dest_ht, dest_str, 1)
#define Z_PARAM_BOOL_EX(dest, is_null, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_bool(_arg, &dest, &is_null, check_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_BOOL_OR_NULL : Z_EXPECTED_BOOL)
#define Z_PARAM_BOOL(dest) Z_PARAM_BOOL_EX(dest, _dummy, 0, 0)
#define Z_PARAM_BOOL_OR_NULL(dest, is_null) Z_PARAM_BOOL_EX(dest, is_null, 1, 0)
#define Z_PARAM_CLASS_EX(dest, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_class(_arg, &dest, _i, check_null))) { _error_code = ZPP_ERROR_FAILURE; break; }
#define Z_PARAM_CLASS(dest) Z_PARAM_CLASS_EX(dest, 0, 0)
#define Z_PARAM_CLASS_OR_NULL(dest) Z_PARAM_CLASS_EX(dest, 1, 0)
#define Z_PARAM_DOUBLE_EX(dest, is_null, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_double(_arg, &dest, &is_null, check_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_DOUBLE_OR_NULL : Z_EXPECTED_DOUBLE)
#define Z_PARAM_DOUBLE(dest) Z_PARAM_DOUBLE_EX(dest, _dummy, 0, 0)
#define Z_PARAM_DOUBLE_OR_NULL(dest, is_null) Z_PARAM_DOUBLE_EX(dest, is_null, 1, 0)
#define Z_PARAM_FUNC_EX2(dest_fci, dest_fcc, check_null, deref, free_trampoline) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_func(_arg, &dest_fci, &dest_fcc, check_null, &_error, free_trampoline))) { \
        if (!_error) { \
            _expected_type = check_null ? Z_EXPECTED_FUNC_OR_NULL : Z_EXPECTED_FUNC; \
            _error_code = ZPP_ERROR_WRONG_ARG; \
        } else { \
            _error_code = check_null ? ZPP_ERROR_WRONG_CALLBACK_OR_NULL : ZPP_ERROR_WRONG_CALLBACK; \
        } \
        break; \
    }
#define Z_PARAM_FUNC_EX(dest_fci, dest_fcc, check_null, deref) Z_PARAM_FUNC_EX2(dest_fci, dest_fcc, check_null, deref, 1)
#define Z_PARAM_FUNC(dest_fci, dest_fcc) Z_PARAM_FUNC_EX(dest_fci, dest_fcc, 0, 0)
#define Z_PARAM_FUNC_NO_TRAMPOLINE_FREE(dest_fci, dest_fcc) Z_PARAM_FUNC_EX2(dest_fci, dest_fcc, 0, 0, 0)
#define Z_PARAM_FUNC_OR_NULL(dest_fci, dest_fcc) Z_PARAM_FUNC_EX(dest_fci, dest_fcc, 1, 0)
#define Z_PARAM_ITERABLE_EX(dest, check_null) \
    Z_PARAM_PROLOGUE(0, 0); \
    if (UNEXPECTED(!zend_parse_arg_iterable(_arg, &dest, check_null))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_ITERABLE_OR_NULL : Z_EXPECTED_ITERABLE)
#define Z_PARAM_ITERABLE(dest) Z_PARAM_ITERABLE_EX(dest, 0)
#define Z_PARAM_ITERABLE_OR_NULL(dest) Z_PARAM_ITERABLE_EX(dest, 1)
#define Z_PARAM_LONG_EX(dest, is_null, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_long(_arg, &dest, &is_null, check_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_LONG_OR_NULL : Z_EXPECTED_LONG)
#define Z_PARAM_LONG(dest) Z_PARAM_LONG_EX(dest, _dummy, 0, 0)
#define Z_PARAM_LONG_OR_NULL(dest, is_null) Z_PARAM_LONG_EX(dest, is_null, 1, 0)
#define Z_PARAM_NUMBER_EX(dest, check_null) \
    Z_PARAM_PROLOGUE(0, 0); \
    if (UNEXPECTED(!zend_parse_arg_number(_arg, &dest, check_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_NUMBER_OR_NULL : Z_EXPECTED_NUMBER)
#define Z_PARAM_NUMBER(dest) Z_PARAM_NUMBER_EX(dest, 0)
#define Z_PARAM_NUMBER_OR_NULL(dest) Z_PARAM_NUMBER_EX(dest, 1)
#define Z_PARAM_NUMBER_OR_STR_EX(dest, check_null) \
    Z_PARAM_PROLOGUE(0, 0); \
    if (UNEXPECTED(!zend_parse_arg_number_or_str(_arg, &dest, check_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_NUMBER_OR_STRING_OR_NULL : Z_EXPECTED_NUMBER_OR_STRING)
#define Z_PARAM_NUMBER_OR_STR(dest) Z_PARAM_NUMBER_OR_STR_EX(dest, 0)
#define Z_PARAM_NUMBER_OR_STR_OR_NULL(dest) Z_PARAM_NUMBER_OR_STR_EX(dest, 1)
#define Z_PARAM_OBJECT_EX(dest, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_object(_arg, &dest, NULL, check_null))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_OBJECT_OR_NULL : Z_EXPECTED_OBJECT)
#define Z_PARAM_OBJECT(dest) Z_PARAM_OBJECT_EX(dest, 0, 0)
#define Z_PARAM_OBJECT_OR_NULL(dest) Z_PARAM_OBJECT_EX(dest, 1, 0)
#define Z_PARAM_OBJ_EX(dest, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_obj(_arg, &dest, NULL, check_null))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_OBJECT_OR_NULL : Z_EXPECTED_OBJECT)
#define Z_PARAM_OBJ(dest) Z_PARAM_OBJ_EX(dest, 0, 0)
#define Z_PARAM_OBJ_OR_NULL(dest) Z_PARAM_OBJ_EX(dest, 1, 0)
#define Z_PARAM_OBJECT_OF_CLASS_EX(dest, _ce, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_object(_arg, &dest, _ce, check_null))) { \
        if (_ce) { _error = ZSTR_VAL((_ce)->name); \
            _error_code = check_null ? ZPP_ERROR_WRONG_CLASS_OR_NULL : ZPP_ERROR_WRONG_CLASS; break; } \
        else { ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_OBJECT_OR_NULL : Z_EXPECTED_OBJECT) } \
    }
#define Z_PARAM_OBJECT_OF_CLASS(dest, _ce) Z_PARAM_OBJECT_OF_CLASS_EX(dest, _ce, 0, 0)
#define Z_PARAM_OBJECT_OF_CLASS_OR_NULL(dest, _ce) Z_PARAM_OBJECT_OF_CLASS_EX(dest, _ce, 1, 0)
#define Z_PARAM_OBJ_OF_CLASS_EX(dest, _ce, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_obj(_arg, &dest, _ce, check_null))) { \
        if (_ce) { _error = ZSTR_VAL((_ce)->name); \
            _error_code = check_null ? ZPP_ERROR_WRONG_CLASS_OR_NULL : ZPP_ERROR_WRONG_CLASS; break; } \
        else { ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_OBJECT_OR_NULL : Z_EXPECTED_OBJECT) } \
    }
#define Z_PARAM_OBJ_OF_CLASS(dest, _ce) Z_PARAM_OBJ_OF_CLASS_EX(dest, _ce, 0, 0)
#define Z_PARAM_OBJ_OF_CLASS_OR_NULL(dest, _ce) Z_PARAM_OBJ_OF_CLASS_EX(dest, _ce, 1, 0)
#define Z_PARAM_OBJ_OF_CLASS_OR_LONG_EX(dest_obj, _ce, dest_long, is_null, allow_null) \
    Z_PARAM_PROLOGUE(0, 0); \
    if (UNEXPECTED(!zend_parse_arg_obj_or_long(_arg, &dest_obj, _ce, &dest_long, &is_null, allow_null, _i))) { \
        _error = ZSTR_VAL((_ce)->name); \
        _error_code = allow_null ? ZPP_ERROR_WRONG_CLASS_OR_LONG_OR_NULL : ZPP_ERROR_WRONG_CLASS_OR_LONG; break; }
#define Z_PARAM_OBJ_OF_CLASS_OR_LONG(dest_obj, _ce, dest_long) Z_PARAM_OBJ_OF_CLASS_OR_LONG_EX(dest_obj, _ce, dest_long, _dummy, 0)
#define Z_PARAM_OBJ_OF_CLASS_OR_LONG_OR_NULL(dest_obj, _ce, dest_long, is_null) Z_PARAM_OBJ_OF_CLASS_OR_LONG_EX(dest_obj, _ce, dest_long, is_null, 1)
#define Z_PARAM_OBJ_OF_CLASS_OR_STR_EX(destination_object, base_ce, destination_string, allow_null) \
    Z_PARAM_PROLOGUE(0, 0); \
    if (UNEXPECTED(!zend_parse_arg_obj_or_str(_arg, &destination_object, base_ce, &destination_string, allow_null, _i))) { \
        if (base_ce) { _error = ZSTR_VAL((base_ce)->name); \
            _error_code = allow_null ? ZPP_ERROR_WRONG_CLASS_OR_STRING_OR_NULL : ZPP_ERROR_WRONG_CLASS_OR_STRING; break; } \
        else { ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, allow_null ? Z_EXPECTED_OBJECT_OR_STRING_OR_NULL : Z_EXPECTED_OBJECT_OR_STRING) } \
    }
#define Z_PARAM_OBJ_OF_CLASS_OR_STR(destination_object, base_ce, destination_string) \
    Z_PARAM_OBJ_OF_CLASS_OR_STR_EX(destination_object, base_ce, destination_string, 0)
#define Z_PARAM_OBJ_OF_CLASS_OR_STR_OR_NULL(destination_object, base_ce, destination_string) \
    Z_PARAM_OBJ_OF_CLASS_OR_STR_EX(destination_object, base_ce, destination_string, 1)
#define Z_PARAM_OBJ_OR_STR(destination_object, destination_string) \
    Z_PARAM_OBJ_OF_CLASS_OR_STR_EX(destination_object, NULL, destination_string, 0)
#define Z_PARAM_OBJ_OR_STR_OR_NULL(destination_object, destination_string) \
    Z_PARAM_OBJ_OF_CLASS_OR_STR_EX(destination_object, NULL, destination_string, 1)
#define Z_PARAM_PATH_EX(dest, dest_len, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_path(_arg, &dest, &dest_len, check_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_PATH_OR_NULL : Z_EXPECTED_PATH)
#define Z_PARAM_PATH(dest, dest_len) Z_PARAM_PATH_EX(dest, dest_len, 0, 0)
#define Z_PARAM_PATH_OR_NULL(dest, dest_len) Z_PARAM_PATH_EX(dest, dest_len, 1, 0)
#define Z_PARAM_PATH_STR_EX(dest, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_path_str(_arg, &dest, check_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_PATH_OR_NULL : Z_EXPECTED_PATH)
#define Z_PARAM_PATH_STR(dest) Z_PARAM_PATH_STR_EX(dest, 0, 0)
#define Z_PARAM_PATH_STR_OR_NULL(dest) Z_PARAM_PATH_STR_EX(dest, 1, 0)
#define Z_PARAM_RESOURCE_EX(dest, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_resource(_arg, &dest, check_null))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_RESOURCE_OR_NULL : Z_EXPECTED_RESOURCE)
#define Z_PARAM_RESOURCE(dest) Z_PARAM_RESOURCE_EX(dest, 0, 0)
#define Z_PARAM_RESOURCE_OR_NULL(dest) Z_PARAM_RESOURCE_EX(dest, 1, 0)
#define Z_PARAM_STRING_EX(dest, dest_len, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_string(_arg, &dest, &dest_len, check_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_STRING_OR_NULL : Z_EXPECTED_STRING)
#define Z_PARAM_STRING(dest, dest_len) Z_PARAM_STRING_EX(dest, dest_len, 0, 0)
#define Z_PARAM_STRING_OR_NULL(dest, dest_len) Z_PARAM_STRING_EX(dest, dest_len, 1, 0)
#define Z_PARAM_STR_EX(dest, check_null, deref) \
    Z_PARAM_PROLOGUE(deref, 0); \
    if (UNEXPECTED(!zend_parse_arg_str(_arg, &dest, check_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, check_null ? Z_EXPECTED_STRING_OR_NULL : Z_EXPECTED_STRING)
#define Z_PARAM_STR(dest) Z_PARAM_STR_EX(dest, 0, 0)
#define Z_PARAM_STR_OR_NULL(dest) Z_PARAM_STR_EX(dest, 1, 0)
#define Z_PARAM_STR_OR_LONG_EX(dest_str, dest_long, is_null, allow_null) \
    Z_PARAM_PROLOGUE(0, 0); \
    if (UNEXPECTED(!zend_parse_arg_str_or_long(_arg, &dest_str, &dest_long, &is_null, allow_null, _i))) \
        ZEND_PARSE_PARAMETERS_SET_FAILURE(ZPP_ERROR_WRONG_ARG, allow_null ? Z_EXPECTED_STRING_OR_LONG_OR_NULL : Z_EXPECTED_STRING_OR_LONG)
#define Z_PARAM_STR_OR_LONG(dest_str, dest_long) Z_PARAM_STR_OR_LONG_EX(dest_str, dest_long, _dummy, 0)
#define Z_PARAM_STR_OR_LONG_OR_NULL(dest_str, dest_long, is_null) Z_PARAM_STR_OR_LONG_EX(dest_str, dest_long, is_null, 1)
#define Z_PARAM_ZVAL_EX2(dest, check_null, deref, separate) \
    Z_PARAM_PROLOGUE(deref, separate); \
    zend_parse_arg_zval_deref(_arg, &dest, check_null);
#define Z_PARAM_ZVAL_EX(dest, check_null, separate) Z_PARAM_ZVAL_EX2(dest, check_null, separate, separate)
#define Z_PARAM_ZVAL(dest) Z_PARAM_ZVAL_EX(dest, 0, 0)
#define Z_PARAM_ZVAL_OR_NULL(dest) Z_PARAM_ZVAL_EX(dest, 1, 0)
#define Z_PARAM_VARIADIC_EX(spec, dest, dest_num, post_varargs) do { \
        uint32_t _num_varargs = _num_args - _i - (post_varargs); \
        if (EXPECTED(_num_varargs > 0)) { \
            dest = _real_arg + 1; \
            dest_num = _num_varargs; \
            _i += _num_varargs; \
            _real_arg += _num_varargs; \
        } else { \
            dest = NULL; \
            dest_num = 0; \
        } \
    } while (0);
#define Z_PARAM_VARIADIC(spec, dest, dest_num) Z_PARAM_VARIADIC_EX(spec, dest, dest_num, 0)
#define Z_PARAM_VARIADIC_WITH_NAMED(dest, dest_num, dest_named) \
    Z_PARAM_VARIADIC_EX('*', dest, dest_num, 0) dest_named = NULL;

/* ------------------------------------------------------------ callables */

#define ZEND_FCI_INITIALIZED(fci) ((fci).size != 0)
#define ZEND_FCC_INITIALIZED(fcc) ((fcc).function_handler != NULL)
#define call_user_function(function_table, object, function_name, retval_ptr, param_count, params) \
    _call_user_function_impl(object, function_name, retval_ptr, param_count, params, NULL)
#define call_user_function_named(function_table, object, function_name, retval_ptr, param_count, params, named_params) \
    _call_user_function_impl(object, function_name, retval_ptr, param_count, params, named_params)
#define zend_call_known_instance_method(fn, object, retval_ptr, param_count, params) \
    zend_call_known_function((fn), (object), (object)->ce, (retval_ptr), (param_count), (params), NULL)
#define zend_call_known_instance_method_with_0_params(fn, object, retval_ptr) \
    zend_call_known_instance_method((fn), (object), (retval_ptr), 0, NULL)
#define zend_call_method_with_0_params(obj, obj_ce, fn_proxy, function_name, retval) \
    zend_call_method(obj, obj_ce, fn_proxy, function_name, sizeof(function_name) - 1, retval, 0, NULL, NULL)
#define zend_call_method_with_1_params(obj, obj_ce, fn_proxy, function_name, retval, arg1) \
    zend_call_method(obj, obj_ce, fn_proxy, function_name, sizeof(function_name) - 1, retval, 1, arg1, NULL)
#define zend_call_method_with_2_params(obj, obj_ce, fn_proxy, function_name, retval, arg1, arg2) \
    zend_call_method(obj, obj_ce, fn_proxy, function_name, sizeof(function_name) - 1, retval, 2, arg1, arg2)

/* ------------------------------------------------------------ modules */

#define ZEND_MOD_REQUIRED_EX(name, rel, ver) { name, rel, ver, MODULE_DEP_REQUIRED },
#define ZEND_MOD_CONFLICTS_EX(name, rel, ver) { name, rel, ver, MODULE_DEP_CONFLICTS },
#define ZEND_MOD_OPTIONAL_EX(name, rel, ver) { name, rel, ver, MODULE_DEP_OPTIONAL },
#define ZEND_MOD_REQUIRED(name) ZEND_MOD_REQUIRED_EX(name, NULL, NULL)
#define ZEND_MOD_CONFLICTS(name) ZEND_MOD_CONFLICTS_EX(name, NULL, NULL)
#define ZEND_MOD_OPTIONAL(name) ZEND_MOD_OPTIONAL_EX(name, NULL, NULL)
#define ZEND_MOD_END { NULL, NULL, NULL, 0 }
#define PHP_MOD_REQUIRED ZEND_MOD_REQUIRED
#define PHP_MOD_CONFLICTS ZEND_MOD_CONFLICTS
#define PHP_MOD_OPTIONAL ZEND_MOD_OPTIONAL
#define PHP_MOD_END ZEND_MOD_END
#define STANDARD_MODULE_HEADER_EX sizeof(zend_module_entry), ZEND_MODULE_API_NO, ZEND_DEBUG, USING_ZTS
#define STANDARD_MODULE_HEADER STANDARD_MODULE_HEADER_EX, NULL, NULL
#define ZE2_STANDARD_MODULE_HEADER STANDARD_MODULE_HEADER
#define ZEND_MODULE_BUILD_ID_ ZEND_MODULE_BUILD_ID
#define STANDARD_MODULE_PROPERTIES_EX 0, 0, NULL, 0, ZEND_MODULE_BUILD_ID
#define NO_MODULE_GLOBALS 0, NULL, NULL, NULL
#define STANDARD_MODULE_PROPERTIES NO_MODULE_GLOBALS, NULL, STANDARD_MODULE_PROPERTIES_EX
#define NO_VERSION_YET NULL
#define PHP_MODULE_GLOBALS(module_name) sizeof(zend_##module_name##_globals), &module_name##_globals
#define ZEND_MODULE_GLOBALS(module_name) PHP_MODULE_GLOBALS(module_name)
#define PHP_STANDARD_MODULE_PROPERTIES_EX STANDARD_MODULE_PROPERTIES_EX
#define PHP_MODULE_GLOBALS_BULK(module_name) (&module_name##_globals)
#define INIT_FUNC_ARGS int type, int module_number
#define INIT_FUNC_ARGS_PASSTHRU type, module_number
#define SHUTDOWN_FUNC_ARGS int type, int module_number
#define SHUTDOWN_FUNC_ARGS_PASSTHRU type, module_number
#define ZEND_MODULE_INFO_FUNC_ARGS zend_module_entry *zend_module
#define ZEND_MODULE_INFO_FUNC_ARGS_PASSTHRU zend_module
#define ZEND_MODULE_STARTUP_N(module) zm_startup_##module
#define ZEND_MODULE_SHUTDOWN_N(module) zm_shutdown_##module
#define ZEND_MODULE_ACTIVATE_N(module) zm_activate_##module
#define ZEND_MODULE_DEACTIVATE_N(module) zm_deactivate_##module
#define ZEND_MODULE_POST_ZEND_DEACTIVATE_N(module) zm_post_zend_deactivate_##module
#define ZEND_MODULE_INFO_N(module) zm_info_##module
#define ZEND_MODULE_GLOBALS_CTOR_N(module) zm_globals_ctor_##module
#define ZEND_MODULE_GLOBALS_DTOR_N(module) zm_globals_dtor_##module
#define ZEND_MODULE_STARTUP_D(module) zend_result ZEND_MODULE_STARTUP_N(module)(INIT_FUNC_ARGS)
#define ZEND_MODULE_SHUTDOWN_D(module) zend_result ZEND_MODULE_SHUTDOWN_N(module)(SHUTDOWN_FUNC_ARGS)
#define ZEND_MODULE_ACTIVATE_D(module) zend_result ZEND_MODULE_ACTIVATE_N(module)(INIT_FUNC_ARGS)
#define ZEND_MODULE_DEACTIVATE_D(module) zend_result ZEND_MODULE_DEACTIVATE_N(module)(SHUTDOWN_FUNC_ARGS)
#define ZEND_MODULE_POST_ZEND_DEACTIVATE_D(module) zend_result ZEND_MODULE_POST_ZEND_DEACTIVATE_N(module)(void)
#define ZEND_MODULE_INFO_D(module) ZEND_COLD void ZEND_MODULE_INFO_N(module)(ZEND_MODULE_INFO_FUNC_ARGS)
#define ZEND_MODULE_GLOBALS_CTOR_D(module) void ZEND_MODULE_GLOBALS_CTOR_N(module)(zend_##module##_globals *module##_globals)
#define ZEND_MODULE_GLOBALS_DTOR_D(module) void ZEND_MODULE_GLOBALS_DTOR_N(module)(zend_##module##_globals *module##_globals)
#define ZEND_MINIT ZEND_MODULE_STARTUP_N
#define ZEND_MSHUTDOWN ZEND_MODULE_SHUTDOWN_N
#define ZEND_RINIT ZEND_MODULE_ACTIVATE_N
#define ZEND_RSHUTDOWN ZEND_MODULE_DEACTIVATE_N
#define ZEND_MINFO ZEND_MODULE_INFO_N
#define ZEND_GINIT(module) ((void (*)(void *))(ZEND_MODULE_GLOBALS_CTOR_N(module)))
#define ZEND_GSHUTDOWN(module) ((void (*)(void *))(ZEND_MODULE_GLOBALS_DTOR_N(module)))
#define ZEND_MINIT_FUNCTION ZEND_MODULE_STARTUP_D
#define ZEND_MSHUTDOWN_FUNCTION ZEND_MODULE_SHUTDOWN_D
#define ZEND_RINIT_FUNCTION ZEND_MODULE_ACTIVATE_D
#define ZEND_RSHUTDOWN_FUNCTION ZEND_MODULE_DEACTIVATE_D
#define ZEND_MINFO_FUNCTION ZEND_MODULE_INFO_D
#define ZEND_GINIT_FUNCTION ZEND_MODULE_GLOBALS_CTOR_D
#define ZEND_GSHUTDOWN_FUNCTION ZEND_MODULE_GLOBALS_DTOR_D
#define PHP_MINIT ZEND_MINIT
#define PHP_MSHUTDOWN ZEND_MSHUTDOWN
#define PHP_RINIT ZEND_RINIT
#define PHP_RSHUTDOWN ZEND_RSHUTDOWN
#define PHP_MINFO ZEND_MINFO
#define PHP_GINIT ZEND_GINIT
#define PHP_GSHUTDOWN ZEND_GSHUTDOWN
#define PHP_MINIT_FUNCTION ZEND_MINIT_FUNCTION
#define PHP_MSHUTDOWN_FUNCTION ZEND_MSHUTDOWN_FUNCTION
#define PHP_RINIT_FUNCTION ZEND_RINIT_FUNCTION
#define PHP_RSHUTDOWN_FUNCTION ZEND_RSHUTDOWN_FUNCTION
#define PHP_MINFO_FUNCTION ZEND_MINFO_FUNCTION
#define PHP_GINIT_FUNCTION ZEND_GINIT_FUNCTION
#define PHP_GSHUTDOWN_FUNCTION ZEND_GSHUTDOWN_FUNCTION
#define PHP_MODULE_GLOBALS_CTOR_N ZEND_MODULE_GLOBALS_CTOR_N
#define ZEND_MODULE_POST_ZEND_DEACTIVATE ZEND_MODULE_POST_ZEND_DEACTIVATE_N
#define PHP_PRSHUTDOWN ZEND_MODULE_POST_ZEND_DEACTIVATE_N
#define PHP_PRSHUTDOWN_FUNCTION ZEND_MODULE_POST_ZEND_DEACTIVATE_D
#define ZEND_GET_MODULE(name) \
    BEGIN_EXTERN_C() ZEND_DLEXPORT zend_module_entry *get_module(void) { return &name##_module_entry; } END_EXTERN_C()
#define ZEND_BEGIN_MODULE_GLOBALS(module_name) typedef struct _zend_##module_name##_globals {
#define ZEND_END_MODULE_GLOBALS(module_name) } zend_##module_name##_globals;
#define ZEND_DECLARE_MODULE_GLOBALS(module_name) zend_##module_name##_globals module_name##_globals;
#define ZEND_EXTERN_MODULE_GLOBALS(module_name) extern zend_##module_name##_globals module_name##_globals;
#define ZEND_INIT_MODULE_GLOBALS(module_name, globals_ctor, globals_dtor) globals_ctor(&module_name##_globals);
#define ZEND_MODULE_GLOBALS_ACCESSOR(module_name, v) (module_name##_globals.v)
#define ZEND_MODULE_GLOBALS_BULK(module_name) (&module_name##_globals)
#define ZEND_GLOBALS_ID(module_name) (module_name##_globals)
#define PHP_BEGIN_MODULE_GLOBALS ZEND_BEGIN_MODULE_GLOBALS
#define PHP_END_MODULE_GLOBALS ZEND_END_MODULE_GLOBALS
#define zend_module_loaded(name) (zend_hash_str_find_ptr(&module_registry, (name), strlen(name)) != NULL)

/* ------------------------------------------------------------ globals */

#define JMP_BUF sigjmp_buf
#define SETJMP(a) sigsetjmp(a, 0)
#define LONGJMP(a, b) siglongjmp(a, b)
#define EG(v) (executor_globals.v)
#define CG(v) (compiler_globals.v)

/* ------------------------------------------------------------ classes */

#define INIT_CLASS_ENTRY(class_container, class_name, functions) \
    INIT_CLASS_ENTRY_EX(class_container, class_name, sizeof(class_name) - 1, functions)
#define INIT_CLASS_ENTRY_EX(class_container, class_name, class_name_len, functions) do { \
        memset(&class_container, 0, sizeof(zend_class_entry)); \
        class_container.name = zend_string_init_interned(class_name, class_name_len, 1); \
        class_container.default_object_handlers = &std_object_handlers; \
        class_container.info.internal.builtin_functions = functions; \
    } while (0)
#define INIT_CLASS_ENTRY_INIT_METHODS(class_container, functions) \
    do { class_container.info.internal.builtin_functions = functions; } while (0)
#define INIT_NS_CLASS_ENTRY(class_container, ns, class_name, functions) \
    INIT_CLASS_ENTRY(class_container, ZEND_NS_NAME(ns, class_name), functions)
#define INIT_OVERLOADED_CLASS_ENTRY_EX(class_container, class_name, class_name_len, functions, handle_fcall, \
                                       handle_propget, handle_propset, handle_propunset, handle_propisset) \
    INIT_CLASS_ENTRY_EX(class_container, class_name, class_name_len, functions)
#define INIT_OVERLOADED_CLASS_ENTRY(class_container, class_name, functions, handle_fcall, handle_propget, \
                                    handle_propset) \
    INIT_CLASS_ENTRY(class_container, class_name, functions)
#define zend_do_inheritance(ce, parent_ce) zend_do_inheritance_ex(ce, parent_ce, 0)
#define zend_register_class_alias(name, ce) zend_register_class_alias_ex(name, sizeof(name) - 1, ce, 1)
#define zend_register_ns_class_alias(ns, name, ce) \
    zend_register_class_alias_ex(ZEND_NS_NAME(ns, name), sizeof(ZEND_NS_NAME(ns, name)) - 1, ce, 1)
#define zend_get_object_type(ce) zend_get_object_type_case(ce, 0)
#define zend_get_object_type_uc(ce) zend_get_object_type_case(ce, 1)
#define CE_CONSTANTS_TABLE(ce) (&(ce)->constants_table)
#define CE_DEFAULT_PROPERTIES_TABLE(ce) ((ce)->default_properties_table)
#define CE_STATIC_MEMBERS(ce) ((zval *)(ce)->static_members_table__ptr)
#define ZEND_CLASS_CONST_FLAGS(c) Z_ACCESS_FLAGS((c)->value)
#define ZEND_CLASS_CONST_FLAGS_SET(c, flags) do { Z_ACCESS_FLAGS((c)->value) = (flags); } while (0)
#define Z_ACCESS_FLAGS(zval) (zval).u2.constant_flags
#define zend_unmangle_property_name(mangled_property, class_name, prop_name) \
    zend_unmangle_property_name_ex(mangled_property, class_name, prop_name, NULL)

/* ------------------------------------------------------------ objects */

#define zend_get_std_object_handlers() (&std_object_handlers)
#define ZEND_WRONG_PROPERTY_INFO ((zend_property_info *)((intptr_t)-1))
#define ZEND_DYNAMIC_PROPERTY_OFFSET ((uintptr_t)(intptr_t)(-1))
#define ZEND_WRONG_PROPERTY_OFFSET 0
#define Z_OBJ_P_HANDLER Z_OBJ_HANDLER_P
#define ZEND_COMPARE_OBJECTS_FALLBACK(op1, op2) \
    { if (Z_TYPE_P(op1) != IS_OBJECT || Z_TYPE_P(op2) != IS_OBJECT || Z_OBJ_HT_P(op1)->compare != Z_OBJ_HT_P(op2)->compare) \
        { return zend_std_compare_objects(op1, op2); } }
#define zend_object_properties_size(ce) \
    (sizeof(zval) * ((ce)->default_properties_count - (((ce)->ce_flags & ZEND_ACC_USE_GUARDS) ? 0 : 1)))
#define ZEND_OBJ_FROM(type, field, obj) ((type *)((char *)(obj) - XtOffsetOf(type, field)))
#define OBJ_RELEASE(obj) zend_object_release(obj)
#define zend_release_properties(ht) do { \
        if ((ht) && !(GC_FLAGS(ht) & GC_IMMUTABLE) && !GC_DELREF(ht)) { zend_array_destroy(ht); } \
    } while (0)
#define zend_free_trampoline(func) ((void)(func))
#define zend_ce_stdClass zend_standard_class_def

/* ------------------------------------------------------------ arrays and objects built by extensions */

#define HASH_OF(p) (Z_TYPE_P(p) == IS_ARRAY ? Z_ARRVAL_P(p) : \
    ((Z_TYPE_P(p) == IS_OBJECT ? Z_OBJ_HT_P(p)->get_properties(Z_OBJ_P(p)) : NULL)))
#define array_init(arg) ZVAL_ARR((arg), zend_new_array(0))
#define array_init_size(arg, size) ZVAL_ARR((arg), zend_new_array(size))
#define add_assoc_long(arg, key, n) add_assoc_long_ex(arg, key, strlen(key), n)
#define add_assoc_null(arg, key) add_assoc_null_ex(arg, key, strlen(key))
#define add_assoc_bool(arg, key, b) add_assoc_bool_ex(arg, key, strlen(key), b)
#define add_assoc_resource(arg, key, r) add_assoc_resource_ex(arg, key, strlen(key), r)
#define add_assoc_double(arg, key, d) add_assoc_double_ex(arg, key, strlen(key), d)
#define add_assoc_str(arg, key, str) add_assoc_str_ex(arg, key, strlen(key), str)
#define add_assoc_string(arg, key, str) add_assoc_string_ex(arg, key, strlen(key), str)
#define add_assoc_stringl(arg, key, str, length) add_assoc_stringl_ex(arg, key, strlen(key), str, length)
#define add_assoc_array(arg, key, arr) add_assoc_array_ex(arg, key, strlen(key), arr)
#define add_assoc_object(arg, key, obj) add_assoc_object_ex(arg, key, strlen(key), obj)
#define add_assoc_reference(arg, key, ref) add_assoc_reference_ex(arg, key, strlen(key), ref)
#define add_assoc_zval(arg, key, value) add_assoc_zval_ex(arg, key, strlen(key), value)
#define add_property_long(arg, key, n) add_property_long_ex(arg, key, strlen(key), n)
#define add_property_null(arg, key) add_property_null_ex(arg, key, strlen(key))
#define add_property_bool(arg, key, b) add_property_bool_ex(arg, key, strlen(key), b)
#define add_property_resource(arg, key, r) add_property_resource_ex(arg, key, strlen(key), r)
#define add_property_double(arg, key, d) add_property_double_ex(arg, key, strlen(key), d)
#define add_property_str(arg, key, str) add_property_str_ex(arg, key, strlen(key), str)
#define add_property_string(arg, key, str) add_property_string_ex(arg, key, strlen(key), str)
#define add_property_stringl(arg, key, str, length) add_property_stringl_ex(arg, key, strlen(key), str, length)
#define add_property_zval(arg, key, value) add_property_zval_ex(arg, key, strlen(key), value)
#define add_property_array(arg, key, arr) add_property_array_ex(arg, key, strlen(key), arr)
#define add_property_object(arg, key, obj) add_property_object_ex(arg, key, strlen(key), obj)

/* ------------------------------------------------------------ exceptions and errors */

#define zend_bailout() _zend_bailout(__FILE__, __LINE__)
#define zend_try { \
        JMP_BUF *__orig_bailout = EG(bailout); \
        JMP_BUF __bailout; \
        EG(bailout) = &__bailout; \
        if (SETJMP(__bailout) == 0) {
#define zend_catch \
        } else { \
            EG(bailout) = __orig_bailout;
#define zend_end_try() \
        } \
        EG(bailout) = __orig_bailout; \
    }
#define zend_first_try EG(bailout) = NULL; zend_try
#define php_error zend_error
#define php_memnstr zend_memnstr
#define php_memnistr zend_memnistr
#define php_sprintf sprintf
#define php_sleep sleep
#define php_log_err(msg) ((void)(msg))
#define error_handling_t zend_error_handling_t

/* ------------------------------------------------------------ printf family */

#define spprintf zend_spprintf
#define vspprintf zend_vspprintf
#define strpprintf zend_strpprintf
#define vstrpprintf zend_vstrpprintf
#define slprintf ap_php_slprintf
#define vslprintf ap_php_vslprintf
#define PHPWRITE(str, str_len) php_write((void *)(str), (str_len))
#define PUTS(str) php_write((void *)(str), strlen(str))
#define PUTC(c) php_write((void *)&(c), 1)
#define ZEND_WRITE(str, str_len) zend_write((str), (str_len))
#define ZEND_PUTS(str) zend_write((str), strlen(str))
#define ZEND_PUTC(c) zend_write(&(c), 1)
#define zend_printf php_printf
#if !defined(__APPLE__) && !(defined(__GLIBC__) && (__GLIBC__ > 2 || __GLIBC_MINOR__ >= 38))
# define strlcpy php_strlcpy
# define strlcat php_strlcat
#endif

/* ------------------------------------------------------------ constants */

#define ZEND_CONSTANT_FLAGS(c) (Z_CONSTANT_FLAGS((c)->value) & 0xff)
#define ZEND_CONSTANT_MODULE_NUMBER(c) (Z_CONSTANT_FLAGS((c)->value) >> 8)
#define ZEND_CONSTANT_SET_FLAGS(c, _flags, _module_number) do { \
        Z_CONSTANT_FLAGS((c)->value) = ((_flags) & 0xff) | ((_module_number) << 8); \
    } while (0)
#define Z_CONSTANT_FLAGS(zval) (zval).u2.constant_flags
#define REGISTER_NULL_CONSTANT(name, flags) zend_register_null_constant((name), sizeof(name) - 1, (flags), module_number)
#define REGISTER_BOOL_CONSTANT(name, bval, flags) zend_register_bool_constant((name), sizeof(name) - 1, (bval), (flags), module_number)
#define REGISTER_LONG_CONSTANT(name, lval, flags) zend_register_long_constant((name), sizeof(name) - 1, (lval), (flags), module_number)
#define REGISTER_DOUBLE_CONSTANT(name, dval, flags) zend_register_double_constant((name), sizeof(name) - 1, (dval), (flags), module_number)
#define REGISTER_STRING_CONSTANT(name, str, flags) zend_register_string_constant((name), sizeof(name) - 1, (str), (flags), module_number)
#define REGISTER_STRINGL_CONSTANT(name, str, len, flags) zend_register_stringl_constant((name), sizeof(name) - 1, (str), (len), (flags), module_number)
#define REGISTER_NS_NULL_CONSTANT(ns, name, flags) zend_register_null_constant(ZEND_NS_NAME(ns, name), sizeof(ZEND_NS_NAME(ns, name)) - 1, (flags), module_number)
#define REGISTER_NS_BOOL_CONSTANT(ns, name, bval, flags) zend_register_bool_constant(ZEND_NS_NAME(ns, name), sizeof(ZEND_NS_NAME(ns, name)) - 1, (bval), (flags), module_number)
#define REGISTER_NS_LONG_CONSTANT(ns, name, lval, flags) zend_register_long_constant(ZEND_NS_NAME(ns, name), sizeof(ZEND_NS_NAME(ns, name)) - 1, (lval), (flags), module_number)
#define REGISTER_NS_DOUBLE_CONSTANT(ns, name, dval, flags) zend_register_double_constant(ZEND_NS_NAME(ns, name), sizeof(ZEND_NS_NAME(ns, name)) - 1, (dval), (flags), module_number)
#define REGISTER_NS_STRING_CONSTANT(ns, name, str, flags) zend_register_string_constant(ZEND_NS_NAME(ns, name), sizeof(ZEND_NS_NAME(ns, name)) - 1, (str), (flags), module_number)
#define REGISTER_NS_STRINGL_CONSTANT(ns, name, str, len, flags) zend_register_stringl_constant(ZEND_NS_NAME(ns, name), sizeof(ZEND_NS_NAME(ns, name)) - 1, (str), (len), (flags), module_number)
#define REGISTER_MAIN_NULL_CONSTANT(name, flags) zend_register_null_constant((name), sizeof(name) - 1, (flags), 0)
#define REGISTER_MAIN_BOOL_CONSTANT(name, bval, flags) zend_register_bool_constant((name), sizeof(name) - 1, (bval), (flags), 0)
#define REGISTER_MAIN_LONG_CONSTANT(name, lval, flags) zend_register_long_constant((name), sizeof(name) - 1, (lval), (flags), 0)
#define REGISTER_MAIN_DOUBLE_CONSTANT(name, dval, flags) zend_register_double_constant((name), sizeof(name) - 1, (dval), (flags), 0)
#define REGISTER_MAIN_STRING_CONSTANT(name, str, flags) zend_register_string_constant((name), sizeof(name) - 1, (str), (flags), 0)
#define REGISTER_MAIN_STRINGL_CONSTANT(name, str, len, flags) zend_register_stringl_constant((name), sizeof(name) - 1, (str), (len), (flags), 0)

/* ------------------------------------------------------------ INI */

#define PHP_INI_USER ZEND_INI_USER
#define PHP_INI_PERDIR ZEND_INI_PERDIR
#define PHP_INI_SYSTEM ZEND_INI_SYSTEM
#define PHP_INI_ALL ZEND_INI_ALL
#define PHP_INI_STAGE_STARTUP ZEND_INI_STAGE_STARTUP
#define PHP_INI_STAGE_SHUTDOWN ZEND_INI_STAGE_SHUTDOWN
#define PHP_INI_STAGE_ACTIVATE ZEND_INI_STAGE_ACTIVATE
#define PHP_INI_STAGE_DEACTIVATE ZEND_INI_STAGE_DEACTIVATE
#define PHP_INI_STAGE_RUNTIME ZEND_INI_STAGE_RUNTIME
#define PHP_INI_STAGE_HTACCESS ZEND_INI_STAGE_HTACCESS
#define PHP_INI_DISPLAY_ORIG ZEND_INI_DISPLAY_ORIG
#define PHP_INI_DISPLAY_ACTIVE ZEND_INI_DISPLAY_ACTIVE
#define ZEND_INI_MH(name) zend_result name(zend_ini_entry *entry, zend_string *new_value, void *mh_arg1, void *mh_arg2, void *mh_arg3, int stage)
#define ZEND_INI_DISP(name) ZEND_COLD void name(zend_ini_entry *ini_entry, int type)
#define PHP_INI_MH ZEND_INI_MH
#define PHP_INI_DISP ZEND_INI_DISP
#define ZEND_INI_BEGIN() static const zend_ini_entry_def ini_entries[] = {
#define ZEND_INI_END() { NULL, NULL, NULL, NULL, NULL, NULL, NULL, 0, 0, 0 } };
#define ZEND_INI_ENTRY3_EX(name, default_value, modifiable, on_modify, arg1, arg2, arg3, displayer) \
    { name, on_modify, arg1, arg2, arg3, default_value, displayer, sizeof(default_value) - 1, sizeof(name) - 1, modifiable },
#define ZEND_INI_ENTRY3(name, default_value, modifiable, on_modify, arg1, arg2, arg3) \
    ZEND_INI_ENTRY3_EX(name, default_value, modifiable, on_modify, arg1, arg2, arg3, NULL)
#define ZEND_INI_ENTRY2_EX(name, default_value, modifiable, on_modify, arg1, arg2, displayer) \
    ZEND_INI_ENTRY3_EX(name, default_value, modifiable, on_modify, arg1, arg2, NULL, displayer)
#define ZEND_INI_ENTRY2(name, default_value, modifiable, on_modify, arg1, arg2) \
    ZEND_INI_ENTRY2_EX(name, default_value, modifiable, on_modify, arg1, arg2, NULL)
#define ZEND_INI_ENTRY1_EX(name, default_value, modifiable, on_modify, arg1, displayer) \
    ZEND_INI_ENTRY3_EX(name, default_value, modifiable, on_modify, arg1, NULL, NULL, displayer)
#define ZEND_INI_ENTRY1(name, default_value, modifiable, on_modify, arg1) \
    ZEND_INI_ENTRY1_EX(name, default_value, modifiable, on_modify, arg1, NULL)
#define ZEND_INI_ENTRY_EX(name, default_value, modifiable, on_modify, displayer) \
    ZEND_INI_ENTRY3_EX(name, default_value, modifiable, on_modify, NULL, NULL, NULL, displayer)
#define ZEND_INI_ENTRY(name, default_value, modifiable, on_modify) \
    ZEND_INI_ENTRY_EX(name, default_value, modifiable, on_modify, NULL)
#define STD_ZEND_INI_ENTRY(name, default_value, modifiable, on_modify, property_name, struct_type, struct_ptr) \
    ZEND_INI_ENTRY2(name, default_value, modifiable, on_modify, (void *)XtOffsetOf(struct_type, property_name), \
                    (void *)&struct_ptr)
#define STD_ZEND_INI_ENTRY_EX(name, default_value, modifiable, on_modify, property_name, struct_type, struct_ptr, displayer) \
    ZEND_INI_ENTRY2_EX(name, default_value, modifiable, on_modify, (void *)XtOffsetOf(struct_type, property_name), \
                       (void *)&struct_ptr, displayer)
#define STD_ZEND_INI_BOOLEAN(name, default_value, modifiable, on_modify, property_name, struct_type, struct_ptr) \
    ZEND_INI_ENTRY3_EX(name, default_value, modifiable, on_modify, (void *)XtOffsetOf(struct_type, property_name), \
                       (void *)&struct_ptr, NULL, zend_ini_boolean_displayer_cb)
#define PHP_INI_BEGIN ZEND_INI_BEGIN
#define PHP_INI_END ZEND_INI_END
#define PHP_INI_ENTRY3_EX ZEND_INI_ENTRY3_EX
#define PHP_INI_ENTRY3 ZEND_INI_ENTRY3
#define PHP_INI_ENTRY2_EX ZEND_INI_ENTRY2_EX
#define PHP_INI_ENTRY2 ZEND_INI_ENTRY2
#define PHP_INI_ENTRY1_EX ZEND_INI_ENTRY1_EX
#define PHP_INI_ENTRY1 ZEND_INI_ENTRY1
#define PHP_INI_ENTRY_EX ZEND_INI_ENTRY_EX
#define PHP_INI_ENTRY ZEND_INI_ENTRY
#define STD_PHP_INI_ENTRY STD_ZEND_INI_ENTRY
#define STD_PHP_INI_ENTRY_EX STD_ZEND_INI_ENTRY_EX
#define STD_PHP_INI_BOOLEAN STD_ZEND_INI_BOOLEAN
#define PHP_INI_BOOLEAN ZEND_INI_BOOLEAN
#define ZEND_INI_BOOLEAN(name, default_value, modifiable, on_modify) \
    ZEND_INI_ENTRY_EX(name, default_value, modifiable, on_modify, zend_ini_boolean_displayer_cb)
#define REGISTER_INI_ENTRIES() zend_register_ini_entries_ex(ini_entries, module_number, type)
#define UNREGISTER_INI_ENTRIES() zend_unregister_ini_entries_ex(module_number, type)
#define DISPLAY_INI_ENTRIES() display_ini_entries(zend_module)
#define REGISTER_PHP_INI_ENTRIES REGISTER_INI_ENTRIES
#define INI_INT(name) zend_ini_long((name), strlen(name), 0)
#define INI_FLT(name) zend_ini_double((name), strlen(name), 0)
#define INI_STR(name) zend_ini_string_ex((name), strlen(name), 0, NULL)
#define INI_BOOL(name) ((bool)INI_INT(name))
#define INI_ORIG_INT(name) zend_ini_long((name), strlen(name), 1)
#define INI_ORIG_FLT(name) zend_ini_double((name), strlen(name), 1)
#define INI_ORIG_STR(name) zend_ini_string((name), strlen(name), 1)
#define INI_ORIG_BOOL(name) ((bool)INI_ORIG_INT(name))
#define cfg_get_entry(name, name_len) zend_get_configuration_directive_str(name, name_len)
#define ZEND_INI_GET_BASE() ((char *)mh_arg2)
#define ZEND_INI_GET_ADDR() (ZEND_INI_GET_BASE() + (size_t)mh_arg1)

/* ------------------------------------------------------------ resources */

#define ZEND_RSRC_DTOR_FUNC(name) void name(zend_resource *res)
#define ZEND_RSRC_DTOR_FUNC_N(name) void name(zend_resource *res)
#define ZEND_REGISTER_RESOURCE(rsrc_result, rsrc_pointer, rsrc_type) \
    ZVAL_RES(rsrc_result, zend_register_resource(rsrc_pointer, rsrc_type))

/* ------------------------------------------------------------ smart strings */

#define SMART_STR_START_SIZE 256
#define SMART_STR_PREALLOC 128
#define SMART_STRING_START_SIZE 255
#define SMART_STRING_PREALLOC 128
#define smart_str_extend(dest, len) smart_str_extend_ex((dest), (len), 0)
#define smart_str_free(str) smart_str_free_ex((str), 0)
#define smart_str_trim_to_size(str) smart_str_trim_to_size_ex((str), 0)
#define smart_str_extract(str) smart_str_extract_ex((str), 0)
#define smart_str_appendc(dest, c) smart_str_appendc_ex((dest), (c), 0)
#define smart_str_appendl(dest, src, len) smart_str_appendl_ex((dest), (src), (len), 0)
#define smart_str_append(dest, src) smart_str_append_ex((dest), (src), 0)
#define smart_str_append_smart_str(dest, src) smart_str_append_smart_str_ex((dest), (src), 0)
#define smart_str_appends_ex(dest, src, what) smart_str_appendl_ex((dest), (src), strlen(src), (what))
#define smart_str_appends(dest, src) smart_str_appendl((dest), (src), strlen(src))
#define smart_str_append_long_ex(dest, num, persistent) smart_str_append_long((dest), (num))
#define smart_str_append_unsigned_ex(dest, num, persistent) smart_str_append_unsigned((dest), (num))
#define smart_str_appendl_unsafe(dest, src, len) smart_str_appendl((dest), (src), (len))
#define smart_str_append_off_t(dest, val) smart_str_append_long((dest), (zend_long)(val))
#define smart_str_sets(dest, src) smart_str_setl((dest), (src), strlen(src))
#define SMART_STRING_DO_REALLOC(d, what) do { (d)->c = (char *)erealloc((d)->c, (d)->a + 1); } while (0)
#define smart_string_free(s) smart_string_free_ex((s), 0)
#define smart_string_appendc(dest, c) smart_string_appendc_ex((dest), (c), 0)
#define smart_string_appendl(dest, src, len) smart_string_appendl_ex((dest), (src), (len), 0)
#define smart_string_appends(dest, src) smart_string_appendl((dest), (src), strlen(src))
#define smart_string_append(dest, src) smart_string_appendl((dest), (src)->c, (src)->len)
#define smart_string_append_long(dest, val) smart_string_append_long_ex((dest), (val), 0)
#define smart_string_append_unsigned(dest, val) smart_string_append_unsigned_ex((dest), (val), 0)
#define smart_string_sets(dest, src) do { (dest)->c = (src); (dest)->len = strlen(src); (dest)->a = (dest)->len; } while (0)

/* ------------------------------------------------------------ phpinfo() */

/* ------------------------------------------------------------ process globals */

#define PG(v) (core_globals.v)
#define SG(v) (sapi_globals.v)
#define sapi_add_header(a, b, c) sapi_add_header_ex((a), (b), (c), 1)
#define BG(v) (basic_globals.v)
#define FG(v) (file_globals.v)

/* ------------------------------------------------------------ serialize() and unserialize() */

#define PHP_VAR_SERIALIZE_INIT(d) (d) = php_var_serialize_init()
#define PHP_VAR_SERIALIZE_DESTROY(d) php_var_serialize_destroy(d)
#define PHP_VAR_UNSERIALIZE_INIT(d) (d) = php_var_unserialize_init()
#define PHP_VAR_UNSERIALIZE_DESTROY(d) php_var_unserialize_destroy(d)
#define PHP_IC_ENTRY php_ce_incomplete_class
#define MAGIC_MEMBER "__PHP_Incomplete_Class_Name"
#define INCOMPLETE_CLASS "__PHP_Incomplete_Class"
#define PHP_CLASS_ATTRIBUTES zend_string *class_name; bool incomplete_class ZEND_ATTRIBUTE_UNUSED = 0
#define PHP_SET_CLASS_ATTRIBUTES(struc) \
    if (Z_OBJCE_P(struc) == php_ce_incomplete_class) { \
        class_name = php_lookup_class_name(Z_OBJ_P(struc)); \
        if (!class_name) { class_name = ZSTR_INIT_LITERAL(INCOMPLETE_CLASS, 0); } \
        incomplete_class = 1; \
    } else { \
        class_name = zend_string_copy(Z_OBJCE_P(struc)->name); \
    }
#define PHP_CLEANUP_CLASS_ATTRIBUTES() zend_string_release_ex(class_name, 0)

/* ------------------------------------------------------------ base64 and digests */

#define PHP_MD5_CTX_SIZE sizeof(PHP_MD5_CTX)
#define PHP_SHA256_SPEC "l8l2b64."
#define PHP_SHA256Init(ctx) PHP_SHA256InitArgs(ctx, NULL)

/* ------------------------------------------------------------ random */

#define php_random_bytes_throw(b, s) php_random_bytes((b), (s), 1)
#define php_random_bytes_silent(b, s) php_random_bytes((b), (s), 0)
#define php_random_int_throw(min, max, result) php_random_int((min), (max), (result), 1)
#define php_random_int_silent(min, max, result) php_random_int((min), (max), (result), 0)
#define GENERATE_SEED() ((zend_long)php_random_generate_fallback_seed())

/* ------------------------------------------------------------ JSON, PCRE, iconv, session */

#define pcre2_match pcre2_match_8
#define pcre2_match_data_create_from_pattern pcre2_match_data_create_from_pattern_8
#define pcre2_match_data_free pcre2_match_data_free_8
#define pcre2_get_ovector_pointer pcre2_get_ovector_pointer_8
#define PS_SERIALIZER_ENCODE_ARGS void
#define PS_SERIALIZER_DECODE_ARGS const char *val, size_t vallen
#define PS_SERIALIZER_ENCODE_FUNC(x) zend_string *ps_srlzr_encode_##x(PS_SERIALIZER_ENCODE_ARGS)
#define PS_SERIALIZER_DECODE_FUNC(x) zend_result ps_srlzr_decode_##x(PS_SERIALIZER_DECODE_ARGS)
#define PS_SERIALIZER_ENCODE_NAME(x) ps_srlzr_encode_##x
#define PS_SERIALIZER_DECODE_NAME(x) ps_srlzr_decode_##x
#define PS_SERIALIZER_FUNCS(x) PS_SERIALIZER_ENCODE_FUNC(x); PS_SERIALIZER_DECODE_FUNC(x)
#define PS_SERIALIZER_ENTRY(x) { #x, ps_srlzr_encode_##x, ps_srlzr_decode_##x }
#define PS_ENCODE_VARS zend_string *key; zend_ulong num_key; zval *struc;
#define PS_ENCODE_LOOP(code) do { \
        HashTable *_ht = Z_ARRVAL_P(Z_REFVAL(PS(http_session_vars))); \
        ZEND_HASH_FOREACH_KEY(_ht, num_key, key) { \
            if (key == NULL) { continue; } \
            if (php_get_session_var(key)) { code; } \
        } ZEND_HASH_FOREACH_END(); \
    } while (0)
#define PS(v) (ps_globals.v)

/* ------------------------------------------------------------ SPL */

/* ------------------------------------------------------------ streams */

#define STREAMS_D
#define STREAMS_C
#define STREAMS_CC
#define STREAMS_DC
#define STREAMS_REL_C
#define STREAMS_REL_CC
#define STREAMS_DC_ZEND_FILE_LINE_DC
#define PHP_STREAM_COPY_ALL ((size_t)-1)
#define php_stream_alloc(ops, thisptr, persistent_id, mode) _php_stream_alloc((ops), (thisptr), (persistent_id), (mode) STREAMS_CC)
#define php_stream_open_wrapper(path, mode, options, opened) \
    _php_stream_open_wrapper_ex((path), (mode), (options), (opened), NULL STREAMS_CC)
#define php_stream_open_wrapper_ex(path, mode, options, opened, context) \
    _php_stream_open_wrapper_ex((path), (mode), (options), (opened), (context) STREAMS_CC)
#define php_stream_free(stream, close_options) _php_stream_free((stream), (close_options))
#define php_stream_close(stream) _php_stream_free((stream), PHP_STREAM_FREE_CLOSE)
#define php_stream_pclose(stream) _php_stream_free((stream), PHP_STREAM_FREE_CLOSE_PERSISTENT)
#define php_stream_read(stream, buf, count) _php_stream_read((stream), (buf), (count))
#define php_stream_write(stream, buf, count) _php_stream_write((stream), (buf), (count))
#define php_stream_write_string(stream, str) _php_stream_write((stream), (str), strlen(str))
#define php_stream_eof(stream) _php_stream_eof((stream))
#define php_stream_getc(stream) _php_stream_getc((stream))
#define php_stream_putc(stream, c) _php_stream_putc((stream), (c))
#define php_stream_flush(stream) _php_stream_flush((stream), 0)
#define php_stream_seek(stream, offset, whence) _php_stream_seek((stream), (offset), (whence))
#define php_stream_tell(stream) _php_stream_tell((stream))
#define php_stream_rewind(stream) _php_stream_seek((stream), 0L, SEEK_SET)
#define php_stream_set_option(stream, option, value, ptrvalue) _php_stream_set_option((stream), (option), (value), (ptrvalue))
#define php_stream_set_chunk_size(stream, size) _php_stream_set_option((stream), PHP_STREAM_OPTION_SET_CHUNK_SIZE, (size), NULL)
#define php_stream_get_line(stream, buf, maxlen, retlen) _php_stream_get_line((stream), (buf), (maxlen), (retlen))
#define php_stream_gets(stream, buf, maxlen) _php_stream_get_line((stream), (buf), (maxlen), NULL)
#define php_stream_copy_to_mem(src, maxlen, persistent) _php_stream_copy_to_mem((src), (maxlen), (persistent) STREAMS_CC)
#define php_stream_copy_to_stream_ex(src, dest, maxlen, len) _php_stream_copy_to_stream_ex((src), (dest), (maxlen), (len) STREAMS_CC)
#define php_stream_stat(stream, ssb) _php_stream_stat((stream), (ssb))
#define php_stream_cast(stream, as, ret, show_err) _php_stream_cast((stream), (as), (ret), (show_err))
#define php_stream_fopen_from_fd(fd, mode, persistent_id) \
    _php_stream_fopen_from_fd((fd), (mode), (persistent_id), 0 STREAMS_CC)
#define php_stream_fopen_from_file(file, mode) _php_stream_fopen_from_file((file), (mode) STREAMS_CC)
#define php_stream_memory_create(mode) _php_stream_memory_create((mode) STREAMS_CC)
#define php_stream_memory_open(mode, str) _php_stream_memory_open((mode), (str) STREAMS_CC)
#define php_stream_memory_get_buffer(stream) _php_stream_memory_get_buffer((stream) STREAMS_CC)
#define php_stream_temp_create(mode, max_memory_usage) _php_stream_temp_create((mode), (max_memory_usage) STREAMS_CC)
#define php_stream_to_zval(stream, zval) do { ZVAL_RES(zval, (stream)->res); (stream)->__exposed = 1; } while (0)
#define php_stream_from_zval(xstr, pzval) \
    do { if (((xstr) = (php_stream *)zend_fetch_resource2_ex((pzval), "stream", php_file_le_stream(), php_file_le_pstream())) == NULL) { \
        return; } } while (0)
#define php_stream_from_res(xstr, res) \
    do { if (((xstr) = (php_stream *)zend_fetch_resource2((res), "stream", php_file_le_stream(), php_file_le_pstream())) == NULL) { \
        return; } } while (0)
#define php_stream_is_persistent(stream) ((stream)->is_persistent)
#define php_stream_is(stream, anops) ((stream)->ops == (anops))
#define PHP_STREAM_IS_STDIO (&php_stream_stdio_ops)
#define php_stream_filter_alloc(fops, thisptr, persistent) _php_stream_filter_alloc((fops), (thisptr), (persistent) STREAMS_CC)
#define php_stream_filter_alloc_rel(fops, thisptr, persistent) _php_stream_filter_alloc((fops), (thisptr), (persistent) STREAMS_REL_CC)
#define php_stream_bucket_addref(bucket) (bucket)->refcount++
#define php_flock flock
#define VCWD_STAT(path, buff) stat(path, buff)
#define VCWD_LSTAT(path, buff) lstat(path, buff)
#define VCWD_UNLINK(path) unlink(path)
#define VCWD_OPEN(path, flags) open(path, flags)
#define VCWD_OPEN_MODE(path, flags, mode) open(path, flags, mode)
#define VCWD_FOPEN(path, mode) fopen(path, mode)
#define VCWD_REALPATH(path, real_path) realpath(path, real_path)
#define VCWD_ACCESS(pathname, mode) access(pathname, mode)
#define VCWD_GETCWD(buff, size) getcwd(buff, size)
#define DEFAULT_SLASH '/'
#define IS_SLASH(c) ((c) == '/')
#define PHP_DIR_SEPARATOR '/'
#define ZEND_PATHS_SEPARATOR ':'
#define php_sys_stat stat
#define php_sys_lstat lstat

/* ------------------------------------------------------------ output */

#define PHPWRITE_H(str, str_len) php_output_write_unbuffered((str), (str_len))

/* ------------------------------------------------------------ odds and ends */

#define zend_get_parameters_array(ht, param_count, argument_array) \
    zend_get_parameters_array_ex(param_count, argument_array)
#define ZEND_SIGNAL_BLOCK_INTERRUPTIONS()
#define ZEND_SIGNAL_UNBLOCK_INTERRUPTIONS()
#define HANDLE_BLOCK_INTERRUPTIONS()
#define HANDLE_UNBLOCK_INTERRUPTIONS()
#define SIGG(v) 0
#define php_ignore_value(x) ((void)(x))
#define php_time() time(NULL)
#define ZEND_EXT_API
#define PHP_ADD_EXTENSION_DEP(ext, dep)
#define php_get_module_initialized() 1
#define php_during_module_startup() 0
#define php_during_module_shutdown() 0
#define PHP_DOUBLE_MAX_LENGTH ZEND_DOUBLE_MAX_LENGTH
#define php_math_is_finite(value) isfinite(value)

#endif /* ELEPHC_ZEND_MACROS_H */
