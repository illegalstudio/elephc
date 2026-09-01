/*
 * Compile-time ABI contract between the vendored timelib headers and Rust's
 * `timelib_ffi` mirrors. This translation unit emits no runtime code.
 */

#include <stddef.h>

#include "timelib.h"

typedef __typeof__(((timelib_rel_time *)0)->special) elephc_timelib_relative_special;
typedef __typeof__(((timelib_tzinfo *)0)->_bit32) elephc_timelib_tz_counts32;
typedef __typeof__(((timelib_tzinfo *)0)->bit64) elephc_timelib_tz_counts64;

_Static_assert(_Alignof(elephc_timelib_relative_special) == 8, "timelib_rel_time.special alignment");
_Static_assert(sizeof(elephc_timelib_relative_special) == 16, "timelib_rel_time.special size");
_Static_assert(offsetof(elephc_timelib_relative_special, type) == 0, "timelib_rel_time.special.type");
_Static_assert(offsetof(elephc_timelib_relative_special, amount) == 8, "timelib_rel_time.special.amount");

_Static_assert(_Alignof(timelib_rel_time) == 8, "timelib_rel_time alignment");
_Static_assert(sizeof(timelib_rel_time) == 104, "timelib_rel_time size");
_Static_assert(offsetof(timelib_rel_time, y) == 0, "timelib_rel_time.y");
_Static_assert(offsetof(timelib_rel_time, m) == 8, "timelib_rel_time.m");
_Static_assert(offsetof(timelib_rel_time, d) == 16, "timelib_rel_time.d");
_Static_assert(offsetof(timelib_rel_time, h) == 24, "timelib_rel_time.h");
_Static_assert(offsetof(timelib_rel_time, i) == 32, "timelib_rel_time.i");
_Static_assert(offsetof(timelib_rel_time, s) == 40, "timelib_rel_time.s");
_Static_assert(offsetof(timelib_rel_time, us) == 48, "timelib_rel_time.us");
_Static_assert(offsetof(timelib_rel_time, weekday) == 56, "timelib_rel_time.weekday");
_Static_assert(offsetof(timelib_rel_time, weekday_behavior) == 60, "timelib_rel_time.weekday_behavior");
_Static_assert(offsetof(timelib_rel_time, first_last_day_of) == 64, "timelib_rel_time.first_last_day_of");
_Static_assert(offsetof(timelib_rel_time, invert) == 68, "timelib_rel_time.invert");
_Static_assert(offsetof(timelib_rel_time, days) == 72, "timelib_rel_time.days");
_Static_assert(offsetof(timelib_rel_time, special) == 80, "timelib_rel_time.special");
_Static_assert(offsetof(timelib_rel_time, have_weekday_relative) == 96, "timelib_rel_time.have_weekday_relative");
_Static_assert(offsetof(timelib_rel_time, have_special_relative) == 100, "timelib_rel_time.have_special_relative");

_Static_assert(_Alignof(elephc_timelib_tz_counts32) == 4, "timelib_tzinfo._bit32 alignment");
_Static_assert(sizeof(elephc_timelib_tz_counts32) == 24, "timelib_tzinfo._bit32 size");
_Static_assert(offsetof(elephc_timelib_tz_counts32, ttisgmtcnt) == 0, "timelib_tzinfo._bit32.ttisgmtcnt");
_Static_assert(offsetof(elephc_timelib_tz_counts32, ttisstdcnt) == 4, "timelib_tzinfo._bit32.ttisstdcnt");
_Static_assert(offsetof(elephc_timelib_tz_counts32, leapcnt) == 8, "timelib_tzinfo._bit32.leapcnt");
_Static_assert(offsetof(elephc_timelib_tz_counts32, timecnt) == 12, "timelib_tzinfo._bit32.timecnt");
_Static_assert(offsetof(elephc_timelib_tz_counts32, typecnt) == 16, "timelib_tzinfo._bit32.typecnt");
_Static_assert(offsetof(elephc_timelib_tz_counts32, charcnt) == 20, "timelib_tzinfo._bit32.charcnt");

_Static_assert(_Alignof(elephc_timelib_tz_counts64) == 8, "timelib_tzinfo.bit64 alignment");
_Static_assert(sizeof(elephc_timelib_tz_counts64) == 48, "timelib_tzinfo.bit64 size");
_Static_assert(offsetof(elephc_timelib_tz_counts64, ttisgmtcnt) == 0, "timelib_tzinfo.bit64.ttisgmtcnt");
_Static_assert(offsetof(elephc_timelib_tz_counts64, ttisstdcnt) == 8, "timelib_tzinfo.bit64.ttisstdcnt");
_Static_assert(offsetof(elephc_timelib_tz_counts64, leapcnt) == 16, "timelib_tzinfo.bit64.leapcnt");
_Static_assert(offsetof(elephc_timelib_tz_counts64, timecnt) == 24, "timelib_tzinfo.bit64.timecnt");
_Static_assert(offsetof(elephc_timelib_tz_counts64, typecnt) == 32, "timelib_tzinfo.bit64.typecnt");
_Static_assert(offsetof(elephc_timelib_tz_counts64, charcnt) == 40, "timelib_tzinfo.bit64.charcnt");

_Static_assert(_Alignof(tlocinfo) == 8, "timelib_tzinfo.location alignment");
_Static_assert(sizeof(tlocinfo) == 32, "timelib_tzinfo.location size");
_Static_assert(offsetof(tlocinfo, country_code) == 0, "timelib_tzinfo.location.country_code");
_Static_assert(offsetof(tlocinfo, latitude) == 8, "timelib_tzinfo.location.latitude");
_Static_assert(offsetof(tlocinfo, longitude) == 16, "timelib_tzinfo.location.longitude");
_Static_assert(offsetof(tlocinfo, comments) == 24, "timelib_tzinfo.location.comments");

_Static_assert(_Alignof(timelib_tzinfo) == 8, "timelib_tzinfo alignment");
_Static_assert(sizeof(timelib_tzinfo) == 176, "timelib_tzinfo size");
_Static_assert(offsetof(timelib_tzinfo, name) == 0, "timelib_tzinfo.name");
_Static_assert(offsetof(timelib_tzinfo, _bit32) == 8, "timelib_tzinfo._bit32");
_Static_assert(offsetof(timelib_tzinfo, bit64) == 32, "timelib_tzinfo.bit64");
_Static_assert(offsetof(timelib_tzinfo, trans) == 80, "timelib_tzinfo.trans");
_Static_assert(offsetof(timelib_tzinfo, trans_idx) == 88, "timelib_tzinfo.trans_idx");
_Static_assert(offsetof(timelib_tzinfo, type) == 96, "timelib_tzinfo.type");
_Static_assert(offsetof(timelib_tzinfo, timezone_abbr) == 104, "timelib_tzinfo.timezone_abbr");
_Static_assert(offsetof(timelib_tzinfo, leap_times) == 112, "timelib_tzinfo.leap_times");
_Static_assert(offsetof(timelib_tzinfo, bc) == 120, "timelib_tzinfo.bc");
_Static_assert(offsetof(timelib_tzinfo, location) == 128, "timelib_tzinfo.location");
_Static_assert(offsetof(timelib_tzinfo, posix_string) == 160, "timelib_tzinfo.posix_string");
_Static_assert(offsetof(timelib_tzinfo, posix_info) == 168, "timelib_tzinfo.posix_info");

_Static_assert(_Alignof(timelib_time) == 8, "timelib_time alignment");
_Static_assert(sizeof(timelib_time) == 240, "timelib_time size");
_Static_assert(offsetof(timelib_time, y) == 0, "timelib_time.y");
_Static_assert(offsetof(timelib_time, m) == 8, "timelib_time.m");
_Static_assert(offsetof(timelib_time, d) == 16, "timelib_time.d");
_Static_assert(offsetof(timelib_time, h) == 24, "timelib_time.h");
_Static_assert(offsetof(timelib_time, i) == 32, "timelib_time.i");
_Static_assert(offsetof(timelib_time, s) == 40, "timelib_time.s");
_Static_assert(offsetof(timelib_time, us) == 48, "timelib_time.us");
_Static_assert(offsetof(timelib_time, z) == 56, "timelib_time.z");
_Static_assert(offsetof(timelib_time, tz_abbr) == 64, "timelib_time.tz_abbr");
_Static_assert(offsetof(timelib_time, tz_info) == 72, "timelib_time.tz_info");
_Static_assert(offsetof(timelib_time, dst) == 80, "timelib_time.dst");
_Static_assert(offsetof(timelib_time, relative) == 88, "timelib_time.relative");
_Static_assert(offsetof(timelib_time, sse) == 192, "timelib_time.sse");
_Static_assert(offsetof(timelib_time, have_time) == 200, "timelib_time.have_time");
_Static_assert(offsetof(timelib_time, have_date) == 204, "timelib_time.have_date");
_Static_assert(offsetof(timelib_time, have_zone) == 208, "timelib_time.have_zone");
_Static_assert(offsetof(timelib_time, have_relative) == 212, "timelib_time.have_relative");
_Static_assert(offsetof(timelib_time, have_weeknr_day) == 216, "timelib_time.have_weeknr_day");
_Static_assert(offsetof(timelib_time, sse_uptodate) == 220, "timelib_time.sse_uptodate");
_Static_assert(offsetof(timelib_time, tim_uptodate) == 224, "timelib_time.tim_uptodate");
_Static_assert(offsetof(timelib_time, is_localtime) == 228, "timelib_time.is_localtime");
_Static_assert(offsetof(timelib_time, zone_type) == 232, "timelib_time.zone_type");

_Static_assert(_Alignof(timelib_error_message) == 8, "timelib_error_message alignment");
_Static_assert(sizeof(timelib_error_message) == 24, "timelib_error_message size");
_Static_assert(offsetof(timelib_error_message, error_code) == 0, "timelib_error_message.error_code");
_Static_assert(offsetof(timelib_error_message, position) == 4, "timelib_error_message.position");
_Static_assert(offsetof(timelib_error_message, character) == 8, "timelib_error_message.character");
_Static_assert(offsetof(timelib_error_message, message) == 16, "timelib_error_message.message");

_Static_assert(_Alignof(timelib_error_container) == 8, "timelib_error_container alignment");
_Static_assert(sizeof(timelib_error_container) == 24, "timelib_error_container size");
_Static_assert(offsetof(timelib_error_container, error_messages) == 0, "timelib_error_container.error_messages");
_Static_assert(offsetof(timelib_error_container, warning_messages) == 8, "timelib_error_container.warning_messages");
_Static_assert(offsetof(timelib_error_container, error_count) == 16, "timelib_error_container.error_count");
_Static_assert(offsetof(timelib_error_container, warning_count) == 20, "timelib_error_container.warning_count");

_Static_assert(_Alignof(timelib_abbr_info) == 8, "timelib_abbr_info alignment");
_Static_assert(sizeof(timelib_abbr_info) == 24, "timelib_abbr_info size");
_Static_assert(offsetof(timelib_abbr_info, utc_offset) == 0, "timelib_abbr_info.utc_offset");
_Static_assert(offsetof(timelib_abbr_info, abbr) == 8, "timelib_abbr_info.abbr");
_Static_assert(offsetof(timelib_abbr_info, dst) == 16, "timelib_abbr_info.dst");
