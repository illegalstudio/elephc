/* Registration uses the real PHP host's API and module ABI. */
#include <php.h>
#include <ext/standard/info.h>

#ifdef ZTS
ZEND_TSRMLS_CACHE_DEFINE()
#endif

static const zend_function_entry elephc_functions[] = {
    ZEND_FE_END
};

static PHP_MINIT_FUNCTION(@MODULE@)
{
    (void)type;
    (void)module_number;
    return SUCCESS;
}

static PHP_MSHUTDOWN_FUNCTION(@MODULE@)
{
    (void)type;
    (void)module_number;
    return SUCCESS;
}

static PHP_RINIT_FUNCTION(@MODULE@)
{
    (void)type;
    (void)module_number;
#ifdef ZTS
    ZEND_TSRMLS_CACHE_UPDATE();
#endif
    return SUCCESS;
}

static PHP_RSHUTDOWN_FUNCTION(@MODULE@)
{
    (void)type;
    (void)module_number;
    return SUCCESS;
}

static PHP_MINFO_FUNCTION(@MODULE@)
{
    (void)zend_module;
    php_info_print_table_start();
    php_info_print_table_row(2, "@MODULE@ support", "enabled");
    php_info_print_table_row(2, "Module version", "@VERSION@");
    php_info_print_table_row(2, "PHP module build", ZEND_MODULE_BUILD_ID);
    php_info_print_table_end();
}

static zend_module_entry @MODULE@_module_entry = {
    STANDARD_MODULE_HEADER,
    "@MODULE@",
    elephc_functions,
    PHP_MINIT(@MODULE@),
    PHP_MSHUTDOWN(@MODULE@),
    PHP_RINIT(@MODULE@),
    PHP_RSHUTDOWN(@MODULE@),
    PHP_MINFO(@MODULE@),
    "@VERSION@",
    STANDARD_MODULE_PROPERTIES
};

ZEND_GET_MODULE(@MODULE@)
