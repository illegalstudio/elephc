/* A hostile sibling extension retains a Fiber and resumes it after elephc's RSHUTDOWN. */
#include <php.h>
#include <Zend/zend_interfaces.h>
#include <stdatomic.h>

#ifdef ZTS
ZEND_TSRMLS_CACHE_DEFINE()
#endif

ZEND_BEGIN_MODULE_GLOBALS(elephc_late_resume)
    zval fiber;
    bool resume;
ZEND_END_MODULE_GLOBALS(elephc_late_resume)
ZEND_DECLARE_MODULE_GLOBALS(elephc_late_resume)
#define LATE_GLOBAL(member) ZEND_MODULE_GLOBALS_ACCESSOR(elephc_late_resume, member)

static atomic_ulong late_resume_count;

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_elephc_late_hold, 0, 2, IS_VOID, 0)
    ZEND_ARG_TYPE_INFO(0, fiber, IS_OBJECT, 0)
    ZEND_ARG_TYPE_INFO(0, resume, _IS_BOOL, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_elephc_late_resume_count, 0, 0, IS_LONG, 0)
ZEND_END_ARG_INFO()

/* Keep an independent request-owned reference after the symbol table is destroyed. */
static PHP_FUNCTION(elephc_late_hold)
{
    (void)return_value;
    zval *fiber;
    bool resume;
    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_OBJECT(fiber)
        Z_PARAM_BOOL(resume)
    ZEND_PARSE_PARAMETERS_END();
    if (!Z_ISUNDEF(LATE_GLOBAL(fiber))) { zval_ptr_dtor(&LATE_GLOBAL(fiber)); }
    ZVAL_COPY(&LATE_GLOBAL(fiber), fiber);
    LATE_GLOBAL(resume) = resume;
}

/* Expose process-wide proof to the next request without retaining Zend values. */
static PHP_FUNCTION(elephc_late_resume_count)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_LONG((zend_long)atomic_load(&late_resume_count));
}

static const zend_function_entry late_resume_functions[] = {
    PHP_FE(elephc_late_hold, arginfo_elephc_late_hold)
    PHP_FE(elephc_late_resume_count, arginfo_elephc_late_resume_count)
    PHP_FE_END
};

static PHP_GINIT_FUNCTION(elephc_late_resume)
{
    ZVAL_UNDEF(&elephc_late_resume_globals->fiber);
    elephc_late_resume_globals->resume = false;
}

static PHP_RINIT_FUNCTION(elephc_late_resume)
{
    (void)type;
    (void)module_number;
#ifdef ZTS
    ZEND_TSRMLS_CACHE_UPDATE();
#endif
    ZVAL_UNDEF(&LATE_GLOBAL(fiber));
    LATE_GLOBAL(resume) = false;
    return SUCCESS;
}

/* Loaded before the produced module, this callback runs after its RSHUTDOWN. */
static PHP_RSHUTDOWN_FUNCTION(elephc_late_resume)
{
    (void)type;
    (void)module_number;
    if (!Z_ISUNDEF(LATE_GLOBAL(fiber)) && LATE_GLOBAL(resume)) {
        zval result;
        ZVAL_UNDEF(&result);
        zval *returned = zend_call_method_with_0_params(Z_OBJ(LATE_GLOBAL(fiber)),
            Z_OBJCE(LATE_GLOBAL(fiber)), NULL, "resume", &result);
        if (returned && !EG(exception)) { atomic_fetch_add(&late_resume_count, 1); }
        if (!Z_ISUNDEF(result)) { zval_ptr_dtor(&result); }
        zval_ptr_dtor(&LATE_GLOBAL(fiber));
        ZVAL_UNDEF(&LATE_GLOBAL(fiber));
    }
    return SUCCESS;
}

/* Reverse load order puts this after the produced module's post-deactivate hook. */
static ZEND_MODULE_POST_ZEND_DEACTIVATE_D(elephc_late_resume)
{
    if (!Z_ISUNDEF(LATE_GLOBAL(fiber))) {
        zval_ptr_dtor(&LATE_GLOBAL(fiber));
        ZVAL_UNDEF(&LATE_GLOBAL(fiber));
    }
    LATE_GLOBAL(resume) = false;
    return SUCCESS;
}

static zend_module_entry elephc_late_resume_module_entry = {
    STANDARD_MODULE_HEADER,
    "elephc_late_resume",
    late_resume_functions,
    NULL,
    NULL,
    PHP_RINIT(elephc_late_resume),
    PHP_RSHUTDOWN(elephc_late_resume),
    NULL,
    "0.1.0-test",
    ZEND_MODULE_GLOBALS(elephc_late_resume),
    PHP_GINIT(elephc_late_resume),
    NULL,
    ZEND_MODULE_POST_ZEND_DEACTIVATE_N(elephc_late_resume),
    STANDARD_MODULE_PROPERTIES_EX
};

ZEND_GET_MODULE(elephc_late_resume)
