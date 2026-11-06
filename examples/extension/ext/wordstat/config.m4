dnl config.m4 for the wordstat example extension — the same file phpize reads.

PHP_ARG_ENABLE([wordstat],
  [whether to enable wordstat support],
  [AS_HELP_STRING([--enable-wordstat], [Enable wordstat support])])

if test "$PHP_WORDSTAT" != "no"; then
  AC_DEFINE(HAVE_WORDSTAT, 1, [Whether wordstat is enabled])
  PHP_NEW_EXTENSION(wordstat, wordstat.c, $ext_shared)
fi
