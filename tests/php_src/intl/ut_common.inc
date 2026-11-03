<?php
class __ElephcIntlMode { public static bool $oo = false; }
/*
 * Run unit test in OO- and in procedural mode.
 * Then compare the outputs.
 * It they're equal then show one of them.
 * Otherwise indicate an error.
 */
function ut_run()
{
    // Run unit test in OO mode.
    \__ElephcIntlMode::$oo = true;
    $oo_result = ut_main();

    // Run unit test in procedural mode.
    \__ElephcIntlMode::$oo = false;
    $proc_result = ut_main();

    // Show error if the APIs produce different results.
    if( $proc_result !== $oo_result )
    {
          echo "ERROR: OO- and procedural APIs produce different results!\n";
          echo "OO API output:\n";
          echo str_repeat( '=', 78 ) . "\n";
          echo $oo_result;
          echo str_repeat( '=', 78 ) . "\n";
          echo "procedural API output:\n";
          echo str_repeat( '=', 78 ) . "\n";
          echo $proc_result;
          echo str_repeat( '=', 78 ) . "\n";
          return;
    }

    // Else, if the results are equal, show one of them.
    echo $proc_result;
}

function dump( $val )
{
    return var_export( $val, true );
}

/*
 * Wrappers around Collator methods to run them in either OO- or procedural mode.
 */

function ut_coll_create(string $locale): Collator
{
    $__object = \__ElephcIntlMode::$oo ? Collator::create( $locale ) : collator_create( $locale );
    if ($__object === null) {
        throw new IntlException("ut_coll_create(): creation failed");
    }
    if ($__object instanceof Collator) {
        return $__object;
    }
    throw new IntlException("ut_coll_create(): creation failed");
}
function ut_coll_compare(Collator $coll, string $str1, string $str2): int|false
{
    return \__ElephcIntlMode::$oo ? $coll->compare( $str1, $str2 ) : collator_compare( $coll, $str1, $str2 );
}
function ut_coll_sort(Collator $coll, mixed &$arr, int $sort_flag = Collator::SORT_REGULAR): bool
{
    return \__ElephcIntlMode::$oo ? $coll->sort( $arr, $sort_flag ) : collator_sort( $coll, $arr, $sort_flag );
}
function ut_coll_sort_with_sort_keys(Collator $coll, mixed &$arr): bool
{
    return \__ElephcIntlMode::$oo ? $coll->sortWithSortKeys( $arr ) : collator_sort_with_sort_keys( $coll, $arr );
}
function ut_coll_get_sort_key(Collator $coll, string $str): string|false
{
    return \__ElephcIntlMode::$oo ? $coll->getSortKey( $str ) : collator_get_sort_key( $coll, $str );
}
function ut_coll_asort(Collator $coll, mixed &$arr, int $sort_flag = Collator::SORT_REGULAR): bool
{
    return \__ElephcIntlMode::$oo ? $coll->asort( $arr, $sort_flag ) : collator_asort( $coll, $arr, $sort_flag );
}
function ut_coll_get_locale(Collator $coll, int $type): string|false
{
    return \__ElephcIntlMode::$oo ? $coll->getLocale( $type ) : collator_get_locale( $coll, $type );
}


function ut_coll_get_attribute(Collator $coll, int $attr): int|false
{
    return \__ElephcIntlMode::$oo ? $coll->getAttribute( $attr ) : collator_get_attribute( $coll, $attr );
}
function ut_coll_get_strength(Collator $coll): int
{
    return \__ElephcIntlMode::$oo ? $coll->getStrength() : collator_get_strength( $coll );
}
function ut_coll_set_strength(Collator $coll, int $strength): bool
{
    return \__ElephcIntlMode::$oo ? $coll->setStrength( $strength ) : collator_set_strength( $coll, $strength );
}
function ut_coll_set_attribute(Collator $coll, int $attr, int $val): bool
{
    return \__ElephcIntlMode::$oo ? $coll->setAttribute( $attr, $val ) : collator_set_attribute( $coll, $attr, $val );
}



function ut_coll_get_error_code(Collator $coll): int|false
{
    return \__ElephcIntlMode::$oo ? $coll->getErrorCode() : collator_get_error_code( $coll );
}
function ut_coll_get_error_message(Collator $coll): string|false
{
    return \__ElephcIntlMode::$oo ? $coll->getErrorMessage() : collator_get_error_message( $coll );
}



/*
 * Wrappers around NumberFormatter methods to run them in either OO- or procedural mode.
 */

// FIXME: incomplete list

function ut_nfmt_create(string $locale, int $style, ?string $pattern = null): ?NumberFormatter
{
    if (\__ElephcIntlMode::$oo) {
        try {
            $__object = new NumberFormatter($locale, $style, $pattern);
        } catch (Exception $e) {
            return null;
        }
    } else {
        $__object = numfmt_create($locale, $style, $pattern);
    }
    return $__object instanceof NumberFormatter ? $__object : null;
}
function ut_nfmt_format(NumberFormatter $fmt, int|float $number, int $type = NumberFormatter::TYPE_DEFAULT): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->format( $number, $type ) : numfmt_format( $fmt, $number, $type );
}
function ut_nfmt_parse(NumberFormatter $fmt, string $string, int $type = NumberFormatter::TYPE_DOUBLE, mixed &$position = null): int|float|false
{
    if(is_null($position)) {
    return \__ElephcIntlMode::$oo ? $fmt->parse( $string, $type ) : numfmt_parse( $fmt, $string, $type );
    } else {
    return \__ElephcIntlMode::$oo ? $fmt->parse( $string, $type, $position ) : numfmt_parse( $fmt, $string, $type, $position );
    }
}
function ut_nfmt_format_currency(NumberFormatter $fmt, float $number, string $currency): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->formatCurrency( $number, $currency ) : numfmt_format_currency( $fmt, $number, $currency );
}
function ut_nfmt_parse_currency(NumberFormatter $fmt, string $string, mixed &$currency, mixed &$position = null): float|false
{
    if(is_null($position)) {
    return \__ElephcIntlMode::$oo ? $fmt->parseCurrency( $string, $currency ) : numfmt_parse_currency( $fmt, $string, $currency );
    } else {
    return \__ElephcIntlMode::$oo ? $fmt->parseCurrency( $string, $currency, $position ) : numfmt_parse_currency( $fmt, $string, $currency, $position );
   }
}
function ut_nfmt_set_attribute(NumberFormatter $fmt, int $attribute, int|float $value): bool
{
    return \__ElephcIntlMode::$oo ? $fmt->setAttribute( $attribute, $value ) : numfmt_set_attribute( $fmt, $attribute, $value );
}
function ut_nfmt_set_text_attribute(NumberFormatter $fmt, int $attribute, string $value): bool
{
    return \__ElephcIntlMode::$oo ? $fmt->setTextAttribute( $attribute, $value ) : numfmt_set_text_attribute( $fmt, $attribute, $value );
}
function ut_nfmt_set_symbol(NumberFormatter $fmt, int $attribute, string $value): bool
{
    return \__ElephcIntlMode::$oo ? $fmt->setSymbol( $attribute, $value ) : numfmt_set_symbol( $fmt, $attribute, $value );
}
function ut_nfmt_set_pattern(NumberFormatter $fmt, string $pattern): bool
{
    return \__ElephcIntlMode::$oo ? $fmt->setPattern( $pattern ) : numfmt_set_pattern( $fmt, $pattern );
}
function ut_nfmt_get_attribute(NumberFormatter $fmt, int $attribute): int|float|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getAttribute( $attribute ) : numfmt_get_attribute( $fmt, $attribute );
}
function ut_nfmt_get_text_attribute(NumberFormatter $fmt, int $attribute): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getTextAttribute( $attribute ) : numfmt_get_text_attribute( $fmt, $attribute );
}
function ut_nfmt_get_symbol(NumberFormatter $fmt, int $attribute): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getSymbol( $attribute ) : numfmt_get_symbol( $fmt, $attribute );
}
function ut_nfmt_get_pattern(NumberFormatter $fmt): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getPattern() : numfmt_get_pattern( $fmt );
}
function ut_nfmt_get_locale(NumberFormatter $fmt, int $type = 0): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getLocale( $type ) : numfmt_get_locale( $fmt, $type );
}
function ut_nfmt_get_error_code(NumberFormatter $fmt): int
{
    return \__ElephcIntlMode::$oo ? $fmt->getErrorCode() : numfmt_get_error_code( $fmt );
}
function ut_nfmt_get_error_message(NumberFormatter $fmt): string
{
    return \__ElephcIntlMode::$oo ? $fmt->getErrorMessage() : numfmt_get_error_message( $fmt );
}

function ut_norm_normalize(string $str, int $form): string|false
{
    return \__ElephcIntlMode::$oo ? Normalizer::normalize( $str, $form ) : normalizer_normalize( $str, $form );
}
function ut_norm_is_normalized(string $str, int $form): bool
{
    return \__ElephcIntlMode::$oo ? Normalizer::isNormalized( $str, $form ) : normalizer_is_normalized( $str, $form );
}
function ut_norm_get_raw_decomposition(string $str, int $form): ?string
{
    return \__ElephcIntlMode::$oo ? Normalizer::getRawDecomposition( $str, $form ) : normalizer_get_raw_decomposition( $str, $form );
}

/*
 * Wrappers around Collator methods to run them in either OO- or procedural mode.
 */

function ut_loc_get_default(): string
{
    return \__ElephcIntlMode::$oo ? Locale::getDefault( ) : locale_get_default();
}
function ut_loc_set_default(string $locale): bool
{
    return \__ElephcIntlMode::$oo ? Locale::setDefault( $locale  ) : locale_set_default( $locale );
}
function ut_loc_get_primary_language(string $locale): ?string
{
    return \__ElephcIntlMode::$oo ? Locale::getPrimaryLanguage( $locale ) : locale_get_primary_language( $locale );
}
function ut_loc_get_script(string $locale): ?string
{
    return \__ElephcIntlMode::$oo ? Locale::getScript( $locale ) : locale_get_script( $locale );
}
function ut_loc_get_region(string $locale): ?string
{
    return \__ElephcIntlMode::$oo ? Locale::getRegion( $locale ) : locale_get_region( $locale );
}
function ut_loc_get_keywords(string $locale): array|false|null
{
    return \__ElephcIntlMode::$oo ? Locale::getKeywords( $locale ) : locale_get_keywords( $locale );
}
function ut_loc_get_display_name(string $locale, ?string $dispLocale): string|false
{
    return \__ElephcIntlMode::$oo ? Locale::getDisplayName( $locale , $dispLocale ) : locale_get_display_name( $locale , $dispLocale );
}
function ut_loc_get_display_language(string $locale, ?string $dispLocale): string|false
{
    return \__ElephcIntlMode::$oo ? Locale::getDisplayLanguage( $locale , $dispLocale ) : locale_get_display_language( $locale , $dispLocale );
}
function ut_loc_get_display_script(string $locale, ?string $dispLocale): string|false
{
    return \__ElephcIntlMode::$oo ? Locale::getDisplayScript( $locale , $dispLocale ) : locale_get_display_script( $locale , $dispLocale );
}
function ut_loc_get_display_region(string $locale, ?string $dispLocale): string|false
{
    return \__ElephcIntlMode::$oo ? Locale::getDisplayRegion( $locale, $dispLocale  ) : locale_get_display_region( $locale, $dispLocale  );
}
function ut_loc_get_display_variant(string $locale, ?string $dispLocale): string|false
{
    return \__ElephcIntlMode::$oo ? Locale::getDisplayVariant( $locale , $dispLocale ) : locale_get_display_variant( $locale, $dispLocale  );
}
function ut_loc_locale_compose(mixed $loc_parts_arr): string|false
{
    $__parts = (array) $loc_parts_arr;
    return \__ElephcIntlMode::$oo ? Locale::composeLocale( $__parts ) : locale_compose( $__parts );
}
function ut_loc_locale_parse(string $locale): ?array
{
    $__value = \__ElephcIntlMode::$oo ? Locale::parseLocale( $locale ) : locale_parse($locale );
    if ($__value === null) {
        return null;
    }
    return (array) $__value;
}
function ut_loc_locale_get_all_variants(string $locale): ?array
{
    $__value = \__ElephcIntlMode::$oo ? Locale::getAllVariants( $locale ) : locale_get_all_variants( $locale );
    if ($__value === null) {
        return null;
    }
    return (array) $__value;
}
function ut_loc_locale_filter_matches(string $lang_tag, mixed $loc_range, bool $isCanonical): ?bool
{
    $__range = (string) $loc_range;
    return \__ElephcIntlMode::$oo ? Locale::filterMatches( $lang_tag,$__range ,$isCanonical) : locale_filter_matches( $lang_tag,$__range ,$isCanonical);
}
function ut_loc_canonicalize(string $locale): ?string
{
    return \__ElephcIntlMode::$oo ? Locale::canonicalize( $locale ) : locale_canonicalize( $locale );
}
function ut_loc_locale_lookup(array $lang_tag_arr, mixed $loc_range, bool $isCanonical, ?string $default_loc): ?string
{
    $__range = (string) $loc_range;
    return \__ElephcIntlMode::$oo ? Locale::lookup( $lang_tag_arr,$__range,$isCanonical,$default_loc ) : locale_lookup( $lang_tag_arr,$__range,$isCanonical,$default_loc );
}
function ut_loc_accept_http(string $http): string|false {
    return \__ElephcIntlMode::$oo ? Locale::acceptFromHttp($http):locale_accept_from_http($http);
}
/* MessageFormatter functions */
function ut_msgfmt_create(string $locale, string $pattern): MessageFormatter
{
    $__object = \__ElephcIntlMode::$oo ? MessageFormatter::create( $locale, $pattern ) : msgfmt_create( $locale, $pattern );
    if ($__object === null) {
        throw new IntlException("ut_msgfmt_create(): creation failed");
    }
    if ($__object instanceof MessageFormatter) {
        return $__object;
    }
    throw new IntlException("ut_msgfmt_create(): creation failed");
}
function ut_msgfmt_format(MessageFormatter $fmt, array $args): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->format( $args ) : msgfmt_format( $fmt, $args);
}
function ut_msgfmt_parse(MessageFormatter $fmt, string $string): array|false
{
    return \__ElephcIntlMode::$oo ? $fmt->parse( $string) : msgfmt_parse( $fmt, $string);
}
function ut_msgfmt_format_message(string $locale, string $pattern, array $args): string|false
{
    return \__ElephcIntlMode::$oo ? MessageFormatter::formatMessage( $locale, $pattern, $args ) : msgfmt_format_message( $locale, $pattern, $args );
}
function ut_msgfmt_parse_message(string $locale, string $pattern, string $string): array|false
{
    return \__ElephcIntlMode::$oo ? MessageFormatter::parseMessage( $locale, $pattern, $string ) : msgfmt_parse_message( $locale, $pattern, $string );
}
function ut_msgfmt_set_pattern(MessageFormatter $fmt, string $pattern): bool
{
    return \__ElephcIntlMode::$oo ? $fmt->setPattern( $pattern ) : msgfmt_set_pattern( $fmt, $pattern );
}
function ut_msgfmt_get_pattern(MessageFormatter $fmt): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getPattern() : msgfmt_get_pattern( $fmt );
}
function ut_msgfmt_get_locale(MessageFormatter $fmt): string
{
    return \__ElephcIntlMode::$oo ? $fmt->getLocale( ) : msgfmt_get_locale( $fmt );
}
function ut_msgfmt_get_error_code(MessageFormatter $fmt): int
{
    return \__ElephcIntlMode::$oo ? $fmt->getErrorCode() : msgfmt_get_error_code( $fmt );
}
function ut_msgfmt_get_error_message(MessageFormatter $fmt): string
{
    return \__ElephcIntlMode::$oo ? $fmt->getErrorMessage() : msgfmt_get_error_message( $fmt );
}
/* IntlDateFormatter functions */
function ut_datefmt_create(?string $locale, int $datetype, int $timetype, IntlTimeZone|DateTimeZone|string|null $timezone = null, IntlCalendar|int|null $calendar = null, ?string $pattern = null): IntlDateFormatter
{
    $__object = \__ElephcIntlMode::$oo ? datefmt_create( $locale, $datetype, $timetype, $timezone, $calendar ,$pattern ) : datefmt_create( $locale, $datetype, $timetype, $timezone, $calendar ,$pattern);
    if ($__object === null) {
        throw new IntlException("ut_datefmt_create(): creation failed");
    }
    if ($__object instanceof IntlDateFormatter) {
        return $__object;
    }
    throw new IntlException("ut_datefmt_create(): creation failed");
}
function ut_datefmt_get_datetype(IntlDateFormatter $fmt): int|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getDateType( ) : datefmt_get_datetype( $fmt );
}
function ut_datefmt_get_timetype(IntlDateFormatter $fmt): int|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getTimeType( ) : datefmt_get_timetype( $fmt );
}
function ut_datefmt_get_calendar(IntlDateFormatter $fmt): int|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getCalendar( ) : datefmt_get_calendar( $fmt );
}
function ut_datefmt_set_calendar(IntlDateFormatter $fmt, IntlCalendar|int|null $calendar): bool
{
    return \__ElephcIntlMode::$oo ? $fmt->setCalendar( $calendar ) : datefmt_set_calendar( $fmt , $calendar );
}
function ut_datefmt_get_timezone_id(IntlDateFormatter $fmt): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getTimeZoneId( ) : datefmt_get_timezone_id( $fmt );
}
function ut_datefmt_set_timezone_id(IntlDateFormatter $fmt, IntlTimeZone|DateTimeZone|string|null $timezone_id): bool
{
    return \__ElephcIntlMode::$oo ? $fmt->setTimeZone( $timezone_id ) : datefmt_set_timezone( $fmt ,$timezone_id);
}
function ut_datefmt_get_pattern(IntlDateFormatter $fmt): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getPattern() : datefmt_get_pattern( $fmt );
}
function ut_datefmt_set_pattern(IntlDateFormatter $fmt, string $pattern): bool
{
    return \__ElephcIntlMode::$oo ? $fmt->setPattern( $pattern ) : datefmt_set_pattern( $fmt ,  $pattern);
}
function ut_datefmt_get_locale(IntlDateFormatter $fmt, int $type = 0): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->getLocale($type ) : datefmt_get_locale( $fmt ,$type);
}
function ut_datefmt_is_lenient(IntlDateFormatter $fmt): bool
{
    return \__ElephcIntlMode::$oo ? $fmt->isLenient() : datefmt_is_lenient( $fmt );
}
function ut_datefmt_set_lenient(IntlDateFormatter $fmt, bool $lenient): void
{
    if (\__ElephcIntlMode::$oo) {
        $fmt->setLenient($lenient);
    } else {
        datefmt_set_lenient($fmt, $lenient);
    }
}
function ut_datefmt_format(IntlDateFormatter $fmt, mixed $value): string|false
{
    return \__ElephcIntlMode::$oo ? $fmt->format(  $value ) : datefmt_format( $fmt ,  $value);
}
function ut_datefmt_parse(IntlDateFormatter $fmt, string $value, mixed &$parse_pos = 0): int|float|false
{
    return \__ElephcIntlMode::$oo ? $fmt->parse(  $value ,$parse_pos ) : datefmt_parse( $fmt ,  $value,$parse_pos);
}
function ut_datefmt_localtime(IntlDateFormatter $fmt, string $value, mixed &$parse_pos = 0): array|false
{
    return \__ElephcIntlMode::$oo ? $fmt->localtime(  $value , $parse_pos ) : datefmt_localtime( $fmt ,  $value , $parse_pos );
}

function ut_resourcebundle_create(?string $locale, ?string $bundle, bool $fallback = true): ResourceBundle
{
    $__object = \__ElephcIntlMode::$oo ? new ResourceBundle($locale, $bundle, $fallback) : resourcebundle_create($locale, $bundle, $fallback);
    if ($__object === null) {
        throw new IntlException("ut_resourcebundle_create(): creation failed");
    }
    if ($__object instanceof ResourceBundle) {
        return $__object;
    }
    throw new IntlException("ut_resourcebundle_create(): creation failed");
}
function ut_resourcebundle_count(ResourceBundle $bundle): int
{
    return \__ElephcIntlMode::$oo ? $bundle->count():resourcebundle_count($bundle);
}
function ut_resourcebundle_locales(string $bundle): array|false
{
    return \__ElephcIntlMode::$oo ? ResourceBundle::getLocales($bundle):resourcebundle_locales($bundle);
}
function ut_resourcebundle_get(ResourceBundle $bundle, string|int $idx): ResourceBundle|array|string|int|null
{
    return \__ElephcIntlMode::$oo ? $bundle->get($idx):resourcebundle_get($bundle, $idx);
}
function ut_resourcebundle_get_error_code(ResourceBundle $bundle): int
{
    return \__ElephcIntlMode::$oo ? $bundle->getErrorCode():resourcebundle_get_error_code($bundle);
}
function ut_resourcebundle_get_error_message(ResourceBundle $bundle): string
{
    return \__ElephcIntlMode::$oo ? $bundle->getErrorMessage():resourcebundle_get_error_message($bundle);
}
