/* php_elephc_demo.h — the fixture extension hosted by tests/php_ext_tests.rs. */
#ifndef PHP_ELEPHC_DEMO_H
#define PHP_ELEPHC_DEMO_H

extern zend_module_entry elephc_demo_module_entry;
#define phpext_elephc_demo_ptr &elephc_demo_module_entry

#define PHP_ELEPHC_DEMO_VERSION "1.2.3"

ZEND_BEGIN_MODULE_GLOBALS(elephc_demo)
    char *greeting;
    zend_long calls;
    HashTable kept;
ZEND_END_MODULE_GLOBALS(elephc_demo)

ZEND_EXTERN_MODULE_GLOBALS(elephc_demo)
#define DEMO_G(v) ZEND_MODULE_GLOBALS_ACCESSOR(elephc_demo, v)

extern zend_class_entry *demo_exception_ce;
extern zend_class_entry *demo_parse_exception_ce;

/* demo_values.c */
PHP_FUNCTION(demo_echo);
PHP_FUNCTION(demo_nested);
PHP_FUNCTION(demo_object);
PHP_FUNCTION(demo_keep);
PHP_FUNCTION(demo_kept);

#endif
