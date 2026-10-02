dnl config.m4 for the elephc_demo fixture extension.
dnl
dnl Written the way a PECL extension's is, so hosting evaluates it through the
dnl same paths: an option with a default, a source variable, and a define set
dnl only on the branch the default selects.

PHP_ARG_ENABLE([elephc_demo],
  [whether to enable the elephc_demo fixture],
  [AS_HELP_STRING([--enable-elephc-demo], [Enable the elephc_demo fixture])])

PHP_ARG_ENABLE([elephc-demo-loud],
  [whether the fixture greets loudly],
  [AS_HELP_STRING([--enable-elephc-demo-loud], [Greet loudly])],
  [no],
  [no])

if test "$PHP_ELEPHC_DEMO" != "no"; then
  AS_VAR_IF([PHP_ELEPHC_DEMO_LOUD], [no],
    [AC_DEFINE([DEMO_QUIET], [1], [Greet quietly])],
    [AC_DEFINE([DEMO_LOUD], [1], [Greet loudly])])

  demo_sources="demo.c \
                demo_values.c"

  AC_DEFINE(HAVE_ELEPHC_DEMO, 1, [Whether the fixture is enabled])
  PHP_NEW_EXTENSION(elephc_demo, $demo_sources, $ext_shared)
fi
