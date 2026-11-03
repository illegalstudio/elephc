/*
 * Purpose: ICU-backed implementation of PHP's ext/intl surface behind a fixed-width C ABI.
 * Called from: libelephc_intl (Rust staticlib) for every Intl class/function operation.
 * Key details:
 * - Values cross the ABI as a compact typed stream; strings are UTF-8 byte strings.
 * - Handles own one ICU object and its per-object error state in a process-wide table.
 * - The table is mutex-protected: compiled web programs may serve concurrent requests.
 * - ICU C++ objects keep the implementation close to php-src's ext/intl semantics while the
 *   exported ABI stays C and version-independent.
 * - No returned storage is process-global; callers supply the output buffer.
 */
#include "elephc_icu_intl_shim.h"

#include <unicode/brkiter.h>
#include <unicode/rbbi.h>
#include <unicode/usearch.h>
#include <unicode/utf16.h>
#include <unicode/utf8.h>
#include <unicode/calendar.h>
#include <unicode/coleitr.h>
#include <unicode/coll.h>
#include <unicode/currunit.h>
#include <unicode/datefmt.h>
#include <unicode/fieldpos.h>
#include <unicode/dcfmtsym.h>
#include <unicode/decimfmt.h>
#include <unicode/dtfmtsym.h>
#include <unicode/dtitvfmt.h>
#include <unicode/dtptngen.h>
#include <unicode/gregocal.h>
#include <unicode/listformatter.h>
#include <unicode/locid.h>
#include <unicode/msgfmt.h>
#include <unicode/messagepattern.h>
#include <unicode/normalizer2.h>
#include <unicode/numfmt.h>
#include <unicode/plurrule.h>
#include <unicode/resbund.h>
#include <unicode/rbnf.h>
#include <unicode/smpdtfmt.h>
#include <unicode/sortkey.h>
#include <unicode/uspoof.h>
#include <unicode/strenum.h>
#include <unicode/timezone.h>
#include <unicode/translit.h>
#include <unicode/ucal.h>
#include <unicode/uchar.h>
#include <unicode/ucnv.h>
#include <unicode/udat.h>
#include <unicode/uidna.h>
#include <unicode/uloc.h>
#include <unicode/umsg.h>
#include <unicode/unistr.h>
#include <unicode/unorm2.h>
#include <unicode/unum.h>
#include <unicode/upluralrules.h>
#include <unicode/ustring.h>
#include <unicode/utypes.h>
#include <unicode/uversion.h>

#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <cstring>
#include <map>
#include <memory>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

namespace {

using icu::BreakIterator;
using icu::Calendar;
using icu::Collator;
using icu::DateFormat;
using icu::DecimalFormat;
using icu::DecimalFormatSymbols;
using icu::FieldPosition;
using icu::Format;
using icu::Formattable;
using icu::Locale;
using icu::MessageFormat;
using icu::MessagePattern;
using icu::Normalizer2;
using icu::NumberFormat;
using icu::ParsePosition;
using icu::ResourceBundle;
using icu::SimpleDateFormat;
using icu::TimeZone;
using icu::Transliterator;
using icu::UnicodeString;

/* --------------------------------------------------------------------------------------- */
/* Typed value stream                                                                       */
/* --------------------------------------------------------------------------------------- */

struct Value {
    enum class Type { Null, False, True, Int, Float, Str, Array, Resource } type = Type::Null;
    int64_t integer = 0;
    double number = 0.0;
    std::string text;
    std::vector<std::pair<Value, Value>> entries;

    static Value null() { return Value(); }
    static Value boolean(bool value) {
        Value out;
        out.type = value ? Type::True : Type::False;
        return out;
    }
    static Value from_int(int64_t value) {
        Value out;
        out.type = Type::Int;
        out.integer = value;
        return out;
    }
    static Value from_double(double value) {
        Value out;
        out.type = Type::Float;
        out.number = value;
        return out;
    }
    static Value from_string(std::string value) {
        Value out;
        out.type = Type::Str;
        out.text = std::move(value);
        return out;
    }
    static Value array() {
        Value out;
        out.type = Type::Array;
        return out;
    }
};

struct Reader {
    const uint8_t *data;
    size_t length;
    size_t offset = 0;
    bool failed = false;

    Reader(const uint8_t *data, size_t length) : data(data), length(length) {}

    bool take(size_t count, const uint8_t **out) {
        if (failed || count > length - offset) {
            failed = true;
            return false;
        }
        *out = data + offset;
        offset += count;
        return true;
    }
    uint32_t u32() {
        const uint8_t *bytes = nullptr;
        if (!take(4, &bytes)) return 0;
        return (uint32_t(bytes[0]) << 24) | (uint32_t(bytes[1]) << 16)
            | (uint32_t(bytes[2]) << 8) | uint32_t(bytes[3]);
    }
    std::string bytes() {
        uint32_t count = u32();
        const uint8_t *start = nullptr;
        if (!take(count, &start)) return std::string();
        return std::string(reinterpret_cast<const char *>(start), count);
    }
};

bool read_value(Reader &reader, Value &out) {
    const uint8_t *tag = nullptr;
    if (!reader.take(1, &tag)) return false;
    switch (*tag) {
        case 'n': out = Value::null(); return true;
        case 'f': out = Value::boolean(false); return true;
        case 't': out = Value::boolean(true); return true;
        case 'i': {
            std::string text = reader.bytes();
            if (reader.failed) return false;
            char *end = nullptr;
            long long parsed = std::strtoll(text.c_str(), &end, 10);
            if (end == text.c_str()) return false;
            out = Value::from_int(parsed);
            return true;
        }
        case 'd': {
            std::string text = reader.bytes();
            if (reader.failed) return false;
            char *end = nullptr;
            double parsed = std::strtod(text.c_str(), &end);
            if (end == text.c_str()) return false;
            out = Value::from_double(parsed);
            return true;
        }
        case 's': {
            std::string text = reader.bytes();
            if (reader.failed) return false;
            out = Value::from_string(std::move(text));
            return true;
        }
        case 'r': {
            Value resource;
            resource.type = Value::Type::Resource;
            out = std::move(resource);
            return true;
        }
        case 'a': {
            uint32_t count = reader.u32();
            if (reader.failed) return false;
            out = Value::array();
            out.entries.reserve(count);
            for (uint32_t index = 0; index < count; index++) {
                Value key, value;
                if (!read_value(reader, key) || !read_value(reader, value)) return false;
                out.entries.emplace_back(std::move(key), std::move(value));
            }
            return true;
        }
        default:
            return false;
    }
}

bool read_arguments(const uint8_t *data, uint64_t length, std::vector<Value> &values) {
    Reader reader(data, size_t(length));
    uint32_t count = reader.u32();
    if (reader.failed) return false;
    values.clear();
    values.reserve(count);
    for (uint32_t index = 0; index < count; index++) {
        Value value;
        if (!read_value(reader, value)) return false;
        values.push_back(std::move(value));
    }
    return reader.offset == reader.length;
}

void append_u32(std::string &out, uint32_t value) {
    out.push_back(char((value >> 24) & 0xff));
    out.push_back(char((value >> 16) & 0xff));
    out.push_back(char((value >> 8) & 0xff));
    out.push_back(char(value & 0xff));
}

void write_value(std::string &out, const Value &value) {
    switch (value.type) {
        case Value::Type::Null: out.push_back('n'); break;
        case Value::Type::False: out.push_back('f'); break;
        case Value::Type::True: out.push_back('t'); break;
        case Value::Type::Int: {
            out.push_back('i');
            std::string text = std::to_string(value.integer);
            append_u32(out, uint32_t(text.size()));
            out += text;
            break;
        }
        case Value::Type::Float: {
            out.push_back('d');
            char text[64];
            int length = snprintf(text, sizeof(text), "%.17g", value.number);
            if (length < 0) length = 0;
            append_u32(out, uint32_t(length));
            out.append(text, size_t(length));
            break;
        }
        case Value::Type::Str: {
            out.push_back('s');
            append_u32(out, uint32_t(value.text.size()));
            out += value.text;
            break;
        }
        case Value::Type::Array: {
            out.push_back('a');
            append_u32(out, uint32_t(value.entries.size()));
            for (const auto &entry : value.entries) {
                write_value(out, entry.first);
                write_value(out, entry.second);
            }
            break;
        }
    }
}

std::string encode_value(const Value &value) {
    std::string out;
    write_value(out, value);
    return out;
}

/* --------------------------------------------------------------------------------------- */
/* String and argument helpers                                                              */
/* --------------------------------------------------------------------------------------- */

std::string to_utf8(const UnicodeString &text) {
    std::string out;
    text.toUTF8String(out);
    return out;
}

std::string from_utf16(const UChar *text, int32_t length, UErrorCode &status) {
    if (U_FAILURE(status)) return std::string();
    int32_t bytes = 0;
    u_strToUTF8(nullptr, 0, &bytes, text, length, &status);
    if (status == U_BUFFER_OVERFLOW_ERROR) status = U_ZERO_ERROR;
    if (U_FAILURE(status)) return std::string();
    std::string out(size_t(bytes), '\0');
    u_strToUTF8(out.data(), bytes, &bytes, text, length, &status);
    if (U_FAILURE(status)) return std::string();
    out.resize(size_t(bytes));
    return out;
}

UnicodeString from_utf8(const std::string &text) {
    return UnicodeString::fromUTF8(icu::StringPiece(text.data(), int32_t(text.size())));
}

bool argument_string(const std::vector<Value> &args, size_t index, std::string &out) {
    if (index >= args.size() || args[index].type != Value::Type::Str) return false;
    out = args[index].text;
    return true;
}

bool optional_string(const std::vector<Value> &args, size_t index, std::string &out) {
    if (index >= args.size() || args[index].type == Value::Type::Null) return false;
    return argument_string(args, index, out);
}

bool argument_int(const std::vector<Value> &args, size_t index, int64_t &out) {
    if (index >= args.size() || args[index].type != Value::Type::Int) return false;
    out = args[index].integer;
    return true;
}

bool optional_int(const std::vector<Value> &args, size_t index, int64_t fallback, int64_t &out) {
    out = fallback;
    if (index >= args.size() || args[index].type == Value::Type::Null) return true;
    return argument_int(args, index, out);
}

bool argument_double(const std::vector<Value> &args, size_t index, double &out) {
    if (index >= args.size()) return false;
    if (args[index].type == Value::Type::Float) {
        out = args[index].number;
        return true;
    }
    if (args[index].type == Value::Type::Int) {
        out = double(args[index].integer);
        return true;
    }
    return false;
}

bool argument_bool(const std::vector<Value> &args, size_t index, bool &out) {
    if (index >= args.size()) return false;
    if (args[index].type == Value::Type::True) { out = true; return true; }
    if (args[index].type == Value::Type::False) { out = false; return true; }
    if (args[index].type == Value::Type::Int) { out = args[index].integer != 0; return true; }
    return false;
}

bool optional_bool(const std::vector<Value> &args, size_t index, bool fallback, bool &out) {
    out = fallback;
    if (index >= args.size() || args[index].type == Value::Type::Null) return true;
    return argument_bool(args, index, out);
}

bool argument_array(const std::vector<Value> &args, size_t index, const Value *&out) {
    if (index >= args.size() || args[index].type != Value::Type::Array) return false;
    out = &args[index];
    return true;
}

/// Formats a PHP float the way PHP's string conversion does (14 significant digits).
std::string php_number_to_string(double value);

/* --------------------------------------------------------------------------------------- */
/* Error state                                                                              */
/* --------------------------------------------------------------------------------------- */

thread_local int32_t global_error_code = 0;
thread_local std::string global_error_message;
thread_local std::string global_error_context;

void set_global_error(UErrorCode code, const std::string &context) {
    global_error_code = int32_t(code);
    global_error_context = context;
    // php's intl_error_get_message() always appends the symbolic ICU status, including
    // U_ZERO_ERROR; the empty context carries the status alone.
    if (context.empty()) {
        global_error_message = u_errorName(code);
    } else if (context == u_errorName(code)) {
        global_error_message = context;
    } else {
        global_error_message = context + ": " + u_errorName(code);
    }
}

std::string icu_error_name(UErrorCode code) {
    return u_errorName(code);
}

/// Mirrors php-src `intl_parse_error_to_string()` for `createFromRules` failures.
std::string parse_error_to_string(const UParseError &parse_error) {
    std::string out = "parse error ";
    bool any = false;
    if (parse_error.line > 0) {
        out += "on line " + std::to_string(parse_error.line);
        any = true;
    }
    if (parse_error.offset >= 0) {
        if (any) out += ", ";
        else out += "at ";
        out += "offset " + std::to_string(parse_error.offset);
        any = true;
    }
    if (parse_error.preContext[0] != 0) {
        if (any) out += ", ";
        out += "after \"";
        out += to_utf8(UnicodeString(parse_error.preContext));
        out += "\"";
        any = true;
    }
    if (parse_error.postContext[0] != 0) {
        if (any) out += ", ";
        out += "before or at \"";
        out += to_utf8(UnicodeString(parse_error.postContext));
        out += "\"";
        any = true;
    }
    if (!any) out = "no parse error";
    return out;
}

/// Returns false when the bytes are not valid UTF-8.
bool valid_utf8(const std::string &input) {
    int32_t index = 0;
    int32_t length = int32_t(input.size());
    while (index < length) {
        UChar32 code = 0;
        U8_NEXT(input.data(), index, length, code);
        if (code < 0) return false;
    }
    return true;
}

struct IntlObject {
    int32_t error_code = 0;
    // php-src keeps the symbolic name even on success; several fixtures dump it verbatim.
    std::string error_message = "U_ZERO_ERROR";
    // The message WITHOUT the `: U_...` suffix, which is what IntlException carries.
    std::string error_context;
    virtual ~IntlObject() = default;
    virtual int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) = 0;
    virtual std::unique_ptr<IntlObject> clone() const = 0;
};

std::mutex &registry_mutex() {
    static std::mutex mutex;
    return mutex;
}

std::map<int64_t, std::unique_ptr<IntlObject>> &registry() {
    static std::map<int64_t, std::unique_ptr<IntlObject>> table;
    return table;
}

int64_t &next_handle() {
    static int64_t handle = 1;
    return handle;
}

void record_error(IntlObject &object, UErrorCode status, const std::string &context) {
    object.error_code = int32_t(status);
    object.error_context = context;
    if (context.empty()) {
        object.error_message = u_errorName(status);
    } else if (context == u_errorName(status)) {
        object.error_message = context;
    } else {
        object.error_message = context + ": " + u_errorName(status);
    }
    // PHP's Intl methods mirror their object error into the process-wide state that
    // `intl_get_error_message()` reports.
    set_global_error(status, context);
}

IntlObject *lookup(int64_t handle) {
    std::lock_guard<std::mutex> lock(registry_mutex());
    auto found = registry().find(handle);
    return found == registry().end() ? nullptr : found->second.get();
}

int64_t insert(std::unique_ptr<IntlObject> object) {
    std::lock_guard<std::mutex> lock(registry_mutex());
    int64_t handle = next_handle()++;
    registry().emplace(handle, std::move(object));
    return handle;
}

int32_t write_output(const std::string &payload, uint8_t *out, uint64_t capacity,
                     uint64_t *needed) {
    if (needed == nullptr) return U_ILLEGAL_ARGUMENT_ERROR;
    *needed = uint64_t(payload.size());
    if (capacity == 0) return U_ZERO_ERROR;
    if (out == nullptr || capacity < payload.size()) return U_BUFFER_OVERFLOW_ERROR;
    std::memcpy(out, payload.data(), payload.size());
    return U_ZERO_ERROR;
}

/// Wraps one inserted handle in the PHP-visible object envelope understood by
/// `__elephc_intl_wrap`, so a procedural create returns an object, not an integer handle.
Value wrap_object(int64_t handle, const char *class_name) {
    Value wrapper = Value::array();
    wrapper.entries = {
        {Value::from_string("__elephc_object"), Value::from_int(handle)},
        {Value::from_string("__elephc_class"), Value::from_string(class_name)},
    };
    return wrapper;
}

/* --------------------------------------------------------------------------------------- */
/* Date/time formatter                                                                      */
/* --------------------------------------------------------------------------------------- */

/// Inserts a cloned ICU time zone into the handle table and returns its handle.
int64_t wrap_time_zone(const TimeZone &zone);

/// Inserts a cloned ICU calendar into the handle table and returns its handle.
int64_t wrap_calendar(const Calendar &calendar);

/// Adopts a cloned calendar from one handle into a date formatter.
bool adopt_calendar_from_handle(DateFormat &formatter, int64_t handle);

struct DateFormatterObject : IntlObject {
    std::unique_ptr<DateFormat> formatter;
    SimpleDateFormat *simple = nullptr;
    int64_t date_style = UDAT_FULL;
    int64_t time_style = UDAT_FULL;
    int64_t calendar_kind = UCAL_GREGORIAN;
    bool calendar_set_by_object = false;
    std::string locale;

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!formatter) {
            record_error(*this, U_INVALID_STATE_ERROR, "IntlDateFormatter is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        if (method == "format") {
            double seconds = 0.0;
            if (!argument_double(args, 0, seconds)) {
                record_error(*this, U_ILLEGAL_ARGUMENT_ERROR, "IntlDateFormatter::format(): invalid datetime");
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            UnicodeString text;
            formatter->format(double(seconds) * 1000.0, text, status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlDateFormatter::format()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::from_string(to_utf8(text));
            return U_ZERO_ERROR;
        }
        if (method == "invalid_datetime") {
            record_error(*this, U_ILLEGAL_ARGUMENT_ERROR,
                "IntlDateFormatter::format(): invalid object type for date/time (only IntlCalendar and DateTimeInterface permitted)");
            out = Value::boolean(false);
            return U_ZERO_ERROR;
        }
        if (method == "format_localtime") {
            int64_t hour = 0, minute = 0, second = 0, month = 0, mday = 0, year = 0;
            if (!argument_int(args, 0, hour) || !argument_int(args, 1, minute) ||
                !argument_int(args, 2, second) || !argument_int(args, 3, month) ||
                !argument_int(args, 4, mday) || !argument_int(args, 5, year)) {
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            std::unique_ptr<Calendar> calendar(formatter->getCalendar()->clone());
            calendar->clear();
            calendar->set(int32_t(year) + 1900, int32_t(month), int32_t(mday),
                          int32_t(hour), int32_t(minute), int32_t(second));
            double millis = calendar->getTime(status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlDateFormatter::format()");
                out = Value::boolean(false);
                return status;
            }
            UnicodeString text;
            formatter->format(millis, text, status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlDateFormatter::format()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::from_string(to_utf8(text));
            return U_ZERO_ERROR;
        }
        if (method == "parse" || method == "localtime" || method == "parseToCalendar") {
            std::string input;
            if (!argument_string(args, 0, input)) {
                record_error(*this, U_ILLEGAL_ARGUMENT_ERROR, "IntlDateFormatter::parse(): invalid string");
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            ParsePosition position(0);
            UDate parsed = formatter->parse(from_utf8(input), position);
            if (position.getErrorIndex() >= 0 || position.getIndex() == 0) {
                record_error(*this, U_PARSE_ERROR, "IntlDateFormatter::parse()");
                out = Value::boolean(false);
                return U_PARSE_ERROR;
            }
            if (method == "parseToCalendar" && simple) {
                std::unique_ptr<Calendar> calendar(simple->getCalendar()->clone());
                calendar->setTime(parsed, status);
                if (U_FAILURE(status)) {
                    record_error(*this, status, "IntlDateFormatter::parseToCalendar()");
                    out = Value::boolean(false);
                    return status;
                }
                double millis = double(calendar->getTime(status));
                out = Value::from_int(int64_t(millis));
                return U_ZERO_ERROR;
            }
            if (method == "parseToCalendar") {
                // PHP's parseToCalendar parses into the formatter's calendar and adopts any
                // timezone carried by the value. A `VV` field names a zone id; adopt it so a
                // following getTimeZone()/parse uses it.
                UErrorCode zone_status = U_ZERO_ERROR;
                UnicodeString zone_input = from_utf8(input);
                UnicodeString canonical;
                TimeZone::getCanonicalID(zone_input, canonical, zone_status);
                if (U_SUCCESS(zone_status) && canonical == zone_input) {
                    formatter->adoptTimeZone(TimeZone::createTimeZone(zone_input));
                }
            }
            if (method == "parse" || method == "parseToCalendar") {
                int32_t utf16_index = position.getIndex();
                UnicodeString prefix = from_utf8(input).tempSubString(0, utf16_index);
                std::string prefix_utf8;
                prefix.toUTF8String(prefix_utf8);
                out = Value::array();
                // PHP returns the parsed instant in seconds, while ICU's UDate is milliseconds.
                out.entries = {
                    {Value::from_int(0), Value::from_int(int64_t(parsed / 1000.0))},
                    {Value::from_int(1), Value::from_int(int64_t(prefix_utf8.size()))},
                };
                return U_ZERO_ERROR;
            }
            std::unique_ptr<Calendar> calendar(simple->getCalendar()->clone());
            calendar->setTime(parsed, status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlDateFormatter::localtime()");
                out = Value::boolean(false);
                return status;
            }
            int32_t year = calendar->get(UCAL_YEAR, status);
            int32_t month = calendar->get(UCAL_MONTH, status);
            int32_t day = calendar->get(UCAL_DATE, status);
            int32_t hour = calendar->get(UCAL_HOUR_OF_DAY, status);
            int32_t minute = calendar->get(UCAL_MINUTE, status);
            int32_t second = calendar->get(UCAL_SECOND, status);
            int32_t wday = calendar->get(UCAL_DAY_OF_WEEK, status) - 1;
            // php reports ICU's 1-based DAY_OF_YEAR for tm_yday (only wday is decremented).
            int32_t yday = calendar->get(UCAL_DAY_OF_YEAR, status);
            int32_t dst = 0;
            calendar->get(UCAL_DST_OFFSET, status);
            UDate millis = calendar->getTime(status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlDateFormatter::localtime()");
                out = Value::boolean(false);
                return status;
            }
            Value result = Value::array();
            // php's localtime array carries exactly these nine keys, in this order.
            result.entries = {
                {Value::from_string("tm_sec"), Value::from_int(second)},
                {Value::from_string("tm_min"), Value::from_int(minute)},
                {Value::from_string("tm_hour"), Value::from_int(hour)},
                {Value::from_string("tm_year"), Value::from_int(year - 1900)},
                {Value::from_string("tm_mday"), Value::from_int(day)},
                {Value::from_string("tm_wday"), Value::from_int(wday)},
                {Value::from_string("tm_yday"), Value::from_int(yday)},
                {Value::from_string("tm_mon"), Value::from_int(month)},
                {Value::from_string("tm_isdst"), Value::from_int(dst)},
            };
            (void)millis;
            Value pair = Value::array();
            pair.entries = {
                {Value::from_int(0), std::move(result)},
                {Value::from_int(1), Value::from_int(position.getIndex())},
            };
            out = std::move(pair);
            return U_ZERO_ERROR;
        }
        if (method == "datetype") {
            out = Value::from_int(date_style);
            return U_ZERO_ERROR;
        }
        if (method == "timetype") {
            out = Value::from_int(time_style);
            return U_ZERO_ERROR;
        }
        if (method == "pattern") {
            UnicodeString pattern;
            if (simple == nullptr) {
                record_error(*this, U_UNSUPPORTED_ERROR, "IntlDateFormatter::getPattern() without a simple formatter");
                return U_UNSUPPORTED_ERROR;
            }
            pattern = simple->toPattern(pattern);
            out = Value::from_string(to_utf8(pattern));
            return U_ZERO_ERROR;
        }
        if (method == "set_pattern") {
            std::string pattern;
            if (!argument_string(args, 0, pattern)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (simple == nullptr) {
                record_error(*this, U_UNSUPPORTED_ERROR, "IntlDateFormatter::setPattern() requires a pattern formatter");
                return U_UNSUPPORTED_ERROR;
            }
            // PHP's setPattern()/constructor pattern uses ICU's non-localized pattern syntax.
            simple->applyPattern(from_utf8(pattern));
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "set_lenient") {
            bool lenient = true;
            if (!argument_bool(args, 0, lenient)) return U_ILLEGAL_ARGUMENT_ERROR;
            formatter->setLenient(lenient);
            out = Value::null();
            return U_ZERO_ERROR;
        }
        if (method == "is_lenient") {
            out = Value::boolean(formatter->isLenient());
            return U_ZERO_ERROR;
        }
        if (method == "timezone_id") {
            UnicodeString id;
            formatter->getTimeZone().getID(id);
            out = Value::from_string(to_utf8(id));
            return U_ZERO_ERROR;
        }
        if (method == "calendar_kind") {
            // php-src reports the type only when setCalendar() received a type constant; an
            // adopted IntlCalendar object makes getCalendar() answer false.
            if (calendar_set_by_object) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            const char *resolved = formatter->getCalendar()->getType();
            out = Value::from_int(resolved != nullptr && std::strcmp(resolved, "gregorian") == 0
                ? 1 : 0);
            return U_ZERO_ERROR;
        }
        if (method == "calendar_object") {
            int64_t handle = wrap_calendar(*formatter->getCalendar());
            if (handle == 0) return U_MEMORY_ALLOCATION_ERROR;
            Value wrapper = Value::array();
            wrapper.entries = {
                {Value::from_string("__elephc_object"), Value::from_int(handle)},
                {Value::from_string("__elephc_class"), Value::from_string("IntlCalendar")},
            };
            out = std::move(wrapper);
            return U_ZERO_ERROR;
        }
        if (method == "set_calendar_object") {
            if (args.empty() || args[0].type != Value::Type::Int) {
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            if (!adopt_calendar_from_handle(*formatter, args[0].integer)) {
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            calendar_set_by_object = true;
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "locale") {
            int64_t type = ULOC_ACTUAL_LOCALE;
            optional_int(args, 0, ULOC_ACTUAL_LOCALE, type);
            Locale actual = formatter->getLocale(ULocDataLocaleType(type), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlDateFormatter::getLocale()");
                return status;
            }
            std::string name = actual.getName();
            // A pattern-created SimpleDateFormat keeps its locale data lazy, so the ICU
            // accessor answers an empty name until the first format. PHP reports the
            // requested (canonicalized) locale in that case.
            if (name.empty()) {
                Locale valid = formatter->getLocale(ULOC_VALID_LOCALE, status);
                name = valid.getName();
            }
            if (name.empty() && !locale.empty()) {
                name = Locale(locale.c_str()).getName();
            }
            out = Value::from_string(name);
            return U_ZERO_ERROR;
        }
        if (method == "get_timezone") {
            int64_t handle = wrap_time_zone(formatter->getTimeZone());
            if (handle == 0) return U_MEMORY_ALLOCATION_ERROR;
            Value wrapper = Value::array();
            wrapper.entries = {
                {Value::from_string("__elephc_object"), Value::from_int(handle)},
                {Value::from_string("__elephc_class"), Value::from_string("IntlTimeZone")},
            };
            out = std::move(wrapper);
            return U_ZERO_ERROR;
        }
        if (method == "set_timezone") {
            std::string zone;
            if (!argument_string(args, 0, zone)) return U_ILLEGAL_ARGUMENT_ERROR;
            std::unique_ptr<TimeZone> timezone(TimeZone::createTimeZone(from_utf8(zone)));
            formatter->adoptTimeZone(timezone.release());
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "set_calendar") {
            int64_t kind = UCAL_GREGORIAN;
            if (!argument_int(args, 0, kind)) return U_ILLEGAL_ARGUMENT_ERROR;
            calendar_kind = kind;
            calendar_set_by_object = false;
            // TRADITIONAL means the locale's default calendar, which is encoded in the
            // requested locale (`fr@calendar=islamic`), not in ICU's resolved valid locale.
            Locale requested = locale.empty() ? Locale::getDefault() : Locale(locale.c_str());
            std::unique_ptr<Calendar> calendar(Calendar::createInstance(
                formatter->getTimeZone().clone(), requested, status));
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlDateFormatter::setCalendar()");
                out = Value::boolean(false);
                return status;
            }
            if (kind == UCAL_GREGORIAN) {
                calendar.reset(new icu::GregorianCalendar(formatter->getTimeZone().clone(),
                                                          formatter->getLocale(ULOC_VALID_LOCALE, status), status));
            }
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlDateFormatter::setCalendar()");
                out = Value::boolean(false);
                return status;
            }
            formatter->adoptCalendar(calendar.release());
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported IntlDateFormatter method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<DateFormatterObject>();
        copy->formatter.reset(formatter->clone());
        copy->simple = dynamic_cast<SimpleDateFormat *>(copy->formatter.get());
        copy->date_style = date_style;
        copy->time_style = time_style;
        copy->calendar_kind = calendar_kind;
        copy->calendar_set_by_object = calendar_set_by_object;
        copy->locale = locale;
        return copy;
    }
};

/* --------------------------------------------------------------------------------------- */
/* Number formatter                                                                         */
/* --------------------------------------------------------------------------------------- */

/// Opens one ICU number formatter the way php-src's `unum_open` does.
///
/// The C++ `DecimalFormat("#")` constructor normalizes the pattern to "0" in ICU 78, while
/// `unum_open(UNUM_PATTERN_DECIMAL, nullptr, 0, ...)` keeps php's "#" and its six fraction
/// digits. Route the pattern styles through the C entry point so both match PHP.
NumberFormat *open_number_format(int64_t style, const std::string &locale,
                                 const std::string &pattern, bool has_pattern,
                                 UErrorCode &status) {
    // php-src always opens through unum_open(), which is the only entry point that supports
    // every UNumberFormatStyle (SPELLOUT, ORDINAL, DURATION, compact, and pattern styles).
    std::string loc = locale.empty() ? Locale::getDefault().getName() : locale;
    // ICU only honours the pattern for the two pattern styles; php's ext/intl passes it to
    // unum_open() in every case, and ICU ignores it for the remaining styles.
    bool pattern_style = style == UNUM_PATTERN_DECIMAL || style == UNUM_PATTERN_RULEBASED;
    UnicodeString pat = (has_pattern && pattern_style) ? from_utf8(pattern) : UnicodeString();
    bool use_pattern = has_pattern && pattern_style;
    UNumberFormat *raw = unum_open(UNumberFormatStyle(style),
        use_pattern ? pat.getBuffer() : nullptr, use_pattern ? pat.length() : 0,
        loc.c_str(), nullptr, &status);
    return reinterpret_cast<NumberFormat *>(raw);
}

struct NumberFormatterObject : IntlObject {
    std::unique_ptr<NumberFormat> formatter;
    std::string locale;
    int64_t style = UNUM_DECIMAL;

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!formatter) {
            record_error(*this, U_INVALID_STATE_ERROR, "NumberFormatter is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        auto *decimal = dynamic_cast<DecimalFormat *>(formatter.get());
        if (method == "format" || method == "format_currency") {
            UnicodeString text;
            if (method == "format_currency") {
                double amount = 0.0;
                std::string currency;
                if (!argument_double(args, 0, amount) || !argument_string(args, 1, currency)) {
                    return U_ILLEGAL_ARGUMENT_ERROR;
                }
                // Currency formatting is requested through an adopted currency symbol below
                // so the ICU formatter keeps PHP's grouping/precision attributes.
                NumberFormat *copy = formatter->clone();
                UnicodeString currency_text = from_utf8(currency);
                copy->setCurrency(currency_text.getBuffer(), status);
                copy->format(amount, text, status);
                delete copy;
            } else {
                int64_t type = 0;
                optional_int(args, 1, 0, type);
                if (args.empty()) return U_ILLEGAL_ARGUMENT_ERROR;
                bool integer_type = type == 1 || type == 2; // TYPE_INT32 / TYPE_INT64
                if (integer_type) {
                    if (args[0].type == Value::Type::Int) {
                        formatter->format(int64_t(args[0].integer), text, status);
                    } else if (args[0].type == Value::Type::Float) {
                        formatter->format(int64_t(args[0].number), text, status);
                    } else {
                        return U_ILLEGAL_ARGUMENT_ERROR;
                    }
                } else if (args[0].type == Value::Type::Int) {
                    formatter->format(int64_t(args[0].integer), text, status);
                } else if (args[0].type == Value::Type::Float) {
                    formatter->format(args[0].number, text, status);
                } else {
                    return U_ILLEGAL_ARGUMENT_ERROR;
                }
            }
            if (U_FAILURE(status)) {
                record_error(*this, status, "NumberFormatter::format()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::from_string(to_utf8(text));
            return U_ZERO_ERROR;
        }
        if (method == "parse" || method == "parse_currency") {
            std::string input;
            if (!argument_string(args, 0, input)) return U_ILLEGAL_ARGUMENT_ERROR;
            int64_t start = 0;
            optional_int(args, method == "parse_currency" ? 1 : 2, 0, start);
            // An omitted by-reference offset can reach the bridge as an uninitialized cell;
            // php treats it as 0.
            if (start < 0 || start > int64_t(input.size())) start = 0;
            ParsePosition position{int32_t(start)};
            if (method == "parse_currency") {
                std::unique_ptr<icu::CurrencyAmount> parsed(
                    formatter->parseCurrency(from_utf8(input), position));
                if (!parsed || position.getErrorIndex() >= 0 || position.getIndex() == 0) {
                    record_error(*this, U_PARSE_ERROR,
                        "NumberFormatter::parseCurrency(): Number parsing failed");
                    out = Value::boolean(false);
                    return U_PARSE_ERROR;
                }
                Value result = Value::array();
                result.entries = {
                    {Value::from_int(0), Value::from_double(parsed->getNumber().getDouble(status))},
                    {Value::from_int(1), Value::from_string(to_utf8(parsed->getISOCurrency()))},
                    {Value::from_int(2), Value::from_int(position.getIndex())},
                };
                out = std::move(result);
                return U_ZERO_ERROR;
            }
            int64_t parse_type = 0;
            optional_int(args, 1, 0, parse_type);
            bool integer_type = parse_type == 1 || parse_type == 2;
            NumberFormat *parser = formatter.get();
            NumberFormat *integer_parser = nullptr;
            if (integer_type) {
                integer_parser = formatter->clone();
                integer_parser->setParseIntegerOnly(true);
                parser = integer_parser;
            }
            Formattable parsed;
            parser->parse(from_utf8(input), parsed, position);
            delete integer_parser;
            if (position.getErrorIndex() >= 0 || position.getIndex() == 0) {
                record_error(*this, U_PARSE_ERROR,
                    "NumberFormatter::parse(): Number parsing failed");
                out = Value::boolean(false);
                return U_PARSE_ERROR;
            }
            Value result = Value::array();
            if (integer_type || parsed.getType() == Formattable::kLong ||
                parsed.getType() == Formattable::kInt64) {
                int64_t value = 0;
                if (parsed.getType() == Formattable::kLong) value = parsed.getLong();
                else if (parsed.getType() == Formattable::kInt64) value = parsed.getInt64();
                else value = int64_t(parsed.getDouble());
                result.entries = {
                    {Value::from_int(0), Value::from_int(value)},
                    {Value::from_int(1), Value::from_int(position.getIndex())},
                };
            } else {
                result.entries = {
                    {Value::from_int(0), Value::from_double(parsed.getDouble())},
                    {Value::from_int(1), Value::from_int(position.getIndex())},
                };
            }
            out = std::move(result);
            return U_ZERO_ERROR;
        }
        if (method == "set_attribute") {
            int64_t attribute = 0, value = 0;
            if (!argument_int(args, 0, attribute) || !argument_int(args, 1, value))
                return U_ILLEGAL_ARGUMENT_ERROR;
            if (!decimal) return U_UNSUPPORTED_ERROR;
            decimal->setAttribute(UNumberFormatAttribute(attribute), int32_t(value), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "NumberFormatter::setAttribute()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "get_attribute") {
            int64_t attribute = 0;
            if (!argument_int(args, 0, attribute)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (!decimal) return U_UNSUPPORTED_ERROR;
            int32_t value = decimal->getAttribute(UNumberFormatAttribute(attribute), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "NumberFormatter::getAttribute()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::from_int(value);
            return U_ZERO_ERROR;
        }
        if (method == "set_text_attribute" || method == "get_text_attribute") {
            int64_t attribute = 0;
            if (!argument_int(args, 0, attribute)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (method == "set_text_attribute") {
                std::string value;
                if (!argument_string(args, 1, value)) return U_ILLEGAL_ARGUMENT_ERROR;
                if (!decimal) return U_UNSUPPORTED_ERROR;
                UnicodeString text = from_utf8(value);
                switch (attribute) {
                    case 0: decimal->setPositivePrefix(text); break;
                    case 1: decimal->setPositiveSuffix(text); break;
                    case 2: decimal->setNegativePrefix(text); break;
                    case 3: decimal->setNegativeSuffix(text); break;
                    case 4: decimal->setPadCharacter(text); break;
                    case 5: decimal->setCurrency(text.getBuffer(), status); break;
                    default: return U_UNSUPPORTED_ERROR;
                }
                out = Value::boolean(U_SUCCESS(status));
            } else {
                if (!decimal) return U_UNSUPPORTED_ERROR;
                UnicodeString value;
                switch (attribute) {
                    case 0: decimal->getPositivePrefix(value); break;
                    case 1: decimal->getPositiveSuffix(value); break;
                    case 2: decimal->getNegativePrefix(value); break;
                    case 3: decimal->getNegativeSuffix(value); break;
                    case 4: value = decimal->getPadCharacterString(); break;
                    case 5: value = UnicodeString(decimal->getCurrency()); break;
                    default: return U_UNSUPPORTED_ERROR;
                }
                out = Value::from_string(to_utf8(value));
            }
            if (U_FAILURE(status)) {
                record_error(*this, status, "NumberFormatter text attribute");
                return status;
            }
            return U_ZERO_ERROR;
        }
        if (method == "set_symbol" || method == "get_symbol") {
            int64_t symbol = 0;
            if (!argument_int(args, 0, symbol)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (symbol < 0 || symbol >= int64_t(DecimalFormatSymbols::kFormatSymbolCount)) {
                record_error(*this, U_ILLEGAL_ARGUMENT_ERROR, "invalid symbol value");
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            if (method == "set_symbol") {
                if (!decimal) return U_UNSUPPORTED_ERROR;
                std::string value;
                if (!argument_string(args, 1, value)) return U_ILLEGAL_ARGUMENT_ERROR;
                DecimalFormatSymbols symbols = *decimal->getDecimalFormatSymbols();
                symbols.setSymbol(DecimalFormatSymbols::ENumberFormatSymbol(symbol),
                                  from_utf8(value));
                decimal->setDecimalFormatSymbols(symbols);
                out = Value::boolean(true);
            } else {
                if (!decimal) return U_UNSUPPORTED_ERROR;
                out = Value::from_string(to_utf8(decimal->getDecimalFormatSymbols()->getSymbol(
                    DecimalFormatSymbols::ENumberFormatSymbol(symbol))));
            }
            if (U_FAILURE(status)) {
                record_error(*this, status, "NumberFormatter symbol");
                return status;
            }
            return U_ZERO_ERROR;
        }
        if (method == "pattern") {
            return call(args.empty() ? "get_pattern" : "set_pattern", args, out);
        }
        if (method == "set_pattern" || method == "get_pattern") {
            if (!decimal) return U_UNSUPPORTED_ERROR;
            if (method == "set_pattern") {
                std::string pattern;
                if (!argument_string(args, 0, pattern)) return U_ILLEGAL_ARGUMENT_ERROR;
                UParseError parse_error = {0};
                decimal->applyPattern(from_utf8(pattern), parse_error, status);
                if (U_FAILURE(status)) {
                    char buffer[128];
                    std::snprintf(buffer, sizeof(buffer),
                        "Error setting pattern value at line %d, offset %d",
                        parse_error.line, parse_error.offset);
                    record_error(*this, status, buffer);
                    out = Value::boolean(false);
                    return status;
                }
                out = Value::boolean(true);
            } else {
                UnicodeString pattern;
                decimal->toPattern(pattern);
                out = Value::from_string(to_utf8(pattern));
            }
            if (U_FAILURE(status)) {
                record_error(*this, status, "NumberFormatter pattern");
                return status;
            }
            return U_ZERO_ERROR;
        }
        if (method == "locale") {
            int64_t type = ULOC_ACTUAL_LOCALE;
            optional_int(args, 0, ULOC_ACTUAL_LOCALE, type);
            Locale actual = formatter->getLocale(ULocDataLocaleType(type), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "NumberFormatter::getLocale()");
                return status;
            }
            out = Value::from_string(actual.getName());
            return U_ZERO_ERROR;
        }
        if (method == "error_code") {
            out = Value::from_int(error_code);
            return U_ZERO_ERROR;
        }
        if (method == "error_message") {
            out = Value::from_string(error_message);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported NumberFormatter method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<NumberFormatterObject>();
        copy->formatter.reset(formatter->clone());
        copy->locale = locale;
        copy->style = style;
        return copy;
    }
};

/* --------------------------------------------------------------------------------------- */
/* Collator                                                                                 */
/* --------------------------------------------------------------------------------------- */

struct CollatorObject : IntlObject {
    std::unique_ptr<Collator> collator;
    std::string locale;

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!collator) {
            record_error(*this, U_INVALID_STATE_ERROR, "Collator is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        if (method != "error_code" && method != "error_message") {
            record_error(*this, U_ZERO_ERROR, "U_ZERO_ERROR");
        }
        if (method == "error_code") {
            out = Value::from_int(error_code);
            return U_ZERO_ERROR;
        }
        if (method == "error_message") {
            out = Value::from_string(error_message);
            return U_ZERO_ERROR;
        }
        if (method == "compare") {
            std::string left, right;
            if (!argument_string(args, 0, left) || !argument_string(args, 1, right))
                return U_ILLEGAL_ARGUMENT_ERROR;
            UCollationResult result = collator->compare(from_utf8(left), from_utf8(right), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Collator::compare()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::from_int(int32_t(result));
            return U_ZERO_ERROR;
        }
        if (method == "get_attribute" || method == "set_attribute") {
            int64_t attribute = 0;
            if (!argument_int(args, 0, attribute)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (attribute < 0 || attribute >= UCOL_ATTRIBUTE_COUNT) {
                record_error(*this, U_ILLEGAL_ARGUMENT_ERROR,
                             "Error getting attribute value");
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            UColAttribute ucol_attribute = UColAttribute(attribute);
            if (method == "set_attribute") {
                int64_t value = 0;
                if (!argument_int(args, 1, value)) return U_ILLEGAL_ARGUMENT_ERROR;
                collator->setAttribute(ucol_attribute, UColAttributeValue(value), status);
                out = Value::boolean(U_SUCCESS(status));
            } else {
                out = Value::from_int(collator->getAttribute(ucol_attribute, status));
            }
            if (U_FAILURE(status)) {
                record_error(*this, status, "Error getting attribute value");
                return status;
            }
            return U_ZERO_ERROR;
        }
        if (method == "get_strength" || method == "set_strength") {
            if (method == "set_strength") {
                int64_t strength = 0;
                if (!argument_int(args, 0, strength)) return U_ILLEGAL_ARGUMENT_ERROR;
                collator->setStrength(Collator::ECollationStrength(strength));
                out = Value::boolean(true);
            } else {
                out = Value::from_int(collator->getStrength());
            }
            return U_ZERO_ERROR;
        }
        if (method == "sort_key") {
            std::string value;
            if (!argument_string(args, 0, value)) return U_ILLEGAL_ARGUMENT_ERROR;
            icu::CollationKey key;
            collator->getCollationKey(from_utf8(value), key, status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Collator::getSortKey()");
                out = Value::boolean(false);
                return status;
            }
            int32_t key_length = 0;
            const uint8_t *bytes = key.getByteArray(key_length);
            // ICU terminates a collation key with a zero byte; php-src returns the bytes
            // before it (`keylen - 1`).
            if (key_length > 0 && bytes[key_length - 1] == 0) key_length -= 1;
            out = Value::from_string(std::string(reinterpret_cast<const char *>(bytes),
                                                 size_t(key_length)));
            return U_ZERO_ERROR;
        }
        if (method == "sort" || method == "asort" || method == "sort_with_sort_keys") {
            const Value *values = nullptr;
            if (!argument_array(args, 0, values)) return U_ILLEGAL_ARGUMENT_ERROR;
            int64_t sort_flags = 0;
            if (method != "sort_with_sort_keys") optional_int(args, 1, 0, sort_flags);
            Value sorted = *values;
            auto value_text = [](const Value &value) -> UnicodeString {
                switch (value.type) {
                    case Value::Type::Str: return from_utf8(value.text);
                    case Value::Type::Int: return UnicodeString::fromUTF8(std::to_string(value.integer));
                    case Value::Type::Float: return UnicodeString::fromUTF8(php_number_to_string(value.number));
                    case Value::Type::True: return UnicodeString("1");
                    case Value::Type::Array: return UnicodeString("Array");
                    default: return UnicodeString();
                }
            };
            auto value_number = [](const Value &value, double &out) -> bool {
                switch (value.type) {
                    case Value::Type::Int: out = double(value.integer); return true;
                    case Value::Type::Float: out = value.number; return true;
                    case Value::Type::True: out = 1.0; return true;
                    case Value::Type::False:
                    case Value::Type::Null: out = 0.0; return true;
                    case Value::Type::Str: {
                        // PHP's numeric strings allow leading/trailing whitespace.
                        size_t begin = value.text.find_first_not_of(" \t\n\r\v\f");
                        if (begin == std::string::npos) return false;
                        size_t end_pos = value.text.find_last_not_of(" \t\n\r\v\f");
                        std::string text = value.text.substr(begin, end_pos - begin + 1);
                        char *end = nullptr;
                        double parsed = std::strtod(text.c_str(), &end);
                        if (end == text.c_str() || *end != '\0') return false;
                        out = parsed;
                        return true;
                    }
                    default: return false;
                }
            };
            auto compare_entries = [&](const std::pair<Value, Value> &left,
                                       const std::pair<Value, Value> &right) {
                UnicodeString left_text = value_text(left.second);
                UnicodeString right_text = value_text(right.second);
                if (method == "sort_with_sort_keys") {
                    icu::CollationKey left_key, right_key;
                    UErrorCode key_status = U_ZERO_ERROR;
                    collator->getCollationKey(left_text, left_key, key_status);
                    collator->getCollationKey(right_text, right_key, key_status);
                    return left_key.compareTo(right_key) < 0;
                }
                if (sort_flags == 2) { // Collator::SORT_NUMERIC
                    double left_number = 0.0, right_number = 0.0;
                    bool left_ok = value_number(left.second, left_number);
                    bool right_ok = value_number(right.second, right_number);
                    if (left_ok && right_ok) return left_number < right_number;
                } else if (sort_flags == 0) { // SORT_REGULAR: numbers compare numerically
                    double left_number = 0.0, right_number = 0.0;
                    bool left_ok = value_number(left.second, left_number);
                    bool right_ok = value_number(right.second, right_number);
                    if (left_ok && right_ok) return left_number < right_number;
                }
                return collator->compare(left_text, right_text, status) == UCOL_LESS;
            };
            std::stable_sort(sorted.entries.begin(), sorted.entries.end(), compare_entries);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Collator sorting");
                out = Value::boolean(false);
                return status;
            }
            if (method == "sort" || method == "sort_with_sort_keys") {
                // `sort` renumbers keys; `asort` and the keyed variants preserve them.
                for (size_t index = 0; index < sorted.entries.size(); index++) {
                    sorted.entries[index].first = Value::from_int(int64_t(index));
                }
            }
            Value result = Value::array();
            result.entries = {
                {Value::from_int(0), Value::boolean(true)},
                {Value::from_int(1), std::move(sorted)},
            };
            out = std::move(result);
            return U_ZERO_ERROR;
        }
        if (method == "locale") {
            int64_t type = ULOC_ACTUAL_LOCALE;
            optional_int(args, 0, ULOC_ACTUAL_LOCALE, type);
            Locale actual = collator->getLocale(ULocDataLocaleType(type), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Error getting locale by type");
                return status;
            }
            std::string name = actual.getName();
            out = Value::from_string(name.empty() ? "root" : name);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported Collator method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<CollatorObject>();
        copy->collator.reset(collator->clone());
        copy->locale = locale;
        return copy;
    }
};

/* --------------------------------------------------------------------------------------- */
/* MessageFormatter                                                                         */
/* --------------------------------------------------------------------------------------- */

/// Formats a PHP float the way PHP's string conversion does (14 significant digits).
std::string php_number_to_string(double value) {
    char buffer[64];
    std::snprintf(buffer, sizeof(buffer), "%.14G", value);
    std::string text(buffer);
    for (char &character : text) {
        if (character == 'e') character = 'E';
    }
    return text;
}

/// Maps every argument of a message pattern to the Formattable type php-src expects.
struct MessageArgumentTypes {
    std::map<int32_t, Formattable::Type> by_index;
    std::map<std::string, Formattable::Type> by_name;
};

/// Collects the pattern's argument types, mirroring php-src `umsg_parse_format()`.
bool collect_message_argument_types(const UnicodeString &pattern, MessageArgumentTypes &types,
                                    UErrorCode &status, std::string &parse_detail) {
    UParseError parse_error;
    MessagePattern parsed(pattern, &parse_error, status);
    if (U_FAILURE(status)) {
        parse_detail = parse_error_to_string(parse_error);
        return false;
    }
    for (int32_t index = 0; index < parsed.countParts(); index++) {
        const MessagePattern::Part &start = parsed.getPart(index);
        if (start.getType() != UMSGPAT_PART_TYPE_ARG_START) continue;
        UMessagePatternArgType argument_type = start.getArgType();
        const MessagePattern::Part &name_part = parsed.getPart(++index);
        int32_t argument_index = -1;
        std::string name;
        if (name_part.getType() == UMSGPAT_PART_TYPE_ARG_NAME) {
            argument_index = name_part.getValue();
            name = to_utf8(parsed.getSubstring(name_part));
        } else if (name_part.getType() == UMSGPAT_PART_TYPE_ARG_NUMBER) {
            argument_index = name_part.getValue();
        } else {
            status = U_INVALID_FORMAT_ERROR;
            return false;
        }
        if (argument_index < 0) {
            status = U_INVALID_FORMAT_ERROR;
            return false;
        }
        bool named = name_part.getType() == UMSGPAT_PART_TYPE_ARG_NAME;
        Formattable::Type type = Formattable::kObject;
        if (argument_type == UMSGPAT_ARG_TYPE_NONE) {
            type = Formattable::kString;
        } else if (argument_type == UMSGPAT_ARG_TYPE_SIMPLE) {
            const MessagePattern::Part &type_part = parsed.getPart(++index);
            if (type_part.getType() != UMSGPAT_PART_TYPE_ARG_TYPE) {
                status = U_PARSE_ERROR;
                return false;
            }
            UnicodeString type_name = parsed.getSubstring(type_part);
            if (type_name == UnicodeString("number")) {
                const MessagePattern::Part &style_part = parsed.getPart(index + 1);
                if (style_part.getType() == UMSGPAT_PART_TYPE_ARG_STYLE &&
                    parsed.getSubstring(style_part) == UnicodeString("integer")) {
                    type = Formattable::kInt64;
                } else {
                    type = Formattable::kDouble;
                }
            } else if (type_name == UnicodeString("date") ||
                       type_name == UnicodeString("time")) {
                type = Formattable::kDate;
            } else if (type_name == UnicodeString("spellout") ||
                       type_name == UnicodeString("ordinal") ||
                       type_name == UnicodeString("duration")) {
                type = Formattable::kDouble;
            }
        } else if (argument_type == UMSGPAT_ARG_TYPE_PLURAL ||
                   argument_type == UMSGPAT_ARG_TYPE_CHOICE ||
                   argument_type == UMSGPAT_ARG_TYPE_SELECTORDINAL) {
            type = Formattable::kDouble;
        } else if (argument_type == UMSGPAT_ARG_TYPE_SELECT) {
            type = Formattable::kString;
        } else {
            type = Formattable::kString;
        }
        // Named arguments are keyed by NAME: ICU reports 0 (or the numeric spelling) for the
        // ARG_NAME part, so using that value as an index merges every name onto argument 0 and
        // fabricates an "Inconsistent types" error for select/plural patterns.
        if (named) {
            auto stored = types.by_name.find(name);
            if (stored != types.by_name.end() && stored->second != Formattable::kObject &&
                type != Formattable::kObject && stored->second != type) {
                status = U_ARGUMENT_TYPE_MISMATCH;
                return false;
            }
            types.by_name[name] = type;
        } else {
            auto stored = types.by_index.find(argument_index);
            if (stored != types.by_index.end() && stored->second != Formattable::kObject &&
                type != Formattable::kObject && stored->second != type) {
                status = U_ARGUMENT_TYPE_MISMATCH;
                return false;
            }
            types.by_index[argument_index] = type;
        }
    }
    return true;
}

/// Reads a calendar handle's current instant in milliseconds, when the value is one.
bool calendar_object_millis(int64_t handle, double &out);

/// Converts one PHP value to the Formattable type the pattern declared for it.
bool convert_message_value(const Value &value, Formattable::Type type,
                           const std::string &argument_name, Formattable &out,
                           std::string &error, UErrorCode &status) {
    auto as_string = [&](std::string &text) -> bool {
        switch (value.type) {
            case Value::Type::Str: text = value.text; return true;
            case Value::Type::Int: text = std::to_string(value.integer); return true;
            case Value::Type::Float: text = php_number_to_string(value.number); return true;
            case Value::Type::True: text = "1"; return true;
            case Value::Type::False:
            case Value::Type::Null: text = ""; return true;
            case Value::Type::Array: text = "Array"; return true;
            default: return false;
        }
    };
    switch (type) {
        case Formattable::kString: {
            std::string text;
            if (!as_string(text)) {
                error = "No strategy to convert the value given for the argument with key '" +
                        argument_name + "' is available";
                status = U_ILLEGAL_ARGUMENT_ERROR;
                return false;
            }
            if (!valid_utf8(text)) {
                error = "Invalid UTF-8 data in string argument: '" + text + "'";
                status = U_INVALID_CHAR_FOUND;
                return false;
            }
            out.setString(UnicodeString::fromUTF8(text));
            return true;
        }
        case Formattable::kDouble: {
            switch (value.type) {
                case Value::Type::Int: out.setDouble(double(value.integer)); return true;
                case Value::Type::Float: out.setDouble(value.number); return true;
                case Value::Type::Str: out.setDouble(std::strtod(value.text.c_str(), nullptr)); return true;
                case Value::Type::True: out.setDouble(1.0); return true;
                case Value::Type::False:
                case Value::Type::Null: out.setDouble(0.0); return true;
                case Value::Type::Array: out.setDouble(value.entries.empty() ? 0.0 : 1.0); return true;
                default: break;
            }
            break;
        }
        case Formattable::kLong:
        case Formattable::kInt64: {
            int64_t integer = 0;
            switch (value.type) {
                case Value::Type::Int: integer = value.integer; break;
                case Value::Type::Float: integer = int64_t(value.number); break;
                case Value::Type::Str: integer = std::strtoll(value.text.c_str(), nullptr, 10); break;
                case Value::Type::True: integer = 1; break;
                case Value::Type::False:
                case Value::Type::Null: integer = 0; break;
                default:
                    error = "No strategy to convert the value given for the argument with key '" +
                            argument_name + "' is available";
                    status = U_ILLEGAL_ARGUMENT_ERROR;
                    return false;
            }
            if (type == Formattable::kLong) out.setLong(int32_t(integer));
            else out.setInt64(integer);
            return true;
        }
        case Formattable::kDate: {
            double millis = 0.0;
            switch (value.type) {
                case Value::Type::Int:
                    if (calendar_object_millis(value.integer, millis)) break;
                    millis = double(value.integer) * 1000.0;
                    break;
                case Value::Type::Float: millis = value.number * 1000.0; break;
                case Value::Type::Str: {
                    // php's intl_zval_to_millis() accepts numeric strings as seconds.
                    char *end = nullptr;
                    double seconds = std::strtod(value.text.c_str(), &end);
                    if (end == value.text.c_str() || *end != '\0') {
                        error = "string '" + value.text +
                                "' is not numeric, which would be required for it to be a valid date";
                        status = U_ILLEGAL_ARGUMENT_ERROR;
                        return false;
                    }
                    millis = seconds * 1000.0;
                    break;
                }
                default:
                    error = "The argument for key '" + argument_name +
                            "' cannot be used as a date or time";
                    status = U_ILLEGAL_ARGUMENT_ERROR;
                    return false;
            }
            out.setDate(millis);
            return true;
        }
        case Formattable::kObject:
        default: {
            switch (value.type) {
                case Value::Type::Str: {
                    if (!valid_utf8(value.text)) {
                        error = "Invalid UTF-8 data in string argument: '" + value.text + "'";
                        status = U_INVALID_CHAR_FOUND;
                        return false;
                    }
                    out.setString(UnicodeString::fromUTF8(value.text));
                    return true;
                }
                case Value::Type::Int: out.setInt64(value.integer); return true;
                case Value::Type::Float: out.setDouble(value.number); return true;
                case Value::Type::True: out.setInt64(1); return true;
                case Value::Type::False:
                case Value::Type::Null: out.setInt64(0); return true;
                default:
                    error = "No strategy to convert the value given for the argument with key '" +
                            argument_name + "' is available";
                    status = U_ILLEGAL_ARGUMENT_ERROR;
                    return false;
            }
        }
    }
    error = "No strategy to convert the value given for the argument with key '" +
            argument_name + "' is available";
    status = U_ILLEGAL_ARGUMENT_ERROR;
    return false;
}

/// Formats one PHP argument array through `MessageFormat`'s named-argument entry point.
bool format_message_values(MessageFormat &formatter, const UnicodeString &pattern,
                           const Value &values, const std::string &timezone,
                           UnicodeString &out, std::string &error, UErrorCode &status) {
    if (!timezone.empty()) {
        // php's umsg_set_timezone() applies PHP's default timezone to every date/time
        // subformat before ICU formats the message.
        std::unique_ptr<TimeZone> used(TimeZone::createTimeZone(from_utf8(timezone)));
        int32_t format_count = 0;
        const Format **formats = formatter.getFormats(format_count);
        for (int32_t index = 0; index < format_count; index++) {
            if (auto *date_format = dynamic_cast<DateFormat *>(
                    const_cast<Format *>(formats[index]))) {
                date_format->adoptTimeZone(used->clone());
            }
        }
    }
    MessageArgumentTypes types;
    std::string parse_detail;
    if (!collect_message_argument_types(pattern, types, status, parse_detail)) {
        if (!error.empty()) return false;
        error = status == U_ARGUMENT_TYPE_MISMATCH
            ? "Inconsistent types declared for an argument"
            : "pattern syntax error (" + parse_detail + ")";
        return false;
    }
    std::vector<UnicodeString> names;
    std::vector<Formattable> arguments;
    names.reserve(values.entries.size());
    arguments.reserve(values.entries.size());
    for (const auto &entry : values.entries) {
        std::string name;
        Formattable::Type expected = Formattable::kObject;
        if (entry.first.type == Value::Type::Int) {
            int64_t key = entry.first.integer;
            if (key < 0 || key > INT32_MAX) {
                error = "Found negative or too large array key";
                status = U_ILLEGAL_ARGUMENT_ERROR;
                return false;
            }
            name = std::to_string(key);
            auto found = types.by_index.find(int32_t(key));
            if (found != types.by_index.end()) expected = found->second;
        } else if (entry.first.type == Value::Type::Str) {
            if (!valid_utf8(entry.first.text)) {
                error = "Invalid UTF-8 data in argument key: '" + entry.first.text + "'";
                status = U_INVALID_CHAR_FOUND;
                return false;
            }
            name = entry.first.text;
            auto found = types.by_name.find(name);
            if (found != types.by_name.end()) expected = found->second;
        } else {
            error = "Found unsupported argument key type";
            status = U_ILLEGAL_ARGUMENT_ERROR;
            return false;
        }
        Formattable converted;
        if (!convert_message_value(entry.second, expected, name, converted, error, status)) {
            return false;
        }
        names.push_back(UnicodeString::fromUTF8(name));
        arguments.push_back(converted);
    }
    formatter.format(names.empty() ? nullptr : names.data(),
                     arguments.empty() ? nullptr : arguments.data(),
                     int32_t(arguments.size()), out, status);
    if (U_FAILURE(status)) {
        error = "Call to ICU MessageFormat::format() has failed";
        return false;
    }
    return true;
}

struct MessageFormatterObject : IntlObject {
    std::unique_ptr<MessageFormat> formatter;
    std::string locale;
    std::string pattern;

    static Formattable formattable_from_value(const Value &value, UErrorCode &status) {
        switch (value.type) {
            case Value::Type::Int: return Formattable(int64_t(value.integer));
            case Value::Type::Float: return Formattable(value.number);
            case Value::Type::True: return Formattable(int64_t(1));
            case Value::Type::False: return Formattable(int64_t(0));
            case Value::Type::Str: return Formattable(from_utf8(value.text));
            case Value::Type::Null:
                return Formattable(double(0.0));
            default:
                status = U_ILLEGAL_ARGUMENT_ERROR;
                return Formattable();
        }
    }

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!formatter) {
            record_error(*this, U_INVALID_STATE_ERROR, "MessageFormatter is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        if (method != "error_code" && method != "error_message" && method != "error_context" &&
            method != "__elephc_reprefix_error") {
            record_error(*this, U_ZERO_ERROR, "U_ZERO_ERROR");
        }
        if (method == "format") {
            const Value *values = nullptr;
            if (!argument_array(args, 0, values)) return U_ILLEGAL_ARGUMENT_ERROR;
            std::string timezone;
            optional_string(args, 1, timezone);
            UnicodeString text;
            std::string error;
            if (!format_message_values(*formatter, from_utf8(pattern), *values, timezone, text,
                                       error, status)) {
                record_error(*this, status, "MessageFormatter::format(): " + error);
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            record_error(*this, U_ZERO_ERROR, "U_ZERO_ERROR");
            out = Value::from_string(to_utf8(text));
            return U_ZERO_ERROR;
        }
        if (method == "error_context") {
            out = Value::from_string(error_context);
            return U_ZERO_ERROR;
        }
        if (method == "__elephc_reprefix_error") {
            std::string prefix;
            if (!argument_string(args, 0, prefix)) return U_ILLEGAL_ARGUMENT_ERROR;
            record_error(*this, UErrorCode(error_code), prefix + ": " + error_context);
            out = Value::null();
            return U_ZERO_ERROR;
        }
        if (method == "parse") {
            std::string input;
            if (!argument_string(args, 0, input)) return U_ILLEGAL_ARGUMENT_ERROR;
            ParsePosition position(0);
            int32_t parsed_count = 0;
            std::unique_ptr<Formattable[]> parsed(
                formatter->parse(from_utf8(input), position, parsed_count));
            if (!parsed || position.getErrorIndex() >= 0 || position.getIndex() == 0) {
                record_error(*this, U_PARSE_ERROR, "MessageFormatter::parse()");
                out = Value::boolean(false);
                return U_PARSE_ERROR;
            }
            Value result = Value::array();
            for (int32_t index = 0; index < parsed_count; index++) {
                const Formattable &value = parsed[index];
                Value item;
                switch (value.getType()) {
                    case Formattable::kLong: item = Value::from_int(value.getLong()); break;
                    case Formattable::kInt64: item = Value::from_int(value.getInt64()); break;
                    case Formattable::kDouble: item = Value::from_double(value.getDouble()); break;
                    case Formattable::kDate:
                        item = Value::from_double(double(value.getDate()) / 1000.0);
                        break;
                    case Formattable::kString: item = Value::from_string(to_utf8(value.getString())); break;
                    default: item = Value::null(); break;
                }
                result.entries.emplace_back(Value::from_int(index), std::move(item));
            }
            out = std::move(result);
            return U_ZERO_ERROR;
        }
        if (method == "set_pattern") {
            std::string pattern;
            if (!argument_string(args, 0, pattern)) return U_ILLEGAL_ARGUMENT_ERROR;
            UParseError parse_error = {0};
            formatter->applyPattern(from_utf8(pattern), parse_error, status);
            if (U_FAILURE(status)) {
                char buffer[128];
                std::snprintf(buffer, sizeof(buffer),
                    "Error setting symbol value at line %d, offset %d",
                    parse_error.line, parse_error.offset);
                record_error(*this, status, std::string(buffer));
                out = Value::boolean(false);
                return status;
            }
            this->pattern = pattern;
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "pattern") {
            UnicodeString pattern_text;
            formatter->toPattern(pattern_text);
            out = Value::from_string(to_utf8(pattern_text));
            return U_ZERO_ERROR;
        }
        if (method == "locale") {
            out = Value::from_string(formatter->getLocale().getName());
            return U_ZERO_ERROR;
        }
        if (method == "error_code") {
            out = Value::from_int(error_code);
            return U_ZERO_ERROR;
        }
        if (method == "error_message") {
            out = Value::from_string(error_message);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported MessageFormatter method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<MessageFormatterObject>();
        copy->formatter.reset(static_cast<MessageFormat *>(formatter->clone()));
        copy->locale = locale;
        copy->pattern = pattern;
        return copy;
    }
};

/* --------------------------------------------------------------------------------------- */
/* Transliterator                                                                           */
/* --------------------------------------------------------------------------------------- */

struct TransliteratorObject : IntlObject {
    std::unique_ptr<Transliterator> transliterator;
    std::string id;

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!transliterator) {
            record_error(*this, U_INVALID_STATE_ERROR, "Transliterator is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        // php-src resets the process-wide error on entry and the object error through
        // INTL_METHOD_FETCH_OBJECT, except for the accessors that report the last error.
        if (method != "error_code" && method != "error_message" && method != "id") {
            record_error(*this, U_ZERO_ERROR, "U_ZERO_ERROR");
        }
        if (method == "__elephc_set_error") {
            int64_t code = 0;
            std::string message;
            if (!argument_int(args, 0, code) || !argument_string(args, 1, message)) {
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            record_error(*this, UErrorCode(code), message);
            out = Value::null();
            return U_ZERO_ERROR;
        }
        if (method == "__elephc_utf16_length") {
            std::string text;
            if (!argument_string(args, 0, text)) return U_ILLEGAL_ARGUMENT_ERROR;
            out = Value::from_int(int64_t(from_utf8(text).length()));
            return U_ZERO_ERROR;
        }
        if (method == "transliterate") {
            std::string input;
            if (!argument_string(args, 0, input)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_utf8(input)) {
                record_error(*this, U_INVALID_CHAR_FOUND,
                    "Transliterator::transliterate(): String conversion of string to UTF-16 failed");
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            int64_t start = 0, end = -1;
            optional_int(args, 1, 0, start);
            optional_int(args, 2, -1, end);
            UnicodeString text = from_utf8(input);
            int32_t length = text.length();
            if (start < 0) start = 0;
            if (end < 0 || end > length) end = length;
            if (start > length) start = length;
            if (end < start) end = start;
            transliterator->transliterate(text, int32_t(start), int32_t(end));
            out = Value::from_string(to_utf8(text));
            return U_ZERO_ERROR;
        }
        if (method == "id") {
            out = Value::from_string(id);
            return U_ZERO_ERROR;
        }
        if (method == "error_code") {
            out = Value::from_int(error_code);
            return U_ZERO_ERROR;
        }
        if (method == "error_message") {
            out = Value::from_string(error_message);
            return U_ZERO_ERROR;
        }
        if (method == "create_inverse") {
            UErrorCode inverse_status = U_ZERO_ERROR;
            Transliterator *inverse = transliterator->createInverse(inverse_status);
            if (U_FAILURE(inverse_status) || inverse == nullptr) return U_ZERO_ERROR;
            auto object = std::make_unique<TransliteratorObject>();
            object->transliterator.reset(inverse);
            std::string inverse_id = id;
            size_t dash = inverse_id.find('-');
            if (dash != std::string::npos) {
                inverse_id = inverse_id.substr(dash + 1) + "-" + inverse_id.substr(0, dash);
            }
            object->id = inverse_id;
            out = wrap_object(insert(std::move(object)), "Transliterator");
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported Transliterator method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<TransliteratorObject>();
        copy->transliterator.reset(transliterator->clone());
        copy->id = id;
        return copy;
    }
};

/* --------------------------------------------------------------------------------------- */
/* Spoofchecker                                                                             */
/* --------------------------------------------------------------------------------------- */

struct SpoofCheckerObject : IntlObject {
    USpoofChecker *checker = nullptr;

    ~SpoofCheckerObject() override {
        if (checker != nullptr) uspoof_close(checker);
    }

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (checker == nullptr) {
            record_error(*this, U_INVALID_STATE_ERROR, "Spoofchecker is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        if (method == "is_suspicious" || method == "are_confusable") {
            std::string left, right;
            if (!argument_string(args, 0, left)) return U_ILLEGAL_ARGUMENT_ERROR;
            int32_t checks = 0;
            if (method == "are_confusable") {
                if (!argument_string(args, 1, right)) return U_ILLEGAL_ARGUMENT_ERROR;
                checks = uspoof_areConfusableUTF8(checker, left.data(),
                    int32_t(left.size()), right.data(), int32_t(right.size()), &status);
            } else {
                UBool result = uspoof_checkUTF8(checker, left.data(), int32_t(left.size()),
                                                nullptr, &status);
                checks = result != 0 ? 1 : 0;
            }
            if (U_FAILURE(status)) {
                record_error(*this, status, "Spoofchecker");
                Value failed = Value::array();
                failed.entries = {
                    {Value::from_int(0), Value::boolean(true)},
                    {Value::from_int(1), Value::from_int(0)},
                };
                out = std::move(failed);
                return U_ZERO_ERROR;
            }
            Value result = Value::array();
            result.entries = {
                {Value::from_int(0), Value::boolean(checks != 0)},
                {Value::from_int(1), Value::from_int(checks)},
            };
            out = std::move(result);
            return U_ZERO_ERROR;
        }
        if (method == "set_checks" || method == "get_checks") {
            if (method == "set_checks") {
                int64_t checks = 0;
                if (!argument_int(args, 0, checks)) return U_ILLEGAL_ARGUMENT_ERROR;
                uspoof_setChecks(checker, int32_t(checks), &status);
                out = Value::boolean(U_SUCCESS(status));
            } else {
                out = Value::from_int(uspoof_getChecks(checker, &status));
            }
            if (U_FAILURE(status)) {
                record_error(*this, status, "Spoofchecker checks");
                return status;
            }
            return U_ZERO_ERROR;
        }
        if (method == "set_allowed_locales") {
            std::string locales;
            if (!argument_string(args, 0, locales)) return U_ILLEGAL_ARGUMENT_ERROR;
            uspoof_setAllowedLocales(checker, locales.c_str(), &status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Spoofchecker::setAllowedLocales()");
                return status;
            }
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "set_allowed_chars") {
            std::string pattern;
            int64_t options = 0;
            if (!argument_string(args, 0, pattern)) return U_ILLEGAL_ARGUMENT_ERROR;
            optional_int(args, 1, 0, options);
            if (pattern.empty() || pattern.front() != '[' || pattern.back() != ']') {
                out = Value::from_string(
                    "Spoofchecker::setAllowedChars(): Argument #1 ($pattern) must be a valid "
                    "regular expression character set pattern");
                return U_ZERO_ERROR;
            }
            const int64_t ignore_space = USET_IGNORE_SPACE;
            const int64_t case_insensitive = USET_CASE_INSENSITIVE;
            const int64_t add_case_mappings = USET_ADD_CASE_MAPPINGS;
            const int64_t simple_case_insensitive = USET_SIMPLE_CASE_INSENSITIVE;
            bool valid_option = options == 0 ||
                options == ignore_space ||
                options == (ignore_space | case_insensitive) ||
                options == (ignore_space | add_case_mappings) ||
                options == (ignore_space | simple_case_insensitive);
            if (!valid_option) {
                out = Value::from_string(
                    "Spoofchecker::setAllowedChars(): Argument #2 ($patternOptions) must be a "
                    "valid pattern option, 0 or (SpoofChecker::IGNORE_SPACE|(<none> or "
                    "SpoofChecker::CASE_INSENSITIVE or SpoofChecker::ADD_CASE_MAPPINGS or "
                    "SpoofChecker::SIMPLE_CASE_INSENSITIVE))");
                return U_ZERO_ERROR;
            }
            UnicodeString utf16 = from_utf8(pattern);
            UErrorCode set_status = U_ZERO_ERROR;
            USet *set = uset_openPatternOptions(utf16.getBuffer(), utf16.length(),
                                                uint32_t(options), &set_status);
            if (U_FAILURE(set_status) || set == nullptr) {
                if (set != nullptr) uset_close(set);
                std::string message =
                    "Spoofchecker::setAllowedChars(): Argument #1 ($pattern) must be a valid "
                    "regular expression character set pattern";
                out = Value::from_string(message);
                return U_ZERO_ERROR;
            }
            uspoof_setAllowedChars(checker, set, &status);
            uset_close(set);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Spoofchecker::setAllowedChars()");
                return status;
            }
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "set_restriction_level" || method == "get_restriction_level") {
            if (method == "set_restriction_level") {
                int64_t level = 0;
                if (!argument_int(args, 0, level)) return U_ILLEGAL_ARGUMENT_ERROR;
                uspoof_setRestrictionLevel(checker, URestrictionLevel(level));
                out = Value::boolean(true);
            } else {
                out = Value::from_int(uspoof_getRestrictionLevel(checker));
            }
            return U_ZERO_ERROR;
        }
        if (method == "get_skeleton") {
            int64_t type = 0;
            optional_int(args, 1, 0, type);
            std::string input;
            if (!argument_string(args, 0, input)) return U_ILLEGAL_ARGUMENT_ERROR;
            int32_t needed = uspoof_getSkeletonUTF8(checker, int32_t(type), input.data(),
                int32_t(input.size()), nullptr, 0, &status);
            if (status == U_BUFFER_OVERFLOW_ERROR) status = U_ZERO_ERROR;
            if (U_FAILURE(status)) {
                record_error(*this, status, "Spoofchecker::getSkeleton()");
                out = Value::boolean(false);
                return status;
            }
            std::string skeleton(size_t(needed), '\0');
            uspoof_getSkeletonUTF8(checker, int32_t(type), input.data(), int32_t(input.size()),
                skeleton.data(), needed, &status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Spoofchecker::getSkeleton()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::from_string(skeleton);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported Spoofchecker method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        UErrorCode clone_status = U_ZERO_ERROR;
        auto copy = std::make_unique<SpoofCheckerObject>();
        copy->checker = uspoof_clone(checker, &clone_status);
        return copy;
    }
};

/* --------------------------------------------------------------------------------------- */
/* Time zones                                                                               */
/* --------------------------------------------------------------------------------------- */

struct TimeZoneObject : IntlObject {
    std::unique_ptr<TimeZone> timezone;

    int32_t call(const std::string &method_in, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        std::string method = method_in;
        if (method == "getID") method = "get_id";
        else if (method == "getRawOffset") method = "get_raw_offset";
        else if (method == "getDSTSavings") method = "get_dst_savings";
        else if (method == "getDisplayName") method = "get_display_name";
        else if (method == "getOffset") method = "get_offset";
        else if (method == "hasSameRules") method = "has_same_rules";
        else if (method == "useDaylightTime") method = "use_daylight_time";
        if (!timezone) {
            record_error(*this, U_INVALID_STATE_ERROR, "IntlTimeZone is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        if (method == "get_id") {
            UnicodeString id;
            timezone->getID(id);
            out = Value::from_string(to_utf8(id));
            return U_ZERO_ERROR;
        }
        if (method == "get_raw_offset") {
            out = Value::from_int(timezone->getRawOffset());
            return U_ZERO_ERROR;
        }
        if (method == "get_dst_savings") {
            out = Value::from_int(timezone->getDSTSavings());
            return U_ZERO_ERROR;
        }
        if (method == "debug_offset") {
            int32_t raw = 0, dst = 0;
            UErrorCode offset_status = U_ZERO_ERROR;
            timezone->getOffset(Calendar::getNow(), false, raw, dst, offset_status);
            if (U_FAILURE(offset_status)) {
                record_error(*this, offset_status, "IntlTimeZone::__debugInfo()");
                out = Value::boolean(false);
                return offset_status;
            }
            Value result = Value::array();
            result.entries = {
                {Value::from_int(0), Value::from_int(raw)},
                {Value::from_int(1), Value::from_int(dst)},
            };
            out = std::move(result);
            return U_ZERO_ERROR;
        }
        if (method == "use_daylight_time") {
            out = Value::boolean(timezone->useDaylightTime());
            return U_ZERO_ERROR;
        }
        if (method == "get_offset") {
            double timestamp = 0.0;
            bool local = false;
            if (!argument_double(args, 0, timestamp) || !argument_bool(args, 1, local))
                return U_ILLEGAL_ARGUMENT_ERROR;
            int32_t raw = 0, dst = 0;
            timezone->getOffset(UDate(timestamp), UBool(local), raw, dst, status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlTimeZone::getOffset()");
                out = Value::boolean(false);
                return status;
            }
            Value result = Value::array();
            result.entries = {
                {Value::from_int(0), Value::boolean(true)},
                {Value::from_int(1), Value::from_int(raw)},
                {Value::from_int(2), Value::from_int(dst)},
            };
            out = std::move(result);
            return U_ZERO_ERROR;
        }
        if (method == "get_display_name") {
            bool dst = false;
            int64_t style = 2; // TimeZone::LONG is PHP's default display style
            std::string locale;
            optional_bool(args, 0, false, dst);
            optional_int(args, 1, 2, style);
            if (style < 1 || style > 8) {
                record_error(*this, U_ILLEGAL_ARGUMENT_ERROR,
                             "IntlTimeZone::getDisplayName(): wrong display type");
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            bool has_locale = optional_string(args, 2, locale);
            UnicodeString name;
            timezone->getDisplayName(UBool(dst), icu::TimeZone::EDisplayType(style),
                                      has_locale ? Locale(locale.c_str()) : Locale::getDefault(),
                                      name);
            out = Value::from_string(to_utf8(name));
            return U_ZERO_ERROR;
        }
        if (method == "has_same_rules") {
            if (args.empty() || args[0].type != Value::Type::Int) {
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            IntlObject *other = lookup(args[0].integer);
            auto *other_timezone = dynamic_cast<TimeZoneObject *>(other);
            if (other_timezone == nullptr || !other_timezone->timezone) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            out = Value::boolean(timezone->hasSameRules(*other_timezone->timezone));
            return U_ZERO_ERROR;
        }
        if (method == "get_equivalent_id") {
            int64_t offset = 0;
            if (!argument_int(args, 0, offset)) return U_ILLEGAL_ARGUMENT_ERROR;
            UnicodeString id;
            timezone->getID(id);
            UnicodeString equivalent = TimeZone::getEquivalentID(id, int32_t(offset));
            out = Value::from_string(to_utf8(equivalent));
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported IntlTimeZone method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<TimeZoneObject>();
        copy->timezone.reset(timezone->clone());
        return copy;
    }
};

/// Inserts a cloned ICU time zone into the handle table and returns its handle.
int64_t wrap_time_zone(const TimeZone &zone) {
    auto object = std::make_unique<TimeZoneObject>();
    object->timezone.reset(zone.clone());
    if (!object->timezone) return 0;
    return insert(std::move(object));
}

/* --------------------------------------------------------------------------------------- */
/* Calendar                                                                                 */
/* --------------------------------------------------------------------------------------- */

struct CalendarObject : IntlObject {
    std::unique_ptr<Calendar> calendar;

    /// Returns whether one PHP field number is a valid ICU calendar field.
    static bool valid_field(int64_t field) {
        return field >= 0 && field < UCAL_FIELD_COUNT;
    }

    /// Returns whether one PHP day-of-week number is a valid ICU weekday.
    static bool valid_day_of_week(int64_t day) {
        return day >= UCAL_SUNDAY && day <= UCAL_SATURDAY;
    }

    /// Formats a PHP argument-validation message using the OO or procedural caller name.
    static std::string argument_error(const std::string &method, int index,
                                      const std::string &name, const std::string &suffix) {
        bool procedural = method.rfind("intlcal_", 0) == 0 ||
                          method.rfind("intlgregcal_", 0) == 0;
        std::string caller = procedural ? method + "()" : "IntlCalendar::" + method + "()";
        return caller + ": Argument #" + std::to_string(index + (procedural ? 1 : 0)) +
               " ($" + name + ") " + suffix;
    }

    /// Records a PHP argument-validation message without appending the ICU status name.
    static void record_argument_error(CalendarObject &object, const std::string &message) {
        object.error_code = U_ILLEGAL_ARGUMENT_ERROR;
        object.error_message = message;
    }

    /// Stores one field result or records the ICU status and returns false.
    static int32_t field_result(CalendarObject &object, int32_t value, UErrorCode status,
                                Value &out) {
        if (U_FAILURE(status)) {
            record_error(object, status, "Call to ICU method has failed");
            out = Value::boolean(false);
            return status;
        }
        out = Value::from_int(value);
        return U_ZERO_ERROR;
    }

    int32_t call(const std::string &method_in, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!calendar) {
            record_error(*this, U_INVALID_STATE_ERROR, "IntlCalendar is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        // Procedural entry points share this object body; strip their prefix and normalize
        // the few names whose bridge spelling differs from the PHP method name.
        std::string method = method_in;
        for (const char *prefix : {"intlcal_", "intlgregcal_"}) {
            size_t length = std::strlen(prefix);
            if (method.rfind(prefix, 0) == 0) {
                method = method.substr(length);
                break;
            }
        }
        if (method == "get_locale") method = "locale";
        else if (method == "get_time_zone") method = "get_timezone";
        else if (method == "set_time_zone") method = "set_timezone";
        else if (method == "get_error_code") method = "error_code";
        else if (method == "get_error_message") method = "error_message";
        else if (method == "get_gregorian_change") method = "gregorian_change";
        else if (method == "set_gregorian_change") method = "set_gregorian_change";
        if (method == "get") {
            int64_t field = 0;
            if (!argument_int(args, 0, field)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_field(field)) {
                record_argument_error(*this,
                             argument_error(method_in, 1, "field", "must be a valid field"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            return field_result(*this, calendar->get(UCalendarDateFields(field), status),
                                status, out);
        }
        if (method == "get_time") {
            UDate value = calendar->getTime(status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "error calling ICU Calendar::getTime");
                out = Value::boolean(false);
                return status;
            }
            // PHP exposes ICU milliseconds, not seconds.
            out = Value::from_double(double(value));
            return U_ZERO_ERROR;
        }
        if (method == "set_time") {
            double timestamp = 0.0;
            if (!argument_double(args, 0, timestamp)) return U_ILLEGAL_ARGUMENT_ERROR;
            calendar->setTime(timestamp, status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Call to underlying method failed");
                out = Value::boolean(false);
                return status;
            }
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "add") {
            int64_t field = 0, amount = 0;
            if (!argument_int(args, 0, field) || !argument_int(args, 1, amount))
                return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_field(field)) {
                record_argument_error(*this,
                             argument_error(method_in, 1, "field", "must be a valid field"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            if (amount < INT32_MIN || amount > INT32_MAX) {
                record_argument_error(*this,
                             argument_error(method_in, 2, "value",
                                            "must be between -2147483648 and 2147483647"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            calendar->add(UCalendarDateFields(field), int32_t(amount), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Call to underlying method failed");
                out = Value::boolean(false);
                return status;
            }
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "roll") {
            int64_t field = 0, amount = 0;
            if (!argument_int(args, 0, field) || !argument_int(args, 1, amount))
                return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_field(field)) {
                record_argument_error(*this,
                             argument_error(method_in, 1, "field", "must be a valid field"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            if (amount < INT32_MIN || amount > INT32_MAX) {
                record_argument_error(*this,
                             argument_error(method_in, 2, "value",
                                            "must be between -2147483648 and 2147483647"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            calendar->roll(UCalendarDateFields(field), int32_t(amount), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Error calling ICU Calendar::roll");
                out = Value::boolean(false);
                return status;
            }
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "set" || method == "set_date" || method == "set_date_time") {
            int64_t year = 0, month = 0, day = 1, hour = 0, minute = 0, second = 0;
            size_t count = args.size();
            if (method == "set" && count == 2) {
                int64_t field = 0, value = 0;
                if (!argument_int(args, 0, field) || !argument_int(args, 1, value))
                    return U_ILLEGAL_ARGUMENT_ERROR;
                if (!valid_field(field)) {
                    record_argument_error(*this,
                                 argument_error(method_in, 1, "field", "must be a valid field"));
                    out = Value::boolean(false);
                    return U_ILLEGAL_ARGUMENT_ERROR;
                }
                calendar->set(UCalendarDateFields(field), int32_t(value));
                out = Value::boolean(true);
                return U_ZERO_ERROR;
            }
            if (method == "set" && count == 4) {
                record_error(*this, U_ILLEGAL_ARGUMENT_ERROR,
                             "IntlCalendar::set() has no variant with exactly 4 parameters");
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            if (!argument_int(args, 0, year) || !argument_int(args, 1, month))
                return U_ILLEGAL_ARGUMENT_ERROR;
            optional_int(args, 2, 1, day);
            optional_int(args, 3, 0, hour);
            optional_int(args, 4, 0, minute);
            optional_int(args, 5, 0, second);
            const int64_t int32_min = INT32_MIN, int32_max = INT32_MAX;
            for (const auto &entry : std::vector<std::pair<int, std::pair<int64_t, const char *>>>{
                     {1, {year, "year"}}, {2, {month, "month"}}, {3, {day, "dayOfMonth"}},
                     {4, {hour, "hour"}}, {5, {minute, "minute"}}, {6, {second, "second"}}}) {
                if (entry.second.first < int32_min || entry.second.first > int32_max) {
                    record_argument_error(*this,
                                 argument_error(method_in, entry.first, entry.second.second,
                                                "must be between -2147483648 and 2147483647"));
                    out = Value::boolean(false);
                    return U_ILLEGAL_ARGUMENT_ERROR;
                }
            }
            if (count <= 3 || method == "set_date") {
                calendar->set(int32_t(year), int32_t(month), int32_t(day));
            } else if (count == 5) {
                calendar->set(int32_t(year), int32_t(month), int32_t(day), int32_t(hour),
                              int32_t(minute));
            } else {
                calendar->set(int32_t(year), int32_t(month), int32_t(day), int32_t(hour),
                              int32_t(minute), int32_t(second));
            }
            if (method == "set_date_time") {
                // php's constructor sets the wall clock exactly; ICU otherwise keeps the
                // calendar's current millisecond field.
                calendar->set(UCAL_MILLISECOND, 0);
            }
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "clear") {
            int64_t field = -1;
            optional_int(args, 0, -1, field);
            if (field < 0) {
                calendar->clear();
            } else {
                if (!valid_field(field)) {
                    record_argument_error(*this,
                                 argument_error(method_in, 1, "field", "must be a valid field"));
                    out = Value::boolean(false);
                    return U_ILLEGAL_ARGUMENT_ERROR;
                }
                calendar->clear(UCalendarDateFields(field));
            }
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "is_set") {
            int64_t field = 0;
            if (!argument_int(args, 0, field)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_field(field)) {
                record_argument_error(*this,
                             argument_error(method_in, 1, "field", "must be a valid field"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            out = Value::boolean(calendar->isSet(UCalendarDateFields(field)));
            return U_ZERO_ERROR;
        }
        if (method == "field_difference") {
            double timestamp = 0.0;
            int64_t field = 0;
            if (!argument_double(args, 0, timestamp) || !argument_int(args, 1, field))
                return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_field(field)) {
                record_argument_error(*this,
                             argument_error(method_in, 2, "field", "must be a valid field"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            int32_t difference =
                calendar->fieldDifference(UDate(timestamp), UCalendarDateFields(field), status);
            if (U_FAILURE(status)) {
                record_error(*this, status,
                             "IntlCalendar::fieldDifference(): Call to ICU method has failed");
                out = Value::boolean(false);
                return status;
            }
            out = Value::from_int(difference);
            return U_ZERO_ERROR;
        }
        if (method == "get_first_day_of_week") {
            return field_result(*this, int32_t(calendar->getFirstDayOfWeek(status)), status, out);
        }
        if (method == "set_first_day_of_week") {
            int64_t day = 0;
            if (!argument_int(args, 0, day)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_day_of_week(day)) {
                record_argument_error(*this,
                             argument_error(method_in, 1, "dayOfWeek", "must be a valid day of the week"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            calendar->setFirstDayOfWeek(UCalendarDaysOfWeek(day));
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "get_minimal_days_in_first_week") {
            return field_result(*this, calendar->getMinimalDaysInFirstWeek(), U_ZERO_ERROR, out);
        }
        if (method == "set_minimal_days_in_first_week") {
            int64_t days = 0;
            if (!argument_int(args, 0, days)) return U_ILLEGAL_ARGUMENT_ERROR;
            calendar->setMinimalDaysInFirstWeek(int32_t(days));
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "get_minimum" || method == "get_maximum" ||
            method == "get_greatest_minimum" || method == "get_least_maximum" ||
            method == "get_actual_minimum" || method == "get_actual_maximum") {
            int64_t field = 0;
            if (!argument_int(args, 0, field)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_field(field)) {
                record_argument_error(*this,
                             argument_error(method_in, 1, "field", "must be a valid field"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            UCalendarDateFields date_field = UCalendarDateFields(field);
            int32_t value = 0;
            if (method == "get_minimum") value = calendar->getMinimum(date_field);
            else if (method == "get_maximum") value = calendar->getMaximum(date_field);
            else if (method == "get_greatest_minimum") value = calendar->getGreatestMinimum(date_field);
            else if (method == "get_least_maximum") value = calendar->getLeastMaximum(date_field);
            else if (method == "get_actual_minimum") value = calendar->getActualMinimum(date_field, status);
            else value = calendar->getActualMaximum(date_field, status);
            return field_result(*this, value, status, out);
        }
        if (method == "get_day_of_week_type") {
            int64_t day = 0;
            if (!argument_int(args, 0, day)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_day_of_week(day)) {
                record_argument_error(*this,
                             argument_error(method_in, 1, "dayOfWeek", "must be a valid day of the week"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            UCalendarWeekdayType type =
                calendar->getDayOfWeekType(UCalendarDaysOfWeek(day), status);
            return field_result(*this, int32_t(type), status, out);
        }
        if (method == "get_weekend_transition") {
            int64_t day = 0;
            if (!argument_int(args, 0, day)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_day_of_week(day)) {
                record_argument_error(*this,
                             argument_error(method_in, 1, "dayOfWeek", "must be a valid day of the week"));
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            return field_result(*this,
                                calendar->getWeekendTransition(UCalendarDaysOfWeek(day), status),
                                status, out);
        }
        if (method == "is_weekend") {
            if (args.empty() || args[0].type == Value::Type::Null) {
                out = Value::boolean(calendar->isWeekend());
                return U_ZERO_ERROR;
            }
            double timestamp = 0.0;
            if (!argument_double(args, 0, timestamp)) return U_ILLEGAL_ARGUMENT_ERROR;
            UBool weekend = calendar->isWeekend(UDate(timestamp), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "Error calling ICU method");
                out = Value::boolean(false);
                return status;
            }
            out = Value::boolean(weekend);
            return U_ZERO_ERROR;
        }
        if (method == "get_repeated_wall_time_option") {
            out = Value::from_int(int32_t(calendar->getRepeatedWallTimeOption()));
            return U_ZERO_ERROR;
        }
        if (method == "set_repeated_wall_time_option") {
            int64_t option = 0;
            if (!argument_int(args, 0, option)) return U_ILLEGAL_ARGUMENT_ERROR;
            calendar->setRepeatedWallTimeOption(UCalendarWallTimeOption(option));
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "get_skipped_wall_time_option") {
            out = Value::from_int(int32_t(calendar->getSkippedWallTimeOption()));
            return U_ZERO_ERROR;
        }
        if (method == "set_skipped_wall_time_option") {
            int64_t option = 0;
            if (!argument_int(args, 0, option)) return U_ILLEGAL_ARGUMENT_ERROR;
            calendar->setSkippedWallTimeOption(UCalendarWallTimeOption(option));
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "get_type") {
            out = Value::from_string(calendar->getType());
            return U_ZERO_ERROR;
        }
        if (method == "locale") {
            int64_t type = ULOC_ACTUAL_LOCALE;
            optional_int(args, 0, ULOC_ACTUAL_LOCALE, type);
            Locale actual = calendar->getLocale(ULocDataLocaleType(type), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlCalendar::getLocale()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::from_string(actual.getName());
            return U_ZERO_ERROR;
        }
        if (method == "set_lenient") {
            bool lenient = false;
            if (!argument_bool(args, 0, lenient)) return U_ILLEGAL_ARGUMENT_ERROR;
            calendar->setLenient(lenient);
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "is_lenient") {
            out = Value::boolean(calendar->isLenient());
            return U_ZERO_ERROR;
        }
        if (method == "in_daylight_time") {
            out = Value::boolean(calendar->inDaylightTime(status));
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlCalendar::inDaylightTime()");
                return status;
            }
            return U_ZERO_ERROR;
        }
        if (method == "is_equivalent_to" || method == "equals") {
            if (args.empty()) return U_ILLEGAL_ARGUMENT_ERROR;
            if (args[0].type != Value::Type::Int) return U_ILLEGAL_ARGUMENT_ERROR;
            IntlObject *other = lookup(args[0].integer);
            auto *other_calendar = dynamic_cast<CalendarObject *>(other);
            if (other_calendar == nullptr || !other_calendar->calendar) {
                record_error(*this, U_ILLEGAL_ARGUMENT_ERROR,
                             "IntlCalendar comparison: invalid calendar argument");
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            if (method == "is_equivalent_to") {
                out = Value::boolean(calendar->isEquivalentTo(*other_calendar->calendar));
                return U_ZERO_ERROR;
            }
            UBool equal = calendar->equals(*other_calendar->calendar, status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlCalendar::equals()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::boolean(equal);
            return U_ZERO_ERROR;
        }
        if (method == "after" || method == "before") {
            if (args.empty() || args[0].type != Value::Type::Int) return U_ILLEGAL_ARGUMENT_ERROR;
            IntlObject *other = lookup(args[0].integer);
            auto *other_calendar = dynamic_cast<CalendarObject *>(other);
            if (other_calendar == nullptr || !other_calendar->calendar) {
                record_error(*this, U_ILLEGAL_ARGUMENT_ERROR,
                             "IntlCalendar comparison: invalid calendar argument");
                out = Value::boolean(false);
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            UBool related = method == "after"
                ? calendar->after(*other_calendar->calendar, status)
                : calendar->before(*other_calendar->calendar, status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlCalendar comparison");
                out = Value::boolean(false);
                return status;
            }
            out = Value::boolean(related);
            return U_ZERO_ERROR;
        }
        if (method == "get_timezone") {
            TimeZone *zone = calendar->getTimeZone().clone();
            if (zone == nullptr) {
                record_error(*this, U_MEMORY_ALLOCATION_ERROR, "could not clone TimeZone");
                out = Value::boolean(false);
                return U_MEMORY_ALLOCATION_ERROR;
            }
            auto timezone = std::make_unique<TimeZoneObject>();
            timezone->timezone.reset(zone);
            int64_t handle = insert(std::move(timezone));
            Value wrapper = Value::array();
            wrapper.entries = {
                {Value::from_string("__elephc_object"), Value::from_int(handle)},
                {Value::from_string("__elephc_class"), Value::from_string("IntlTimeZone")},
            };
            out = std::move(wrapper);
            return U_ZERO_ERROR;
        }
        if (method == "set_timezone") {
            std::string zone;
            if (!argument_string(args, 0, zone)) return U_ILLEGAL_ARGUMENT_ERROR;
            TimeZone *timezone = TimeZone::createTimeZone(from_utf8(zone));
            if (timezone == nullptr) {
                record_error(*this, U_MEMORY_ALLOCATION_ERROR, "could not create TimeZone");
                out = Value::boolean(false);
                return U_MEMORY_ALLOCATION_ERROR;
            }
            calendar->adoptTimeZone(timezone);
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "gregorian_change") {
            auto *gregorian = dynamic_cast<icu::GregorianCalendar *>(calendar.get());
            if (gregorian == nullptr) {
                record_error(*this, U_UNSUPPORTED_ERROR,
                             "gregorian change is only available on IntlGregorianCalendar");
                out = Value::boolean(false);
                return U_UNSUPPORTED_ERROR;
            }
            out = Value::from_double(double(gregorian->getGregorianChange()));
            return U_ZERO_ERROR;
        }
        if (method == "set_gregorian_change") {
            auto *gregorian = dynamic_cast<icu::GregorianCalendar *>(calendar.get());
            if (gregorian == nullptr) {
                record_error(*this, U_UNSUPPORTED_ERROR,
                             "gregorian change is only available on IntlGregorianCalendar");
                out = Value::boolean(false);
                return U_UNSUPPORTED_ERROR;
            }
            double timestamp = 0.0;
            if (!argument_double(args, 0, timestamp)) return U_ILLEGAL_ARGUMENT_ERROR;
            gregorian->setGregorianChange(UDate(timestamp), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlGregorianCalendar::setGregorianChange()");
                out = Value::boolean(false);
                return status;
            }
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "is_leap_year") {
            auto *gregorian = dynamic_cast<icu::GregorianCalendar *>(calendar.get());
            if (gregorian == nullptr) {
                record_error(*this, U_UNSUPPORTED_ERROR,
                             "isLeapYear is only available on IntlGregorianCalendar");
                out = Value::boolean(false);
                return U_UNSUPPORTED_ERROR;
            }
            int64_t year = 0;
            if (!argument_int(args, 0, year)) return U_ILLEGAL_ARGUMENT_ERROR;
            out = Value::boolean(gregorian->isLeapYear(int32_t(year)));
            return U_ZERO_ERROR;
        }
        if (method == "__elephc_set_error") {
            int64_t code = 0;
            std::string message;
            if (!argument_int(args, 0, code) || !argument_string(args, 1, message)) {
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            record_error(*this, UErrorCode(code), message);
            out = Value::null();
            return U_ZERO_ERROR;
        }
        if (method == "error_code") {
            out = Value::from_int(error_code);
            return U_ZERO_ERROR;
        }
        if (method == "error_message") {
            out = Value::from_string(error_message);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported IntlCalendar method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<CalendarObject>();
        if (calendar) copy->calendar.reset(calendar->clone());
        copy->error_code = error_code;
        copy->error_message = error_message;
        return copy;
    }
};

/// Inserts a cloned ICU calendar into the handle table and returns its handle.
int64_t wrap_calendar(const Calendar &calendar) {
    auto object = std::make_unique<CalendarObject>();
    object->calendar.reset(calendar.clone());
    if (!object->calendar) return 0;
    return insert(std::move(object));
}

/// Adopts a cloned calendar from one handle into a date formatter.
bool adopt_calendar_from_handle(DateFormat &formatter, int64_t handle) {
    IntlObject *other = lookup(handle);
    auto *other_calendar = dynamic_cast<CalendarObject *>(other);
    if (other_calendar == nullptr || !other_calendar->calendar) return false;
    formatter.adoptCalendar(other_calendar->calendar->clone());
    return true;
}

/* --------------------------------------------------------------------------------------- */
/* Break iterator                                                                           */
/* --------------------------------------------------------------------------------------- */

struct BreakIteratorObject : IntlObject {
    std::unique_ptr<BreakIterator> iterator;
    UnicodeString text;
    /// PHP's `IntlCodePointBreakIterator` walks UTF-16 code points instead of ICU boundaries.
    bool code_point = false;
    bool text_started = false;
    std::string utf8_text;
    int32_t position = 0;
    int32_t last_code_point = U_SENTINEL;

    /// Advances one UTF-8 code point, recording it for `getLastCodePoint()`.
    int32_t code_point_next() {
        if (position >= int32_t(utf8_text.size())) return BreakIterator::DONE;
        int32_t index = position;
        UChar32 code = 0;
        U8_NEXT(utf8_text.data(), index, int32_t(utf8_text.size()), code);
        if (code < 0) return BreakIterator::DONE;
        position = index;
        last_code_point = code;
        return position;
    }

    /// Retreats one UTF-8 code point, recording it for `getLastCodePoint()`.
    int32_t code_point_previous() {
        if (position <= 0) return BreakIterator::DONE;
        int32_t start = position - 1;
        while (start > 0 &&
               (static_cast<unsigned char>(utf8_text[size_t(start)]) & 0xC0) == 0x80) {
            start--;
        }
        int32_t index = start;
        UChar32 code = 0;
        U8_NEXT(utf8_text.data(), index, int32_t(utf8_text.size()), code);
        if (code < 0) return BreakIterator::DONE;
        position = start;
        last_code_point = code;
        return position;
    }

    /// Converts one UTF-16 code-unit index into PHP's UTF-8 byte offset.
    int32_t utf16_to_utf8_offset(int32_t index) {
        int32_t units = 0;
        int32_t bytes = 0;
        int32_t length = int32_t(utf8_text.size());
        while (units < index && bytes < length) {
            UChar32 code = 0;
            U8_NEXT(utf8_text.data(), bytes, length, code);
            if (code < 0) break;
            units += U16_LENGTH(code);
        }
        return bytes;
    }

    /// Converts one UTF-8 byte offset into the UTF-16 code-unit index ICU uses.
    int32_t utf8_to_utf16_index(int32_t offset) {
        int32_t units = 0;
        int32_t bytes = 0;
        int32_t length = int32_t(utf8_text.size());
        while (bytes < offset && bytes < length) {
            UChar32 code = 0;
            U8_NEXT(utf8_text.data(), bytes, length, code);
            if (code < 0) break;
            units += U16_LENGTH(code);
        }
        return units;
    }

    /// Runs the `IntlCodePointBreakIterator` surface over the retained UTF-16 text.
    int32_t call_code_point(const std::string &method, const std::vector<Value> &args,
                            Value &out) {
        if (method == "first") {
            position = 0;
            last_code_point = U_SENTINEL;
            out = Value::from_int(0);
            return U_ZERO_ERROR;
        }
        if (method == "last") {
            position = int32_t(utf8_text.size());
            last_code_point = U_SENTINEL;
            out = Value::from_int(position);
            return U_ZERO_ERROR;
        }
        if (method == "current") {
            out = Value::from_int(position);
            return U_ZERO_ERROR;
        }
        if (method == "get_last_code_point") {
            out = Value::from_int(last_code_point);
            return U_ZERO_ERROR;
        }
        if (method == "next") {
            int64_t amount = 1;
            if (!args.empty() && args[0].type != Value::Type::Null &&
                !argument_int(args, 0, amount)) {
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            int32_t result = position;
            while (amount > 0) {
                result = code_point_next();
                if (result == BreakIterator::DONE) break;
                amount--;
            }
            while (amount < 0) {
                result = code_point_previous();
                if (result == BreakIterator::DONE) break;
                amount++;
            }
            out = Value::from_int(result == BreakIterator::DONE ? -1 : result);
            return U_ZERO_ERROR;
        }
        if (method == "previous") {
            int32_t result = code_point_previous();
            out = Value::from_int(result == BreakIterator::DONE ? -1 : result);
            return U_ZERO_ERROR;
        }
        if (method == "following") {
            int64_t offset = 0;
            if (!argument_int(args, 0, offset)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (offset < 0 || offset >= int64_t(utf8_text.size())) {
                out = Value::from_int(BreakIterator::DONE);
                return U_ZERO_ERROR;
            }
            position = int32_t(offset);
            int32_t result = code_point_next();
            out = Value::from_int(result == BreakIterator::DONE ? -1 : result);
            return U_ZERO_ERROR;
        }
        if (method == "preceding") {
            int64_t offset = 0;
            if (!argument_int(args, 0, offset)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (offset <= 0) {
                out = Value::from_int(BreakIterator::DONE);
                return U_ZERO_ERROR;
            }
            position = int32_t(std::min<int64_t>(offset, int64_t(utf8_text.size())));
            int32_t result = code_point_previous();
            out = Value::from_int(result == BreakIterator::DONE ? -1 : result);
            return U_ZERO_ERROR;
        }
        if (method == "is_boundary") {
            int64_t offset = 0;
            if (!argument_int(args, 0, offset)) return U_ILLEGAL_ARGUMENT_ERROR;
            bool boundary = offset >= 0 && offset <= int64_t(utf8_text.size()) &&
                (offset == int64_t(utf8_text.size()) ||
                 (static_cast<unsigned char>(utf8_text[size_t(offset)]) & 0xC0) != 0x80);
            out = Value::boolean(boundary);
            return U_ZERO_ERROR;
        }
        if (method == "get_locale") {
            int64_t type = ULOC_ACTUAL_LOCALE;
            optional_int(args, 0, ULOC_ACTUAL_LOCALE, type);
            UErrorCode status = U_ZERO_ERROR;
            Locale actual = iterator->getLocale(ULocDataLocaleType(type), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlBreakIterator::getLocale()");
                return status;
            }
            out = Value::from_string(actual.getName());
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR,
                     "unsupported IntlCodePointBreakIterator method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!iterator) {
            record_error(*this, U_INVALID_STATE_ERROR, "IntlBreakIterator is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        if (method == "set_text") {
            std::string input;
            if (!argument_string(args, 0, input)) return U_ILLEGAL_ARGUMENT_ERROR;
            text = from_utf8(input);
            utf8_text = input;
            text_started = true;
            iterator->setText(text);
            position = 0;
            last_code_point = U_SENTINEL;
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (code_point && method == "get_text") {
            if (!text_started) {
                out = Value::null();
                return U_ZERO_ERROR;
            }
            out = Value::from_string(utf8_text);
            return U_ZERO_ERROR;
        }
        if (code_point && method != "get_text" && method != "get_rules" &&
            method != "get_binary_rules" && method != "get_rule_status" &&
            method != "get_rule_status_vec") {
            return call_code_point(method, args, out);
        }
        if (method == "get_text") {
            if (!text_started) {
                out = Value::null();
                return U_ZERO_ERROR;
            }
            out = Value::from_string(to_utf8(text));
            return U_ZERO_ERROR;
        }
        if (method == "debug_type") {
            // php-src exposes `typeid(*biter).name()` in the debug info.
            out = Value::from_string(typeid(*iterator).name());
            return U_ZERO_ERROR;
        }
        if (method == "first" || method == "last" || method == "next" || method == "previous" ||
            method == "following" || method == "preceding" || method == "current") {
            int32_t position = 0;
            if (method == "first") position = iterator->first();
            else if (method == "last") position = iterator->last();
            else if (method == "next") {
                // PHP's `next(?int $offset)` advances that many boundaries (ICU's
                // `BreakIterator::next(int32_t)`); a null offset advances one.
                int64_t amount = 1;
                if (!args.empty() && args[0].type != Value::Type::Null &&
                    !argument_int(args, 0, amount)) {
                    return U_ILLEGAL_ARGUMENT_ERROR;
                }
                position = iterator->next(int32_t(amount));
            }
            else if (method == "previous") position = iterator->previous();
            else if (method == "following") {
                int64_t offset = 0;
                if (!argument_int(args, 0, offset)) return U_ILLEGAL_ARGUMENT_ERROR;
                position = iterator->following(offset < 0
                    ? int32_t(offset) : utf8_to_utf16_index(int32_t(offset)));
            } else if (method == "preceding") {
                int64_t offset = 0;
                if (!argument_int(args, 0, offset)) return U_ILLEGAL_ARGUMENT_ERROR;
                // Offsets past the text stay past it: ICU's preceding() then returns the last
                // boundary, which is what PHP reports (mapping them to the text length would
                // step one boundary too far back).
                position = iterator->preceding(offset > int64_t(utf8_text.size())
                    ? int32_t(offset) : utf8_to_utf16_index(int32_t(offset)));
            } else {
                position = iterator->current();
            }
            out = Value::from_int(position == BreakIterator::DONE
                ? -1 : utf16_to_utf8_offset(position));
            return U_ZERO_ERROR;
        }
        if (method == "is_boundary") {
            int64_t offset = 0;
            if (!argument_int(args, 0, offset)) return U_ILLEGAL_ARGUMENT_ERROR;
            bool boundary = offset >= 0 && offset <= int64_t(utf8_text.size()) &&
                iterator->isBoundary(utf8_to_utf16_index(int32_t(offset)));
            out = Value::boolean(boundary);
            return U_ZERO_ERROR;
        }
        if (method == "get_locale") {
            int64_t type = ULOC_ACTUAL_LOCALE;
            optional_int(args, 0, ULOC_ACTUAL_LOCALE, type);
            Locale actual = iterator->getLocale(ULocDataLocaleType(type), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlBreakIterator::getLocale()");
                return status;
            }
            out = Value::from_string(actual.getName());
            return U_ZERO_ERROR;
        }
        if (method == "get_rule_status") {
            auto *rule_based = dynamic_cast<icu::RuleBasedBreakIterator *>(iterator.get());
            out = Value::from_int(rule_based == nullptr ? 0 : rule_based->getRuleStatus());
            return U_ZERO_ERROR;
        }
        if (method == "get_rules") {
            auto *rule_based = dynamic_cast<icu::RuleBasedBreakIterator *>(iterator.get());
            if (rule_based == nullptr) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            out = Value::from_string(to_utf8(rule_based->getRules()));
            return U_ZERO_ERROR;
        }
        if (method == "get_binary_rules") {
            auto *rule_based = dynamic_cast<icu::RuleBasedBreakIterator *>(iterator.get());
            if (rule_based == nullptr) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            uint32_t length = 0;
            const uint8_t *rules = rule_based->getBinaryRules(length);
            out = Value::from_string(std::string(reinterpret_cast<const char *>(rules),
                                                 size_t(length)));
            return U_ZERO_ERROR;
        }
        if (method == "get_rule_status_vec") {
            auto *rule_based = dynamic_cast<icu::RuleBasedBreakIterator *>(iterator.get());
            if (rule_based == nullptr) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            int32_t capacity = 8;
            std::vector<int32_t> values;
            values.resize(static_cast<size_t>(capacity));
            int32_t count = rule_based->getRuleStatusVec(values.data(), capacity, status);
            if (status == U_BUFFER_OVERFLOW_ERROR) {
                status = U_ZERO_ERROR;
                values.resize(size_t(count));
                count = rule_based->getRuleStatusVec(values.data(), count, status);
            }
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlRuleBasedBreakIterator::getRuleStatusVec()");
                out = Value::boolean(false);
                return status;
            }
            Value result = Value::array();
            for (int32_t index = 0; index < count; index++) {
                result.entries.emplace_back(Value::from_int(index),
                                            Value::from_int(values[size_t(index)]));
            }
            out = std::move(result);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported IntlBreakIterator method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<BreakIteratorObject>();
        copy->iterator.reset(iterator->clone());
        copy->text = text;
        if (copy->iterator) copy->iterator->setText(text);
        return copy;
    }
};

/* --------------------------------------------------------------------------------------- */
/* Resource bundle                                                                          */
/* --------------------------------------------------------------------------------------- */

struct ResourceBundleObject : IntlObject {
    std::unique_ptr<ResourceBundle> bundle;

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!bundle) {
            record_error(*this, U_INVALID_STATE_ERROR, "ResourceBundle is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        if (method == "count") {
            out = Value::from_int(bundle->getSize());
            return U_ZERO_ERROR;
        }
        if (method == "keys") {
            // ICU's ResourceBundle iterates tables through hasNext()/getNext(); getKey()
            // names the resource returned by that iteration step.
            std::unique_ptr<ResourceBundle> iterator(bundle->clone());
            iterator->resetIterator();
            Value result = Value::array();
            int32_t index = 0;
            while (iterator->hasNext()) {
                UErrorCode key_status = U_ZERO_ERROR;
                ResourceBundle next = iterator->getNext(key_status);
                if (U_FAILURE(key_status)) break;
                const char *key = next.getKey();
                result.entries.emplace_back(Value::from_int(index),
                                            Value::from_string(key != nullptr
                                                ? std::string(key)
                                                : std::to_string(index)));
                index++;
            }
            out = std::move(result);
            return U_ZERO_ERROR;
        }
        if (method == "get") {
            if (args.empty()) return U_ILLEGAL_ARGUMENT_ERROR;
            std::unique_ptr<ResourceBundle> entry;
            if (args[0].type == Value::Type::Int) {
                entry.reset(new ResourceBundle(bundle->get(int32_t(args[0].integer), status)));
            } else if (args[0].type == Value::Type::Str) {
                // Array elements are indexed; a numeric-string key addresses the index.
                char *end = nullptr;
                long long numeric = std::strtoll(args[0].text.c_str(), &end, 10);
                if (!args[0].text.empty() && end != nullptr && *end == '\0') {
                    entry.reset(new ResourceBundle(bundle->get(int32_t(numeric), status)));
                } else {
                    entry.reset(new ResourceBundle(bundle->get(args[0].text.c_str(), status)));
                }
            } else {
                return U_ILLEGAL_ARGUMENT_ERROR;
            }
            if (U_FAILURE(status) || !entry) {
                std::string element = args[0].type == Value::Type::Str
                    ? args[0].text : std::to_string(args[0].integer);
                record_error(*this, status == U_ZERO_ERROR ? U_MISSING_RESOURCE_ERROR : status,
                    "Cannot load resource element '" + element + "'");
                out = Value::null();
                return U_ZERO_ERROR;
            }
            switch (entry->getType()) {
                case URES_STRING: out = Value::from_string(to_utf8(entry->getString(status))); break;
                case URES_INT: out = Value::from_int(entry->getInt(status)); break;
                case URES_BINARY: {
                    int32_t length = 0;
                    const uint8_t *bytes = entry->getBinary(length, status);
                    out = U_SUCCESS(status) ? Value::from_string(
                        std::string(reinterpret_cast<const char *>(bytes), size_t(length)))
                                            : Value::null();
                    break;
                }
                case URES_INT_VECTOR: {
                    int32_t length = 0;
                    const int32_t *values = entry->getIntVector(length, status);
                    Value array = Value::array();
                    for (int32_t index = 0; index < length; index++) {
                        array.entries.emplace_back(Value::from_int(index),
                            Value::from_int(values[index]));
                    }
                    out = std::move(array);
                    break;
                }
                case URES_ARRAY: {
                    // php wraps a resource array in a ResourceBundle, like a table; only the
                    // primitive int vector below becomes a PHP array.
                    auto sub = std::make_unique<ResourceBundleObject>();
                    sub->bundle = std::move(entry);
                    out = wrap_object(insert(std::move(sub)), "ResourceBundle");
                    break;
                }
                case URES_TABLE: {
                    auto sub = std::make_unique<ResourceBundleObject>();
                    sub->bundle = std::move(entry);
                    out = wrap_object(insert(std::move(sub)), "ResourceBundle");
                    break;
                }
                default:
                    out = Value::null();
                    break;
            }
            return U_ZERO_ERROR;
        }
        if (method == "error_code") {
            out = Value::from_int(error_code);
            return U_ZERO_ERROR;
        }
        if (method == "error_message") {
            out = Value::from_string(error_message);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported ResourceBundle method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<ResourceBundleObject>();
        copy->bundle.reset(new ResourceBundle(*bundle));
        return copy;
    }
};

/* --------------------------------------------------------------------------------------- */
/* UConverter                                                                               */
/* --------------------------------------------------------------------------------------- */

struct ConverterObject : IntlObject {
    UConverter *converter = nullptr;
    std::string source;
    std::string destination;
    std::string subst_chars = "?";

    ~ConverterObject() override {
        if (converter != nullptr) ucnv_close(converter);
    }

    /// Returns the canonical ICU name for an encoding, or an empty string.
    static std::string canonical_name(const std::string &name, UErrorCode &status) {
        UConverter *probe = ucnv_open(name.empty() ? nullptr : name.c_str(), &status);
        if (U_FAILURE(status) || probe == nullptr) return {};
        const char *canonical = ucnv_getName(probe, &status);
        std::string result = canonical == nullptr ? name : canonical;
        ucnv_close(probe);
        return result;
    }

    /// Converts bytes between the configured source and destination encodings.
    bool convert_bytes(const std::string &from, const std::string &to, const std::string &input,
                       std::string &output) {
        UErrorCode local = U_ZERO_ERROR;
        int32_t needed = ucnv_convert(to.c_str(), from.c_str(), nullptr, 0,
                                      input.data(), int32_t(input.size()), &local);
        if (local == U_BUFFER_OVERFLOW_ERROR) local = U_ZERO_ERROR;
        if (U_FAILURE(local)) {
            record_error(*this, local, "UConverter::convert()");
            return false;
        }
        output.assign(size_t(needed), '\0');
        ucnv_convert(to.c_str(), from.c_str(), output.data(), needed,
                     input.data(), int32_t(input.size()), &local);
        if (U_FAILURE(local)) {
            record_error(*this, local, "UConverter::convert()");
            return false;
        }
        // ucnv_convert does not run the object's substitution callback; PHP's converter
        // reports the configured substitution string (default "?") instead of U+001A.
        if (!subst_chars.empty()) {
            size_t position = 0;
            while ((position = output.find('\x1A', position)) != std::string::npos) {
                output.replace(position, 1, subst_chars);
                position += subst_chars.size();
            }
        }
        return true;
    }

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (method == "convert") {
            std::string input;
            bool reverse = false;
            if (!argument_string(args, 0, input)) return U_ILLEGAL_ARGUMENT_ERROR;
            optional_bool(args, 1, false, reverse);
            std::string from = reverse ? destination : source;
            std::string to = reverse ? source : destination;
            std::string output;
            if (from.empty()) from = "UTF-8";
            if (to.empty()) to = "UTF-8";
            if (!convert_bytes(from, to, input, output)) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            out = Value::from_string(output);
            return U_ZERO_ERROR;
        }
        if (method == "get_source_encoding" || method == "get_destination_encoding") {
            const std::string &name = method == "get_source_encoding" ? source : destination;
            UErrorCode name_status = U_ZERO_ERROR;
            std::string canonical = canonical_name(name, name_status);
            if (U_FAILURE(name_status)) {
                record_error(*this, name_status, "UConverter encoding");
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            out = Value::from_string(canonical);
            return U_ZERO_ERROR;
        }
        if (method == "set_source_encoding" || method == "set_destination_encoding") {
            std::string name;
            if (!argument_string(args, 0, name)) return U_ILLEGAL_ARGUMENT_ERROR;
            UErrorCode name_status = U_ZERO_ERROR;
            std::string canonical = canonical_name(name, name_status);
            if (U_FAILURE(name_status) || canonical.empty()) {
                record_error(*this, name_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                                : name_status,
                             "UConverter encoding");
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            if (method == "set_source_encoding") source = canonical;
            else destination = canonical;
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "get_source_type" || method == "get_destination_type") {
            const std::string &name = method == "get_source_type" ? source : destination;
            UErrorCode type_status = U_ZERO_ERROR;
            UConverter *probe = ucnv_open(name.empty() ? nullptr : name.c_str(), &type_status);
            if (U_FAILURE(type_status) || probe == nullptr) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            out = Value::from_int(int64_t(ucnv_getType(probe)));
            ucnv_close(probe);
            return U_ZERO_ERROR;
        }
        if (method == "set_subst_chars") {
            std::string chars;
            if (!argument_string(args, 0, chars)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (chars.empty() || chars.size() > 4) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            UConverter *probe = ucnv_open(destination.empty() ? nullptr : destination.c_str(),
                                          &status);
            if (U_FAILURE(status) || probe == nullptr) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            ucnv_setSubstChars(probe, chars.data(), int8_t(chars.size()), &status);
            ucnv_close(probe);
            if (U_FAILURE(status)) {
                record_error(*this, status, "UConverter::setSubstChars()");
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            subst_chars = chars;
            out = Value::boolean(true);
            return U_ZERO_ERROR;
        }
        if (method == "get_subst_chars") {
            UConverter *probe = ucnv_open(destination.empty() ? nullptr : destination.c_str(),
                                          &status);
            if (U_FAILURE(status) || probe == nullptr) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            char chars[8];
            int8_t length = 0;
            ucnv_getSubstChars(probe, chars, &length, &status);
            ucnv_close(probe);
            if (U_FAILURE(status)) {
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            out = Value::from_string(subst_chars);
            return U_ZERO_ERROR;
        }
        if (method == "get_aliases") {
            std::string name;
            if (!argument_string(args, 0, name)) return U_ILLEGAL_ARGUMENT_ERROR;
            return converter_aliases(name, out);
        }
        if (method == "error_code") {
            out = Value::from_int(error_code);
            return U_ZERO_ERROR;
        }
        if (method == "error_message") {
            out = Value::from_string(error_message);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported UConverter method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    /// Fills `out` with the canonical name and aliases of an encoding, or false.
    static int32_t converter_aliases(const std::string &name, Value &out) {
        UErrorCode status = U_ZERO_ERROR;
        uint16_t count = ucnv_countAliases(name.c_str(), &status);
        if (U_FAILURE(status) || count == 0) {
            out = Value::boolean(false);
            return U_ZERO_ERROR;
        }
        Value result = Value::array();
        for (uint16_t index = 0; index < count; index++) {
            const char *alias = ucnv_getAlias(name.c_str(), index, &status);
            if (U_FAILURE(status) || alias == nullptr) break;
            result.entries.emplace_back(Value::from_int(int64_t(index)),
                                        Value::from_string(alias));
        }
        out = std::move(result);
        return U_ZERO_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<ConverterObject>();
        copy->source = source;
        copy->destination = destination;
        UErrorCode status = U_ZERO_ERROR;
        copy->converter = ucnv_open(destination.empty() ? nullptr : destination.c_str(), &status);
        if (U_FAILURE(status)) copy->converter = nullptr;
        return copy;
    }
};

struct DatePatternGeneratorObject : IntlObject {
    std::unique_ptr<icu::DateTimePatternGenerator> generator;

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!generator) {
            record_error(*this, U_INVALID_STATE_ERROR,
                         "IntlDatePatternGenerator is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        if (method == "best_pattern") {
            std::string skeleton;
            if (!argument_string(args, 0, skeleton)) return U_ILLEGAL_ARGUMENT_ERROR;
            if (!valid_utf8(skeleton)) {
                record_error(*this, U_INVALID_CHAR_FOUND,
                             "IntlDatePatternGenerator::getBestPattern(): Skeleton is not a valid UTF-8 string");
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            UnicodeString pattern = generator->getBestPattern(from_utf8(skeleton), status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlDatePatternGenerator::getBestPattern()");
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            record_error(*this, U_ZERO_ERROR, "U_ZERO_ERROR");
            out = Value::from_string(to_utf8(pattern));
            return U_ZERO_ERROR;
        }
        if (method == "error_code") {
            out = Value::from_int(error_code);
            return U_ZERO_ERROR;
        }
        if (method == "error_message") {
            out = Value::from_string(error_message);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR,
                     "unsupported IntlDatePatternGenerator method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        auto copy = std::make_unique<DatePatternGeneratorObject>();
        if (generator) copy->generator.reset(generator->clone());
        return copy;
    }
};

struct ListFormatterObject : IntlObject {
    std::unique_ptr<icu::ListFormatter> formatter;

    int32_t call(const std::string &method, const std::vector<Value> &args, Value &out) override {
        UErrorCode status = U_ZERO_ERROR;
        if (!formatter) {
            record_error(*this, U_INVALID_STATE_ERROR, "IntlListFormatter is not initialized");
            return U_INVALID_STATE_ERROR;
        }
        if (method == "format") {
            const Value *values = nullptr;
            if (!argument_array(args, 0, values)) return U_ILLEGAL_ARGUMENT_ERROR;
            std::vector<UnicodeString> strings;
            strings.reserve(values->entries.size());
            for (const auto &entry : values->entries) {
                const Value &value = entry.second;
                std::string text;
                if (value.type == Value::Type::Str) {
                    if (!valid_utf8(value.text)) {
                        record_error(*this, U_INVALID_CHAR_FOUND,
                                     "IntlListFormatter::format(): Failed to convert string to UTF-16");
                        out = Value::boolean(false);
                        return U_ZERO_ERROR;
                    }
                    text = value.text;
                } else if (value.type == Value::Type::Int) {
                    text = std::to_string(value.integer);
                } else if (value.type == Value::Type::Float) {
                    char buffer[64];
                    std::snprintf(buffer, sizeof(buffer), "%.15g", value.number);
                    text = buffer;
                } else if (value.type == Value::Type::True) {
                    text = "1";
                } else if (value.type == Value::Type::False) {
                    text.clear();
                } else if (value.type == Value::Type::Null) {
                    text.clear();
                } else {
                    text = value.text;
                }
                strings.push_back(from_utf8(text));
            }
            UnicodeString result;
            formatter->format(strings.data(), int32_t(strings.size()), result, status);
            if (U_FAILURE(status)) {
                record_error(*this, status, "IntlListFormatter::format()");
                out = Value::boolean(false);
                return U_ZERO_ERROR;
            }
            record_error(*this, U_ZERO_ERROR, "U_ZERO_ERROR");
            out = Value::from_string(to_utf8(result));
            return U_ZERO_ERROR;
        }
        if (method == "error_code") {
            out = Value::from_int(error_code);
            return U_ZERO_ERROR;
        }
        if (method == "error_message") {
            out = Value::from_string(error_message);
            return U_ZERO_ERROR;
        }
        record_error(*this, U_UNSUPPORTED_ERROR, "unsupported IntlListFormatter method " + method);
        return U_UNSUPPORTED_ERROR;
    }

    std::unique_ptr<IntlObject> clone() const override {
        return nullptr;
    }
};

/* --------------------------------------------------------------------------------------- */
/* Static Intl families                                                                     */
/* --------------------------------------------------------------------------------------- */

const Normalizer2 *normalizer_for(int64_t form, UErrorCode &status) {
    // PHP publishes the Normalizer::FORM_* values (16/4/32/8/48), not ICU's
    // UNormalization2Mode ordinals.
    switch (form) {
        case 4: return Normalizer2::getNFDInstance(status);
        case 8: return Normalizer2::getNFKDInstance(status);
        case 32: return Normalizer2::getNFKCInstance(status);
        case 48: return Normalizer2::getNFKCCasefoldInstance(status);
        default: return Normalizer2::getNFCInstance(status);
    }
}

Value normalize_value(const std::vector<Value> &args, bool test_only, bool raw) {
    std::string input;
    int64_t form = 16;
    if (!argument_string(args, 0, input)) return Value::boolean(false);
    optional_int(args, 1, 16, form);
    UErrorCode status = U_ZERO_ERROR;
    const Normalizer2 *normalizer = normalizer_for(form, status);
    if (U_FAILURE(status)) {
        set_global_error(status, "Normalizer");
        return Value::boolean(false);
    }
    UnicodeString text = from_utf8(input);
    if (test_only) {
        UBool normalized = normalizer->isNormalized(text, status);
        if (U_FAILURE(status)) return Value::boolean(false);
        set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
        return Value::boolean(normalized);
    }
    if (raw) {
        // php requires exactly one UTF-8 code point and reports out-of-range bytes distinctly.
        int32_t offset = 0;
        int32_t count = 0;
        UChar32 codepoint = 0;
        while (offset < int32_t(input.size())) {
            U8_NEXT(input.data(), offset, int32_t(input.size()), codepoint);
            if (codepoint < 0) {
                set_global_error(U_ILLEGAL_ARGUMENT_ERROR, "Code point out of range");
                return Value::null();
            }
            count++;
        }
        if (count != 1) {
            set_global_error(U_ILLEGAL_ARGUMENT_ERROR,
                "Input string must be exactly one UTF-8 encoded code point long.");
            return Value::null();
        }
        UnicodeString result;
        set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
        if (normalizer->getRawDecomposition(codepoint, result)) {
            return Value::from_string(to_utf8(result));
        }
        return Value::null();
    }
    UnicodeString result;
    normalizer->normalize(text, result, status);
    if (U_FAILURE(status)) {
        set_global_error(status, "Normalizer");
        return Value::boolean(false);
    }
    set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
    return Value::from_string(to_utf8(result));
}

std::vector<int32_t> grapheme_boundaries(const std::string &input, UErrorCode &status) {
    std::vector<int32_t> boundaries;
    std::unique_ptr<BreakIterator> iterator(
        BreakIterator::createCharacterInstance(Locale::getDefault(), status));
    if (U_FAILURE(status)) return boundaries;
    UnicodeString text = from_utf8(input);
    iterator->setText(text);
    for (int32_t position = iterator->first(); position != BreakIterator::DONE;
         position = iterator->next()) {
        boundaries.push_back(position);
    }
    return boundaries;
}

Value grapheme_operation(const std::string &operation, const std::vector<Value> &args) {
    UErrorCode status = U_ZERO_ERROR;
    set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
    std::string input;
    if (!argument_string(args, 0, input)) return Value::boolean(false);
    std::vector<int32_t> boundaries = grapheme_boundaries(input, status);
    if (U_FAILURE(status)) return Value::boolean(false);
    size_t clusters = boundaries.size() >= 2 ? boundaries.size() - 1 : 0;
    UnicodeString text = from_utf8(input);
    if (operation == "grapheme_strlen") {
        return Value::from_int(int64_t(clusters));
    }
    if (operation == "grapheme_str_split") {
        int64_t length = 1;
        optional_int(args, 1, 1, length);
        if (length < 1) return Value::boolean(false);
        Value result = Value::array();
        size_t index = 0;
        for (size_t cluster = 0; cluster < clusters; cluster += size_t(length)) {
            int32_t start = boundaries[cluster];
            int32_t end = boundaries[std::min(cluster + size_t(length), clusters)];
            result.entries.emplace_back(Value::from_int(int64_t(index++)),
                Value::from_string(to_utf8(text.tempSubStringBetween(start, end))));
        }
        return result;
    }
    if (operation == "grapheme_substr") {
        int64_t offset = 0;
        if (!argument_int(args, 1, offset)) return Value::boolean(false);
        int64_t length = 0;
        bool has_length = args.size() > 2 && args[2].type != Value::Type::Null;
        if (has_length && !argument_int(args, 2, length)) return Value::boolean(false);
        int64_t count = int64_t(clusters);
        if (offset < 0) offset = count + offset;
        if (offset < 0) offset = 0;
        if (offset > count) return Value::from_string("");
        int64_t end = count;
        if (has_length) {
            if (length < 0) end = count + length;
            else end = offset + length;
            if (end < offset) end = offset;
            if (end > count) end = count;
        }
        int32_t start_byte = boundaries[size_t(offset)];
        int32_t end_byte = boundaries[size_t(end)];
        return Value::from_string(to_utf8(text.tempSubStringBetween(start_byte, end_byte)));
    }
    if (operation == "grapheme_strpos" || operation == "grapheme_stripos" ||
        operation == "grapheme_strrpos" || operation == "grapheme_strripos") {
        std::string needle_value;
        if (!argument_string(args, 1, needle_value)) return Value::boolean(false);
        int64_t offset = 0;
        optional_int(args, 2, 0, offset);
        std::string locale;
        optional_string(args, 3, locale);
        bool reverse = operation.find("strrpos") != std::string::npos ||
                       operation.find("strripos") != std::string::npos;
        bool insensitive = operation.find("stripos") != std::string::npos ||
                           operation.find("strripos") != std::string::npos;
        int64_t raw_offset = offset;
        bool negative_offset = raw_offset < 0;
        if (offset < 0) offset = int64_t(clusters) + offset;
        if (offset < 0 || offset > int64_t(clusters)) return Value::boolean(false);
        if (needle_value.empty()) {
            // php answers the UTF-16 length of the haystack for a reverse search from the
            // start, and the grapheme offset otherwise.
            if (reverse && !negative_offset) return Value::from_int(int64_t(text.length()));
            return Value::from_int(offset);
        }
        UErrorCode search_status = U_ZERO_ERROR;
        UnicodeString haystack = text;
        UnicodeString needle = from_utf8(needle_value);
        std::unique_ptr<UBreakIterator, decltype(&ubrk_close)> iterator(
            ubrk_open(UBRK_CHARACTER, locale.c_str(), nullptr, 0, &search_status),
            &ubrk_close);
        if (U_FAILURE(search_status) || !iterator) {
            set_global_error(search_status,
                operation + "(): Error creating search object");
            return Value::boolean(false);
        }
        ubrk_setText(iterator.get(), haystack.getBuffer(), haystack.length(), &search_status);
        std::unique_ptr<UStringSearch, decltype(&usearch_close)> search(
            usearch_open(needle.getBuffer(), needle.length(), haystack.getBuffer(),
                         haystack.length(), locale.c_str(), iterator.get(), &search_status),
            &usearch_close);
        if (U_FAILURE(search_status) || !search) {
            set_global_error(search_status,
                operation + "(): Error creating search object");
            return Value::boolean(false);
        }
        if (insensitive) {
            UCollator *collator = usearch_getCollator(search.get());
            ucol_setAttribute(collator, UCOL_STRENGTH, UCOL_SECONDARY, &search_status);
            usearch_reset(search.get());
        }
        int32_t start = boundaries[size_t(offset)];
        int32_t found = USEARCH_DONE;
        if (reverse) {
            if (!negative_offset) {
                found = usearch_last(search.get(), &search_status);
                if (found < start) found = USEARCH_DONE;
            } else {
                int32_t previous = USEARCH_DONE;
                for (;;) {
                    int32_t candidate = usearch_next(search.get(), &search_status);
                    if (candidate == USEARCH_DONE || candidate > start) {
                        found = previous;
                        break;
                    }
                    previous = candidate;
                }
            }
        } else {
            usearch_setOffset(search.get(), start, &search_status);
            found = usearch_next(search.get(), &search_status);
        }
        if (U_FAILURE(search_status)) {
            set_global_error(search_status, operation + "(): Error looking up string");
            return Value::boolean(false);
        }
        if (found == USEARCH_DONE) return Value::boolean(false);
        size_t cluster_index = 0;
        for (size_t index = 0; index < clusters; index++) {
            if (boundaries[index] <= found) cluster_index = index;
        }
        return Value::from_int(int64_t(cluster_index));
    }
    if (operation == "grapheme_strstr" || operation == "grapheme_stristr") {
        std::string needle_value;
        if (!argument_string(args, 1, needle_value)) return Value::boolean(false);
        bool before = false;
        optional_bool(args, 2, false, before);
        std::string locale;
        optional_string(args, 3, locale);
        bool insensitive = operation == "grapheme_stristr";
        if (needle_value.empty()) {
            return Value::from_string(before ? "" : input);
        }
        UErrorCode search_status = U_ZERO_ERROR;
        UnicodeString haystack = text;
        UnicodeString needle = from_utf8(needle_value);
        std::unique_ptr<UBreakIterator, decltype(&ubrk_close)> iterator(
            ubrk_open(UBRK_CHARACTER, locale.c_str(), nullptr, 0, &search_status),
            &ubrk_close);
        if (U_FAILURE(search_status) || !iterator) {
            set_global_error(search_status, operation + "(): Error creating search object");
            return Value::boolean(false);
        }
        ubrk_setText(iterator.get(), haystack.getBuffer(), haystack.length(), &search_status);
        std::unique_ptr<UStringSearch, decltype(&usearch_close)> search(
            usearch_open(needle.getBuffer(), needle.length(), haystack.getBuffer(),
                         haystack.length(), locale.c_str(), iterator.get(), &search_status),
            &usearch_close);
        if (U_FAILURE(search_status) || !search) {
            set_global_error(search_status, operation + "(): Error creating search object");
            return Value::boolean(false);
        }
        if (insensitive) {
            UCollator *collator = usearch_getCollator(search.get());
            ucol_setAttribute(collator, UCOL_STRENGTH, UCOL_SECONDARY, &search_status);
            usearch_reset(search.get());
        }
        int32_t found = usearch_next(search.get(), &search_status);
        if (U_FAILURE(search_status)) {
            set_global_error(search_status, operation + "(): Error looking up string");
            return Value::boolean(false);
        }
        if (found == USEARCH_DONE) return Value::boolean(false);
        if (before) return Value::from_string(to_utf8(text.tempSubStringBetween(0, found)));
        return Value::from_string(to_utf8(text.tempSubString(found)));
    }
    if (operation == "grapheme_extract") {
        int64_t size = 0, extract_type = 0, offset = 0;
        if (!argument_int(args, 1, size)) return Value::boolean(false);
        optional_int(args, 2, 0, extract_type);
        optional_int(args, 3, 0, offset);
        if (offset < 0 || offset > int32_t(input.size())) return Value::boolean(false);
        auto utf16_to_byte = [&](int32_t index) -> int32_t {
            std::string prefix;
            text.tempSubString(0, index).toUTF8String(prefix);
            return int32_t(prefix.size());
        };
        // Convert the byte start offset to a UTF-16 boundary index.
        int32_t start = 0;
        if (offset > 0) {
            int32_t units = 0;
            int32_t bytes = 0;
            while (bytes < offset && bytes < int32_t(input.size())) {
                UChar32 code = 0;
                U8_NEXT(input.data(), bytes, int32_t(input.size()), code);
                if (code < 0) return Value::boolean(false);
                units += U16_LENGTH(code);
            }
            start = units;
        }
        int32_t end = start;
        for (size_t index = 0; index + 1 < boundaries.size(); index++) {
            if (boundaries[index] >= start) {
                size_t cluster = index;
                size_t consumed = 0;
                while (cluster < clusters && consumed < size_t(size)) {
                    int32_t next = boundaries[cluster + 1];
                    int32_t bytes = utf16_to_byte(next) - utf16_to_byte(boundaries[cluster]);
                    size_t units = 1;
                    if (extract_type == 2) {
                        units = 0;
                        for (int32_t point = boundaries[cluster]; point < next;
                             point += U16_LENGTH(text.char32At(point))) {
                            units++;
                        }
                    }
                    size_t addition = extract_type == 0 ? 1
                        : (extract_type == 1 ? size_t(bytes) : units);
                    if (consumed + addition > size_t(size)) break;
                    consumed += addition;
                    cluster++;
                }
                end = boundaries[cluster];
                break;
            }
        }
        Value array = Value::array();
        array.entries = {
            {Value::from_int(0), Value::from_string(to_utf8(text.tempSubStringBetween(start, end)))},
            {Value::from_int(1), Value::from_int(utf16_to_byte(end))},
        };
        return array;
    }
    if (operation == "grapheme_levenshtein") {
        std::string second;
        if (!argument_string(args, 1, second)) return Value::boolean(false);
        std::string locale;
        optional_string(args, 5, locale);
        UErrorCode locale_status = U_ZERO_ERROR;
        UCollator *collator = ucol_open(locale.empty() ? "" : locale.c_str(), &locale_status);
        if (U_FAILURE(locale_status) || collator == nullptr) {
            set_global_error(locale_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR : locale_status,
                "grapheme_levenshtein(): Error on ucol_open");
            return Value::boolean(false);
        }
        std::vector<UnicodeString> left_clusters;
        std::vector<UnicodeString> right_clusters;
        for (size_t index = 0; index + 1 < boundaries.size(); index++)
            left_clusters.push_back(text.tempSubStringBetween(boundaries[index], boundaries[index + 1]));
        UErrorCode second_status = U_ZERO_ERROR;
        std::vector<int32_t> second_boundaries = grapheme_boundaries(second, second_status);
        UnicodeString right = from_utf8(second);
        for (size_t index = 0; index + 1 < second_boundaries.size(); index++)
            right_clusters.push_back(right.tempSubStringBetween(second_boundaries[index],
                                                                 second_boundaries[index + 1]));
        int64_t insertion = 1, replacement = 1, deletion = 1;
        optional_int(args, 2, 1, insertion);
        optional_int(args, 3, 1, replacement);
        optional_int(args, 4, 1, deletion);
        std::vector<int64_t> previous(right_clusters.size() + 1);
        std::vector<int64_t> current(right_clusters.size() + 1);
        for (size_t j = 0; j <= right_clusters.size(); j++) previous[j] = int64_t(j) * insertion;
        for (size_t i = 1; i <= left_clusters.size(); i++) {
            current[0] = int64_t(i) * deletion;
            for (size_t j = 1; j <= right_clusters.size(); j++) {
                int64_t cost = ucol_strcoll(collator, left_clusters[i - 1].getBuffer(),
                                             left_clusters[i - 1].length(),
                                             right_clusters[j - 1].getBuffer(),
                                             right_clusters[j - 1].length()) == UCOL_EQUAL
                    ? 0 : replacement;
                current[j] = std::min({previous[j] + deletion, current[j - 1] + insertion,
                                       previous[j - 1] + cost});
            }
            previous.swap(current);
        }
        int64_t distance = previous[right_clusters.size()];
        ucol_close(collator);
        return Value::from_int(distance);
    }
    return Value::boolean(false);
}

/* --------------------------------------------------------------------------------------- */
/* Locale                                                                                   */
/* --------------------------------------------------------------------------------------- */

thread_local std::string intl_default_locale;
thread_local std::string intl_startup_locale;
thread_local bool intl_locale_initialized = false;

std::string effective_locale() {
    if (!intl_default_locale.empty()) return intl_default_locale;
    if (!intl_startup_locale.empty()) return intl_startup_locale;
    return Locale::getDefault().getName();
}

/// Returns whether one locale is a grandfathered or redundant tag.
///
/// The list mirrors php-src's IANA snapshot; for these tags `getPrimaryLanguage()` returns the
/// tag itself and script/region have no value.
bool is_grandfathered_locale(const std::string &locale) {
    static const char *const tags[] = {
        "art-lojban", "cel-gaulish", "en-GB-oed", "i-ami", "i-bnn", "i-default",
        "i-enochian", "i-hak", "i-klingon", "i-lux", "i-mingo", "i-navajo", "i-pwn",
        "i-tao", "i-tay", "i-tsu", "no-bok", "no-nyn", "sgn-BE-FR", "sgn-BE-NL",
        "sgn-BR", "sgn-CH-DE", "sgn-CO", "sgn-DE", "sgn-DK", "sgn-ES", "sgn-FR",
        "sgn-GB", "sgn-GR", "sgn-IE", "sgn-IT", "sgn-JP", "sgn-MX", "sgn-NI",
        "sgn-NL", "sgn-NO", "sgn-PT", "sgn-SE", "sgn-US", "sgn-ZA", "zh-cmn",
        "zh-cmn-Hans", "zh-cmn-Hant", "zh-gan", "zh-guoyu", "zh-hakka", "zh-min",
        "zh-min-nan", "zh-wuu", "zh-xiang",
    };
    for (const char *tag : tags) {
        if (strcasecmp(locale.c_str(), tag) == 0) return true;
    }
    return false;
}

/// Normalizes one locale tag for RFC 4647 matching: lowercase and `-` -> `_`.
std::string normalize_locale_tag(const std::string &tag) {
    std::string out;
    out.reserve(tag.size());
    for (char character : tag) {
        out.push_back(character == '-' ? '_' : char(std::tolower((unsigned char)character)));
    }
    return out;
}

/// Canonicalizes one locale tag through ICU, returning the input when ICU rejects it.
std::string canonicalize_locale_tag(const std::string &tag) {
    char result[ULOC_FULLNAME_CAPACITY];
    UErrorCode status = U_ZERO_ERROR;
    int32_t length = uloc_canonicalize(tag.c_str(), result, sizeof(result), &status);
    if (U_FAILURE(status)) return tag;
    return std::string(result, size_t(length));
}

/// Canonicalizes a formatter locale the way php's NumberFormatter/IntlDateFormatter do:
/// drop a POSIX charset and a non-keyword modifier, and normalize `-` to `_`.
std::string canonicalize_formatter_locale(std::string locale) {
    size_t dot = locale.find('.');
    if (dot != std::string::npos) locale = locale.substr(0, dot);
    size_t at = locale.find('@');
    if (at != std::string::npos && locale.find('=', at) == std::string::npos) {
        locale = locale.substr(0, at);
    }
    for (char &character : locale) {
        if (character == '-') character = '_';
    }
    return locale;
}

/// Selects php's message-constructor failure text for the locale/pattern distinction.
void set_message_formatter_creation_error(UErrorCode status, const std::string &locale,
                                          const UParseError &parse_error, UErrorCode zero_code);

/// Returns whether php-src would accept one locale argument (a nonempty ISO3 language).
bool php_locale_valid(const std::string &locale) {
    if (locale.empty()) return true;
    UErrorCode status = U_ZERO_ERROR;
    char language[ULOC_LANG_CAPACITY];
    int32_t length = uloc_getLanguage(locale.c_str(), language, sizeof(language), &status);
    if (U_FAILURE(status) || length <= 0) return false;
    return uloc_getISO3Language(locale.c_str())[0] != '\0';
}

/// Selects php's message-constructor failure text for the locale/pattern distinction.
void set_message_formatter_creation_error(UErrorCode status, const std::string &locale,
                                          const UParseError &parse_error, UErrorCode zero_code) {
    if (!php_locale_valid(locale)) {
        set_global_error(zero_code, "Creating message formatter failed");
    } else {
        set_global_error(status, "pattern syntax error (" + parse_error_to_string(parse_error) + ")");
    }
}

/// Collects one compose value that may be a string or an array of strings.
bool compose_values(const std::vector<Value> &args, const char *key,
                    std::vector<std::string> &out) {
    const Value *entries = nullptr;
    if (!argument_array(args, 0, entries)) return false;
    for (const auto &entry : entries->entries) {
        if (entry.first.type != Value::Type::Str || entry.first.text != key) continue;
        if (entry.second.type == Value::Type::Str) {
            out.push_back(entry.second.text);
            return true;
        }
        if (entry.second.type == Value::Type::Array) {
            for (const auto &item : entry.second.entries) {
                if (item.second.type != Value::Type::Str) return false;
                out.push_back(item.second.text);
            }
            return true;
        }
        return false;
    }
    // `variant0`, `variant1`, ... and the other numbered compose forms.
    for (int32_t index = 0; index < 64; index++) {
        std::string numbered = std::string(key) + std::to_string(index);
        for (const auto &entry : entries->entries) {
            if (entry.first.type == Value::Type::Str && entry.first.text == numbered) {
                if (entry.second.type != Value::Type::Str) return false;
                out.push_back(entry.second.text);
                break;
            }
        }
    }
    return true;
}

/// Splits a variant/private string at `_`/`-`, stopping before a singleton subtag.
std::vector<std::string> split_variant_subtags(const std::string &value) {
    std::vector<std::string> out;
    size_t start = 0;
    while (start <= value.size()) {
        size_t end = value.find_first_of("_-", start);
        std::string token = value.substr(start, end == std::string::npos
            ? std::string::npos : end - start);
        if (!token.empty()) {
            if (out.empty() || token.size() > 1) out.push_back(token);
            else break;
        }
        if (end == std::string::npos) break;
        start = end + 1;
    }
    return out;
}

/// Truncates a locale before its first extension singleton (other than `x`).
std::string strip_locale_extensions(const std::string &locale) {
    size_t start = 0;
    size_t subtag = 0;
    while (start <= locale.size()) {
        size_t end = locale.find_first_of("_-", start);
        std::string token = locale.substr(start, end == std::string::npos
            ? std::string::npos : end - start);
        if (subtag > 0 && token.size() == 1 && token[0] != 'x' && token[0] != 'X') {
            return locale.substr(0, start > 0 ? start - 1 : 0);
        }
        subtag++;
        if (end == std::string::npos) break;
        start = end + 1;
    }
    return locale;
}

/// Parses one locale into the `language`/`script`/`region`/variant keyed array.
Value parse_locale_value(const std::string &locale) {
    Value result = Value::array();
    if (locale.size() > 1 && (locale[1] == '-' || locale[1] == '_') &&
        locale[0] != 'x' && locale[0] != 'X' &&
        locale[0] != 'i' && locale[0] != 'I') {
        return result;
    }
    if (locale.size() > 2 && (locale[0] == 'x' || locale[0] == 'X') &&
        (locale[1] == '-' || locale[1] == '_')) {
        std::vector<std::string> subtags = split_variant_subtags(locale.substr(2));
        for (size_t index = 0; index < subtags.size(); index++) {
            result.entries.emplace_back(Value::from_string("private" + std::to_string(index)),
                                        Value::from_string(subtags[index]));
        }
        return result;
    }
    if (is_grandfathered_locale(locale)) {
        result.entries.emplace_back(Value::from_string("grandfathered"),
                                    Value::from_string(locale));
        return result;
    }
    std::string base = strip_locale_extensions(locale);
    char buffer[ULOC_FULLNAME_CAPACITY];
    UErrorCode status = U_ZERO_ERROR;
    int32_t length = uloc_getLanguage(base.c_str(), buffer, sizeof(buffer), &status);
    if (U_SUCCESS(status) && length > 0) {
        result.entries.emplace_back(Value::from_string("language"),
                                    Value::from_string(std::string(buffer, size_t(length))));
    }
    status = U_ZERO_ERROR;
    length = uloc_getScript(base.c_str(), buffer, sizeof(buffer), &status);
    if (U_SUCCESS(status) && length > 0) {
        result.entries.emplace_back(Value::from_string("script"),
                                    Value::from_string(std::string(buffer, size_t(length))));
    }
    status = U_ZERO_ERROR;
    length = uloc_getCountry(base.c_str(), buffer, sizeof(buffer), &status);
    if (U_SUCCESS(status) && length > 0) {
        result.entries.emplace_back(Value::from_string("region"),
                                    Value::from_string(std::string(buffer, size_t(length))));
    }
    status = U_ZERO_ERROR;
    length = uloc_getVariant(base.c_str(), buffer, sizeof(buffer), &status);
    if (U_SUCCESS(status) && length > 0) {
        std::vector<std::string> variants = split_variant_subtags(std::string(buffer, size_t(length)));
        for (size_t index = 0; index < variants.size(); index++) {
            result.entries.emplace_back(Value::from_string("variant" + std::to_string(index)),
                                        Value::from_string(variants[index]));
        }
    }
    size_t private_position = locale.find("-x-");
    if (private_position == std::string::npos) private_position = locale.find("_x_");
    if (private_position != std::string::npos) {
        std::vector<std::string> subtags = split_variant_subtags(locale.substr(private_position + 3));
        for (size_t index = 0; index < subtags.size(); index++) {
            result.entries.emplace_back(Value::from_string("private" + std::to_string(index)),
                                        Value::from_string(subtags[index]));
        }
    }
    return result;
}

/// Implements PHP's Locale::lookup() best-match loop.
Value lookup_locale_value(const std::vector<Value> &args) {
    const Value *available = nullptr;
    if (!argument_array(args, 0, available)) return Value::boolean(false);
    std::string range;
    if (!argument_string(args, 1, range)) return Value::boolean(false);
    bool canonicalize = false;
    optional_bool(args, 2, false, canonicalize);
    std::string fallback;
    bool has_fallback = optional_string(args, 3, fallback);
    if (range.empty()) range = has_fallback ? fallback : effective_locale();
    if (available->entries.empty()) return Value::from_string("");
    std::vector<std::pair<std::string, std::string>> tags;
    for (const auto &entry : available->entries) {
        if (entry.second.type != Value::Type::Str) return Value::null();
        std::string original = entry.second.text;
        if (original.empty()) {
            set_global_error(U_ILLEGAL_ARGUMENT_ERROR, "unable to canonicalize lang_tag");
            return Value::null();
        }
        std::string normalized = canonicalize ? canonicalize_locale_tag(original) : original;
        if (normalized.empty()) {
            set_global_error(U_ILLEGAL_ARGUMENT_ERROR, "unable to canonicalize lang_tag");
            return Value::null();
        }
        tags.emplace_back(normalize_locale_tag(normalized), original);
    }
    std::string normalized_range = canonicalize ? canonicalize_locale_tag(range) : range;
    normalized_range = normalize_locale_tag(normalized_range);
    size_t saved = normalized_range.size();
    while (saved > 0) {
        for (const auto &tag : tags) {
            if (tag.first.size() == saved &&
                normalized_range.compare(0, saved, tag.first) == 0) {
                return Value::from_string(canonicalize ? tag.first : tag.second);
            }
        }
        size_t position = saved;
        bool found = false;
        while (position > 0) {
            char character = normalized_range[position - 1];
            if (character == '_' || character == '@') {
                if (position >= 3 && (normalized_range[position - 3] == '_' ||
                                      normalized_range[position - 3] == '@')) {
                    saved = position - 3;
                } else {
                    saved = position - 1;
                }
                found = true;
                break;
            }
            position--;
        }
        if (!found || saved < 1) break;
    }
    return has_fallback ? Value::from_string(fallback) : Value::from_string("");
}

/// Returns the IANA preferred tag for one grandfathered locale, or the tag itself.
std::string preferred_grandfathered_locale(const std::string &locale) {
    static const struct { const char *tag; const char *preferred; } tags[] = {
        {"art-lojban", "jbo"}, {"cel-gaulish", "cel-gaulish"}, {"en-GB-oed", "en-GB-oxendict"},
        {"i-ami", "ami"}, {"i-bnn", "bnn"}, {"i-default", "i-default"},
        {"i-enochian", "i-enochian"}, {"i-hak", "hak"}, {"i-klingon", "tlh"},
        {"i-lux", "lb"}, {"i-mingo", "i-mingo"}, {"i-navajo", "nv"}, {"i-pwn", "pwn"},
        {"i-tao", "tao"}, {"i-tay", "tay"}, {"i-tsu", "tsu"}, {"no-bok", "nb"},
        {"no-nyn", "nn"}, {"sgn-BE-FR", "sfb"}, {"sgn-BE-NL", "vgt"}, {"sgn-BR", "bzs"},
        {"sgn-CH-DE", "sgg"}, {"sgn-CO", "csn"}, {"sgn-DE", "gsg"}, {"sgn-DK", "dsl"},
        {"sgn-ES", "ssp"}, {"sgn-FR", "fsl"}, {"sgn-GB", "bfi"}, {"sgn-GR", "gss"},
        {"sgn-IE", "isg"}, {"sgn-IT", "ise"}, {"sgn-JP", "jsl"}, {"sgn-MX", "mfs"},
        {"sgn-NI", "ncs"}, {"sgn-NL", "dse"}, {"sgn-NO", "nsl"}, {"sgn-PT", "psr"},
        {"sgn-SE", "swl"}, {"sgn-US", "ase"}, {"sgn-ZA", "sfs"}, {"zh-cmn", "cmn"},
        {"zh-cmn-Hans", "cmn-Hans"}, {"zh-cmn-Hant", "cmn-Hant"}, {"zh-gan", "gan"},
        {"zh-guoyu", "cmn"}, {"zh-hakka", "hak"}, {"zh-min", "zh-min"},
        {"zh-min-nan", "nan"}, {"zh-wuu", "wuu"}, {"zh-xiang", "hsn"},
    };
    for (const auto &entry : tags) {
        if (locale == entry.tag) return entry.preferred;
    }
    return locale;
}

Value locale_operation(const std::string &operation, const std::vector<Value> &args) {
    UErrorCode status = U_ZERO_ERROR;
    set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
    if (operation == "locale_get_default") {
        return Value::from_string(effective_locale());
    }
    if (operation == "locale_set_default") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::boolean(false);
        if (!intl_locale_initialized) {
            intl_startup_locale = locale;
            intl_locale_initialized = true;
        }
        intl_default_locale = locale;
        // ICU-owned objects (calendars, formatters, collators) read `Locale::getDefault()`
        // when PHP does not pass an explicit locale, so mirror the shim's default there.
        if (!locale.empty()) {
            UErrorCode default_status = U_ZERO_ERROR;
            Locale::setDefault(Locale(locale.c_str()), default_status);
        }
        return Value::boolean(true);
    }
    if (operation == "locale_get_primary_language" || operation == "locale_get_script" ||
        operation == "locale_get_region") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::null();
        if (is_grandfathered_locale(locale)) {
            if (operation == "locale_get_primary_language") return Value::from_string(locale);
            return Value::null();
        }
        char buffer[ULOC_FULLNAME_CAPACITY];
        UErrorCode local_status = U_ZERO_ERROR;
        int32_t length = 0;
        if (operation == "locale_get_primary_language") {
            length = uloc_getLanguage(locale.c_str(), buffer, sizeof(buffer), &local_status);
        } else if (operation == "locale_get_script") {
            length = uloc_getScript(locale.c_str(), buffer, sizeof(buffer), &local_status);
        } else {
            length = uloc_getCountry(locale.c_str(), buffer, sizeof(buffer), &local_status);
        }
        if (U_FAILURE(local_status)) return Value::null();
        return Value::from_string(std::string(buffer, size_t(length)));
    }
    if (operation == "locale_parse") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::null();
        if (locale.empty()) locale = effective_locale();
        return parse_locale_value(locale);
    }
    if (operation == "locale_get_all_variants") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::null();
        if (locale.empty()) locale = effective_locale();
        Value result = Value::array();
        if (!is_grandfathered_locale(locale)) {
            char buffer[ULOC_FULLNAME_CAPACITY];
            UErrorCode status = U_ZERO_ERROR;
            std::string base = strip_locale_extensions(locale);
            int32_t length = uloc_getVariant(base.c_str(), buffer, sizeof(buffer), &status);
            if (U_SUCCESS(status) && length > 0) {
                int32_t index = 0;
                for (const std::string &variant :
                     split_variant_subtags(std::string(buffer, size_t(length)))) {
                    result.entries.emplace_back(Value::from_int(index++),
                                                Value::from_string(variant));
                }
            }
        }
        return result;
    }
    if (operation == "locale_lookup") {
        return lookup_locale_value(args);
    }
    if (operation == "locale_filter_matches") {
        std::string lang_tag, range;
        if (!argument_string(args, 0, lang_tag) || !argument_string(args, 1, range))
            return Value::null();
        bool canonicalize = false;
        optional_bool(args, 2, false, canonicalize);
        if (range == "*") return Value::boolean(true);
        if (range.empty()) range = effective_locale();
        std::string left = canonicalize ? canonicalize_locale_tag(lang_tag) : lang_tag;
        std::string right = canonicalize ? canonicalize_locale_tag(range) : range;
        left = normalize_locale_tag(left);
        right = normalize_locale_tag(right);
        if (left == right) return Value::boolean(true);
        if (left.size() > right.size() && left.compare(0, right.size(), right) == 0) {
            char next = left[right.size()];
            return Value::boolean(next == '_' || next == '@');
        }
        return Value::boolean(false);
    }
    if (operation == "locale_compose") {
        const Value *entries = nullptr;
        if (!argument_array(args, 0, entries)) return Value::boolean(false);
        for (const auto &entry : entries->entries) {
            bool plural = entry.first.type == Value::Type::Str &&
                (entry.first.text == "variant" || entry.first.text == "private" ||
                 entry.first.text == "extlang");
            if (entry.second.type != Value::Type::Str &&
                !(plural && entry.second.type == Value::Type::Array)) {
                set_global_error(U_ILLEGAL_ARGUMENT_ERROR,
                    "parameter array element is not a string");
                return Value::boolean(false);
            }
        }
        std::string grandfathered;
        for (const auto &entry : entries->entries) {
            if (entry.first.type == Value::Type::Str && entry.first.text == "grandfathered" &&
                entry.second.type == Value::Type::Str) {
                return Value::from_string(entry.second.text);
            }
        }
        std::vector<std::string> languages;
        if (!compose_values(args, "language", languages) || languages.size() != 1)
            return Value::boolean(false);
        std::string composed = languages[0];
        for (const char *key : {"extlang", "script", "region"}) {
            std::vector<std::string> values;
            if (!compose_values(args, key, values)) return Value::boolean(false);
            for (const auto &value : values) composed += "_" + value;
        }
        std::vector<std::string> variants;
        if (!compose_values(args, "variant", variants)) return Value::boolean(false);
        for (const auto &value : variants) composed += "_" + value;
        std::vector<std::string> privates;
        if (!compose_values(args, "private", privates)) return Value::boolean(false);
        if (!privates.empty()) {
            composed += "_x";
            for (const auto &value : privates) composed += "_" + value;
        }
        std::string keywords;
        for (const auto &entry : entries->entries) {
            if (entry.first.type != Value::Type::Str)
                return Value::boolean(false);
            const std::string &key = entry.first.text;
            if (key == "language" || key == "extlang" || key == "script" || key == "region" ||
                key == "variant" || key == "private" || key == "grandfathered") continue;
            if (key.find("variant") == 0 || key.find("private") == 0 ||
                key.find("extlang") == 0) continue;
            if (entry.second.type != Value::Type::Str) return Value::boolean(false);
            if (!keywords.empty()) keywords += ";";
            keywords += key + "=" + entry.second.text;
        }
        if (!keywords.empty()) composed += "@" + keywords;
        return Value::from_string(composed);
    }
    if (operation == "locale_get_keywords") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::null();
        UEnumeration *keywords = uloc_openKeywords(locale.c_str(), &status);
        if (U_FAILURE(status)) return Value::boolean(false);
        Value result = Value::array();
        const char *keyword = nullptr;
        int32_t length = 0;
        while ((keyword = uenum_next(keywords, &length, &status)) != nullptr) {
            char value[128];
            UErrorCode value_status = U_ZERO_ERROR;
            int32_t value_length = uloc_getKeywordValue(locale.c_str(), keyword, value,
                                                        sizeof(value), &value_status);
            if (U_SUCCESS(value_status)) {
                result.entries.emplace_back(
                    Value::from_string(std::string(keyword, size_t(length))),
                    Value::from_string(std::string(value, size_t(value_length))));
            }
        }
        uenum_close(keywords);
        return result;
    }
    if (operation == "locale_get_display_name" || operation == "locale_get_display_language" ||
        operation == "locale_get_display_script" || operation == "locale_get_display_region" ||
        operation == "locale_get_display_variant") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::boolean(false);
        if (locale.size() > ULOC_FULLNAME_CAPACITY) {
            set_global_error(U_ILLEGAL_ARGUMENT_ERROR, "name too long");
            return Value::boolean(false);
        }
        if (operation != "locale_get_display_name" && is_grandfathered_locale(locale)) {
            if (operation != "locale_get_display_language") return Value::boolean(false);
            locale = preferred_grandfathered_locale(locale);
        }
        std::string display;
        optional_string(args, 1, display);
        if (display.empty()) display = effective_locale();
        UChar buffer[1024];
        UErrorCode display_status = U_ZERO_ERROR;
        int32_t length = 0;
        if (operation == "locale_get_display_name") {
            length = uloc_getDisplayName(locale.c_str(), display.c_str(), buffer, 1024,
                                         &display_status);
        } else if (operation == "locale_get_display_language") {
            length = uloc_getDisplayLanguage(locale.c_str(), display.c_str(), buffer, 1024,
                                             &display_status);
        } else if (operation == "locale_get_display_script") {
            length = uloc_getDisplayScript(locale.c_str(), display.c_str(), buffer, 1024,
                                           &display_status);
        } else if (operation == "locale_get_display_region") {
            length = uloc_getDisplayCountry(locale.c_str(), display.c_str(), buffer, 1024,
                                            &display_status);
        } else {
            length = uloc_getDisplayVariant(locale.c_str(), display.c_str(), buffer, 1024,
                                            &display_status);
        }
        if (U_FAILURE(display_status)) return Value::boolean(false);
        return Value::from_string(from_utf16(buffer, length, display_status));
    }
    if (operation == "locale_canonicalize") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::null();
        char result[ULOC_FULLNAME_CAPACITY];
        UErrorCode canonical_status = U_ZERO_ERROR;
        int32_t length = uloc_canonicalize(locale.c_str(), result, sizeof(result),
                                            &canonical_status);
        if (U_FAILURE(canonical_status)) return Value::null();
        return Value::from_string(std::string(result, size_t(length)));
    }
    if (operation == "locale_is_right_to_left") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::boolean(false);
        UErrorCode orientation_status = U_ZERO_ERROR;
        ULayoutType orientation = uloc_getCharacterOrientation(locale.c_str(),
                                                                &orientation_status);
        return Value::boolean(U_SUCCESS(orientation_status) && orientation == U_RIGHT_TO_LEFT);
    }
    if (operation == "locale_accept_from_http") {
        std::string header;
        if (!argument_string(args, 0, header)) return Value::boolean(false);
        // php rejects any comma-separated fragment longer than ULOC_FULLNAME_CAPACITY.
        size_t start = 0;
        while (start <= header.size()) {
            size_t end = header.find(',', start);
            size_t length = end == std::string::npos ? header.size() - start : end - start;
            if (length > ULOC_FULLNAME_CAPACITY) {
                set_global_error(U_ILLEGAL_ARGUMENT_ERROR, "locale string too long");
                return Value::boolean(false);
            }
            if (end == std::string::npos) break;
            start = end + 1;
        }
        char result[ULOC_FULLNAME_CAPACITY];
        UErrorCode accept_status = U_ZERO_ERROR;
        UAcceptResult accepted = ULOC_ACCEPT_FAILED;
        UEnumeration *available = ures_openAvailableLocales(nullptr, &accept_status);
        if (U_FAILURE(accept_status) || available == nullptr) return Value::boolean(false);
        int32_t length = uloc_acceptLanguageFromHTTP(result, sizeof(result), &accepted,
                                                     header.c_str(), available, &accept_status);
        uenum_close(available);
        if (U_FAILURE(accept_status) || accepted == ULOC_ACCEPT_FAILED)
            return Value::boolean(false);
        return Value::from_string(std::string(result, size_t(length)));
    }
    if (operation == "locale_lookup") {
        const Value *tags = nullptr;
        std::string locale;
        if (!argument_array(args, 0, tags) || !argument_string(args, 1, locale))
            return Value::null();
        // PHP's lookup is an RFC 4647 best-fit over the supplied tags. Iterate the tags in
        // order and return the first that matches the requested locale's language.
        Locale requested(locale.c_str());
        for (const auto &entry : tags->entries) {
            if (entry.second.type != Value::Type::Str) continue;
            Locale candidate(entry.second.text.c_str());
            if (std::strcmp(candidate.getLanguage(), requested.getLanguage()) == 0) {
                return Value::from_string(entry.second.text);
            }
        }
        bool has_default = false;
        std::string default_locale;
        has_default = optional_string(args, 3, default_locale);
        return has_default ? Value::from_string(default_locale) : Value::null();
    }
    if (operation == "locale_add_likely_subtags" || operation == "locale_minimize_subtags") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::boolean(false);
        char result[ULOC_FULLNAME_CAPACITY];
        UErrorCode subtag_status = U_ZERO_ERROR;
        int32_t length = operation == "locale_add_likely_subtags"
            ? uloc_addLikelySubtags(locale.c_str(), result, sizeof(result), &subtag_status)
            : uloc_minimizeSubtags(locale.c_str(), result, sizeof(result), &subtag_status);
        if (U_FAILURE(subtag_status) || length <= 0) {
            set_global_error(U_ILLEGAL_ARGUMENT_ERROR, "invalid locale");
            return Value::boolean(false);
        }
        return Value::from_string(std::string(result, size_t(length)));
    }
    if (operation == "locale_parse" || operation == "locale_get_all_variants") {
        std::string locale;
        if (!argument_string(args, 0, locale)) return Value::null();
        Locale parsed(locale.c_str());
        if (operation == "locale_parse") {
            Value result = Value::array();
            if (parsed.getLanguage()[0] != '\0')
                result.entries.emplace_back(Value::from_string("language"),
                                            Value::from_string(parsed.getLanguage()));
            if (parsed.getScript()[0] != '\0')
                result.entries.emplace_back(Value::from_string("script"),
                                            Value::from_string(parsed.getScript()));
            if (parsed.getCountry()[0] != '\0')
                result.entries.emplace_back(Value::from_string("region"),
                                            Value::from_string(parsed.getCountry()));
            if (parsed.getVariant()[0] != '\0')
                result.entries.emplace_back(Value::from_string("variant"),
                                            Value::from_string(parsed.getVariant()));
            UEnumeration *keywords = uloc_openKeywords(locale.c_str(), &status);
            if (keywords != nullptr) {
                const char *keyword = nullptr;
                int32_t length = 0;
                while ((keyword = uenum_next(keywords, &length, &status)) != nullptr) {
                    char value[128];
                    UErrorCode keyword_status = U_ZERO_ERROR;
                    int32_t value_length = uloc_getKeywordValue(locale.c_str(), keyword, value,
                                                                sizeof(value), &keyword_status);
                    if (U_SUCCESS(keyword_status)) {
                        result.entries.emplace_back(
                            Value::from_string(std::string(keyword, size_t(length))),
                            Value::from_string(std::string(value, size_t(value_length))));
                    }
                }
                uenum_close(keywords);
            }
            return result;
        }
        Value result = Value::array();
        if (parsed.getVariant()[0] != '\0') {
            result.entries.emplace_back(Value::from_int(0),
                                        Value::from_string(parsed.getVariant()));
        }
        return result;
    }
    if (operation == "locale_compose") {
        const Value *subtags = nullptr;
        if (!argument_array(args, 0, subtags)) return Value::boolean(false);
        std::string language, script, region, variant;
        for (const auto &entry : subtags->entries) {
            if (entry.first.type != Value::Type::Str || entry.second.type != Value::Type::Str)
                continue;
            const std::string &key = entry.first.text;
            if (key == "language") language = entry.second.text;
            else if (key == "script") script = entry.second.text;
            else if (key == "region") region = entry.second.text;
            else if (key == "variant") variant = entry.second.text;
        }
        std::string composed = language;
        if (!script.empty()) composed += "_" + script;
        if (!region.empty()) composed += "_" + region;
        if (!variant.empty()) composed += "_" + variant;
        return Value::from_string(composed);
    }
    if (operation == "locale_filter_matches") {
        std::string tag, locale;
        if (!argument_string(args, 0, tag) || !argument_string(args, 1, locale))
            return Value::null();
        Locale requested(locale.c_str());
        Locale candidate(tag.c_str());
        return Value::boolean(std::strcmp(requested.getLanguage(), candidate.getLanguage()) == 0);
    }
    return Value::boolean(false);
}

/* --------------------------------------------------------------------------------------- */
/* IDN                                                                                      */
/* --------------------------------------------------------------------------------------- */

Value idn_operation(const std::string &operation, const std::vector<Value> &args) {
    std::string domain;
    if (!argument_string(args, 0, domain)) return Value::boolean(false);
    int64_t flags = 0, variant = 0;
    optional_int(args, 1, 0, flags);
    optional_int(args, 2, 0, variant);
    (void)variant;
    UErrorCode status = U_ZERO_ERROR;
    UIDNA *idna = uidna_openUTS46(uint32_t(flags), &status);
    if (U_FAILURE(status)) return Value::boolean(false);
    UIDNAInfo info = UIDNA_INFO_INITIALIZER;
    UnicodeString input = from_utf8(domain);
    UnicodeString output;
    int32_t length = operation == "idn_to_ascii"
        ? uidna_nameToASCII_UTF8(idna, domain.data(), int32_t(domain.size()), nullptr, 0,
                                 &info, &status)
        : uidna_nameToUnicodeUTF8(idna, domain.data(), int32_t(domain.size()), nullptr, 0,
                                  &info, &status);
    if (status == U_BUFFER_OVERFLOW_ERROR) status = U_ZERO_ERROR;
    std::string result(size_t(length), '\0');
    if (length > 0) {
        info = UIDNA_INFO_INITIALIZER;
        if (operation == "idn_to_ascii") {
            uidna_nameToASCII_UTF8(idna, domain.data(), int32_t(domain.size()), result.data(),
                                   length, &info, &status);
        } else {
            uidna_nameToUnicodeUTF8(idna, domain.data(), int32_t(domain.size()), result.data(),
                                    length, &info, &status);
        }
    }
    uidna_close(idna);
    if (U_FAILURE(status)) return Value::boolean(false);
    Value details = Value::array();
    details.entries = {
        {Value::from_string("result"), Value::from_string(result)},
        {Value::from_string("isTransitionalDifferent"), Value::boolean(info.isTransitionalDifferent)},
        {Value::from_string("errors"), Value::from_int(info.errors)},
    };
    // The synthetic PHP wrapper reads element 0 as the return value and element 1 as the
    // by-reference `idna_info` array.
    Value array = Value::array();
    array.entries = {
        {Value::from_int(0), Value::from_string(result)},
        {Value::from_int(1), std::move(details)},
    };
    return array;
}

/// Decodes one PHP codepoint argument: an integer, or the first Unicode scalar of a string.
bool argument_codepoint(const std::vector<Value> &args, size_t index, UChar32 &out) {
    if (index >= args.size()) return false;
    if (args[index].type == Value::Type::Int) {
        out = UChar32(args[index].integer);
        return true;
    }
    if (args[index].type == Value::Type::Str) {
        int32_t offset = 0;
        int32_t length = int32_t(args[index].text.size());
        U8_NEXT(args[index].text.data(), offset, length, out);
        return out >= 0;
    }
    return false;
}

/// Serves the `IntlChar` static surface php-src implements with `u_char*`/`u_*digit`.
Value char_operation(const std::string &operation, const std::vector<Value> &args) {
    if (operation == "char_from_name") {
        std::string name;
        if (!argument_string(args, 0, name)) return Value::null();
        int64_t choice = U_UNICODE_CHAR_NAME;
        optional_int(args, 1, U_UNICODE_CHAR_NAME, choice);
        UErrorCode status = U_ZERO_ERROR;
        UChar32 codepoint = u_charFromName(UCharNameChoice(choice), name.c_str(), &status);
        // ICU reports an unknown name as U+FFFF with U_ILLEGAL_CHAR_FOUND, not a negative
        // codepoint; php-src surfaces that as null.
        if (U_FAILURE(status) || codepoint < 0 || codepoint == 0xFFFF) {
            if (U_SUCCESS(status)) status = U_ILLEGAL_ARGUMENT_ERROR;
            set_global_error(status, "");
            return Value::null();
        }
        set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
        return Value::from_int(codepoint);
    }
    if (operation == "char_name") {
        UChar32 codepoint = 0;
        if (!argument_codepoint(args, 0, codepoint)) return Value::null();
        int64_t choice = U_UNICODE_CHAR_NAME;
        optional_int(args, 1, U_UNICODE_CHAR_NAME, choice);
        UErrorCode status = U_ZERO_ERROR;
        int32_t length = u_charName(codepoint, UCharNameChoice(choice), nullptr, 0, &status);
        // The sizing call always answers U_BUFFER_OVERFLOW_ERROR; only a real failure matters.
        if (status == U_BUFFER_OVERFLOW_ERROR) status = U_ZERO_ERROR;
        if (U_FAILURE(status)) {
            set_global_error(status, "Failure getting character name");
            return Value::null();
        }
        std::string name(size_t(length), '\0');
        status = U_ZERO_ERROR;
        u_charName(codepoint, UCharNameChoice(choice), name.data(), length + 1, &status);
        if (U_FAILURE(status)) {
            set_global_error(status, "Failure getting character name");
            return Value::null();
        }
        set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
        return Value::from_string(name);
    }
    if (operation == "digit") {
        UChar32 codepoint = 0;
        if (!argument_codepoint(args, 0, codepoint)) return Value::null();
        int64_t radix = 10;
        optional_int(args, 1, 10, radix);
        int32_t digit = u_digit(codepoint, int8_t(radix));
        if (digit < 0) {
            set_global_error(U_ILLEGAL_ARGUMENT_ERROR, "Invalid digit");
            return Value::boolean(false);
        }
        set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
        return Value::from_int(digit);
    }
    if (operation == "for_digit") {
        int64_t digit = 0, radix = 10;
        if (!argument_int(args, 0, digit)) return Value::boolean(false);
        optional_int(args, 1, 10, radix);
        set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
        return Value::from_int(UChar32(u_forDigit(int32_t(digit), int8_t(radix))));
    }
    return Value::null();
}

/// Reads a calendar handle's current instant in milliseconds, when the value is one.
bool calendar_object_millis(int64_t handle, double &out) {
    IntlObject *object = lookup(handle);
    if (object == nullptr) return false;
    auto *calendar = dynamic_cast<CalendarObject *>(object);
    if (calendar == nullptr || !calendar->calendar) return false;
    UErrorCode status = U_ZERO_ERROR;
    out = double(calendar->calendar->getTime(status));
    return U_SUCCESS(status);
}

/// Opens a resource bundle honouring php's `$fallback` flag.
bool open_resource_bundle(const std::string &locale, const std::string &path, bool fallback,
                          std::unique_ptr<ResourceBundle> &out, UErrorCode &status,
                          std::string &context) {
    std::string actual_locale = locale.empty() ? Locale::getDefault().getName() : locale;
    const char *path_ptr = path.empty() ? nullptr : path.c_str();
    UResourceBundle *raw = fallback
        ? ures_open(path_ptr, actual_locale.c_str(), &status)
        : ures_openDirect(path_ptr, actual_locale.c_str(), &status);
    if (U_FAILURE(status) || raw == nullptr) {
        context = "Cannot load libICU resource bundle";
        return false;
    }
    out = std::make_unique<ResourceBundle>(raw, status);
    if (U_FAILURE(status)) {
        context = "Cannot load libICU resource bundle";
        return false;
    }
    if (!fallback &&
        (status == U_USING_FALLBACK_WARNING || status == U_USING_DEFAULT_WARNING)) {
        UErrorCode locale_status = U_ZERO_ERROR;
        const char *actual = ures_getLocaleByType(raw, ULOC_ACTUAL_LOCALE, &locale_status);
        context = "Cannot load libICU resource '" +
                  (path.empty() ? std::string("(default data)") : path) +
                  "' without fallback from " + actual_locale + " to " +
                  (actual != nullptr ? actual : "");
        return false;
    }
    return true;
}

/* --------------------------------------------------------------------------------------- */
/* Static dispatch                                                                          */
/* --------------------------------------------------------------------------------------- */

Value static_dispatch(const std::string &family, const std::string &operation_in,
                      const std::vector<Value> &args) {
    UErrorCode status = U_ZERO_ERROR;
    std::string operation = operation_in;
    // Class methods use the PHP method spelling; procedural entry points use the
    // function spelling. Accept both at the one bridge boundary.
    if ((family == "timezone" || family == "intltz") && operation.rfind("intltz_", 0) != 0) {
        operation = "intltz_" + operation;
    } else if (family == "locale" && operation.rfind("locale_", 0) != 0) {
        operation = "locale_" + operation;
    }
    if (family == "normalizer") {
        // Procedural names carry the family prefix; class methods use the bare operation.
        if (operation == "normalize" || operation == "normalizer_normalize")
            return normalize_value(args, false, false);
        if (operation == "is_normalized" || operation == "normalizer_is_normalized")
            return normalize_value(args, true, false);
        if (operation == "get_raw_decomposition" || operation == "normalizer_get_raw_decomposition")
            return normalize_value(args, false, true);
    }
    if (family == "grapheme") return grapheme_operation(operation, args);
    if (family == "char") return char_operation(operation, args);
    if (family == "locale") return locale_operation(operation, args);
    if (family == "idn") return idn_operation(operation, args);
    if (family == "intl") {
        if (operation == "locale_valid") {
            std::string locale;
            if (!argument_string(args, 0, locale)) return Value::boolean(false);
            return Value::boolean(php_locale_valid(locale));
        }
        if (operation == "timezone_valid") {
            std::string id;
            if (!argument_string(args, 0, id)) return Value::boolean(false);
            UErrorCode zone_status = U_ZERO_ERROR;
            std::unique_ptr<TimeZone> zone(TimeZone::createTimeZone(from_utf8(id)));
            return Value::boolean(zone != nullptr && *zone != TimeZone::getUnknown());
        }
        if (operation == "intl_get_error_code") return Value::from_int(global_error_code);
        if (operation == "intl_get_error_message") return Value::from_string(global_error_message);
        if (operation == "intl_get_error_context") return Value::from_string(global_error_context);
        if (operation == "intl_reprefix_error") {
            int64_t handle = 0;
            std::string prefix;
            if (!argument_int(args, 0, handle) || !argument_string(args, 1, prefix))
                return Value::null();
            IntlObject *object = lookup(handle);
            if (object == nullptr) return Value::null();
            std::string context = object->error_context;
            size_t marker = context.find("(): ");
            if (marker != std::string::npos && context.find("::") < marker) {
                // Replace the object's `Class::method(): ` prefix with the procedural one.
                context = context.substr(marker + 4);
            }
            object->error_context = prefix + ": " + context;
            object->error_message = object->error_context;
            if (U_FAILURE(UErrorCode(object->error_code))) {
                object->error_message += ": ";
                object->error_message += u_errorName(UErrorCode(object->error_code));
            }
            set_global_error(UErrorCode(object->error_code), object->error_context);
            return Value::null();
        }
        if (operation == "intl_restore_error") {
            int64_t code = 0;
            std::string context;
            if (!argument_int(args, 0, code) || !argument_string(args, 1, context))
                return Value::null();
            set_global_error(UErrorCode(code), context);
            return Value::null();
        }
        if (operation == "intl_set_error") {
            int64_t code = 0;
            std::string context;
            if (!argument_int(args, 0, code) || !argument_string(args, 1, context))
                return Value::null();
            set_global_error(UErrorCode(code), context);
            return Value::null();
        }
        if (operation == "intl_is_failure") {
            int64_t code = 0;
            if (!argument_int(args, 0, code)) return Value::boolean(false);
            return Value::boolean(U_FAILURE(UErrorCode(code)));
        }
        if (operation == "intl_error_name") {
            int64_t code = 0;
            if (!argument_int(args, 0, code)) return Value::boolean(false);
            return Value::from_string(u_errorName(UErrorCode(code)));
        }
    }
    if (family == "version") {
        if (operation == "icu") return Value::from_string(U_ICU_VERSION);
        return Value::from_string(U_ICU_DATA_VERSION);
    }
    if (family == "collator") {
        if (operation == "getDisplayName" || operation == "collator_get_display_name") {
            std::string object_locale, display_locale;
            if (!argument_string(args, 0, object_locale)) return Value::boolean(false);
            optional_string(args, 1, display_locale);
            Locale object(object_locale.c_str());
            Locale display(display_locale.empty() ? Locale::getDefault().getName()
                                                  : display_locale.c_str());
            UnicodeString text;
            object.getDisplayName(display, text);
            return Value::from_string(to_utf8(text));
        }
        if (operation == "getAvailableLocales" || operation == "collator_get_available_locales") {
            int32_t count = 0;
            const Locale *locales = Collator::getAvailableLocales(count);
            Value result = Value::array();
            for (int32_t index = 0; index < count; index++) {
                result.entries.emplace_back(Value::from_int(index),
                                            Value::from_string(locales[index].getName()));
            }
            return result;
        }
        if (operation == "getDefault" || operation == "collator_get_default") {
            UErrorCode local_status = U_ZERO_ERROR;
            Collator *collator = Collator::createInstance(local_status);
            if (U_FAILURE(local_status) || collator == nullptr) return Value::null();
            auto object = std::make_unique<CollatorObject>();
            object->collator.reset(collator);
            return wrap_object(insert(std::move(object)), "Collator");
        }
        if (operation == "setDefault" || operation == "collator_set_default") {
            return Value::boolean(true);
        }
        if (operation == "collator_create") {
            std::string locale;
            if (!argument_string(args, 0, locale)) return Value::null();
            if (locale.size() > 156) {
                set_global_error(U_ILLEGAL_ARGUMENT_ERROR,
                    "Locale string too long, should be no longer than 156 characters");
                return Value::null();
            }
            std::unique_ptr<Collator> collator(Collator::createInstance(Locale(locale.c_str()),
                                                                        status));
            if (U_FAILURE(status) || !collator) {
                set_global_error(U_ILLEGAL_ARGUMENT_ERROR, "unable to open ICU collator");
                return Value::null();
            }
            auto object = std::make_unique<CollatorObject>();
            object->collator = std::move(collator);
            object->locale = locale;
            return wrap_object(insert(std::move(object)), "Collator");
        }
    }
    if (family == "number" || family == "numfmt") {
        if (operation == "numfmt_create") {
            std::string locale;
            if (!argument_string(args, 0, locale)) return Value::null();
            int64_t style = UNUM_DECIMAL;
            optional_int(args, 1, UNUM_DECIMAL, style);
            std::string pattern;
            bool has_pattern = optional_string(args, 2, pattern) && !pattern.empty();
            NumberFormat *created = open_number_format(style, locale, pattern, has_pattern,
                                                       status);
            if (U_FAILURE(status) || created == nullptr) {
                delete created;
                return Value::null();
            }
            if (has_pattern && style != UNUM_PATTERN_DECIMAL && style != UNUM_PATTERN_RULEBASED) {
                auto *decimal = dynamic_cast<DecimalFormat *>(created);
                if (decimal != nullptr) decimal->applyPattern(from_utf8(pattern), status);
            }
            if (U_FAILURE(status)) {
                delete created;
                return Value::null();
            }
            auto object = std::make_unique<NumberFormatterObject>();
            object->formatter.reset(created);
            object->locale = locale;
            object->style = style;
            return wrap_object(insert(std::move(object)), "NumberFormatter");
        }
    }
    if (family == "msgfmt") {
        if (operation == "msgfmt_create") {
            std::string locale, pattern;
            if (!argument_string(args, 0, locale) || !argument_string(args, 1, pattern))
                return Value::null();
            UParseError parse_error = {0};
            auto *formatter = new MessageFormat(from_utf8(pattern), Locale(locale.c_str()),
                                                parse_error, status);
            if (U_FAILURE(status)) {
                delete formatter;
                set_message_formatter_creation_error(status, locale, parse_error, U_ZERO_ERROR);
                return Value::null();
            }
            auto object = std::make_unique<MessageFormatterObject>();
            object->formatter.reset(formatter);
            object->locale = locale;
            object->pattern = pattern;
            return wrap_object(insert(std::move(object)), "MessageFormatter");
        }
        if (operation == "msgfmt_parse_message" || operation == "parse_message") {
            bool procedural = operation == "msgfmt_parse_message";
            std::string locale, pattern, message;
            if (!argument_string(args, 0, locale) || !argument_string(args, 1, pattern) ||
                !argument_string(args, 2, message)) return Value::boolean(false);
            UParseError parse_error = {0};
            MessageFormat formatter(from_utf8(pattern), Locale(locale.c_str()), parse_error, status);
            if (U_FAILURE(status)) {
                set_message_formatter_creation_error(status, locale, parse_error, U_ILLEGAL_ARGUMENT_ERROR);
                return Value::boolean(false);
            }
            ParsePosition position(0);
            int32_t parsed_count = 0;
            std::unique_ptr<Formattable[]> parsed(
                formatter.parse(from_utf8(message), position, parsed_count));
            if (!parsed || position.getErrorIndex() >= 0 || position.getIndex() == 0)
                return Value::boolean(false);
            Value result = Value::array();
            for (int32_t index = 0; index < parsed_count; index++) {
                const Formattable &value = parsed[index];
                Value item;
                if (value.getType() == Formattable::kLong) item = Value::from_int(value.getLong());
                else if (value.getType() == Formattable::kInt64) item = Value::from_int(value.getInt64());
                else if (value.getType() == Formattable::kDate) item = Value::from_double(double(value.getDate()) / 1000.0);
                else if (value.getType() == Formattable::kDouble) item = Value::from_double(value.getDouble());
                else if (value.getType() == Formattable::kString) item = Value::from_string(to_utf8(value.getString()));
                else item = Value::null();
                result.entries.emplace_back(Value::from_int(index), std::move(item));
            }
            return result;
        }
        if (operation == "msgfmt_format_message") {
            std::string locale, pattern;
            if (!argument_string(args, 0, locale) || !argument_string(args, 1, pattern))
                return Value::boolean(false);
            const Value *values = nullptr;
            if (!argument_array(args, 2, values)) return Value::boolean(false);
            std::string timezone;
            optional_string(args, 3, timezone);
            UParseError parse_error = {0};
            MessageFormat formatter(from_utf8(pattern), Locale(locale.c_str()), parse_error, status);
            if (U_FAILURE(status)) {
                set_message_formatter_creation_error(status, locale, parse_error, U_ZERO_ERROR);
                return Value::boolean(false);
            }
            UnicodeString text;
            std::string error;
            if (!format_message_values(formatter, from_utf8(pattern), *values, timezone, text,
                                       error, status)) {
                set_global_error(status, error);
                return Value::boolean(false);
            }
            set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            return Value::from_string(to_utf8(text));
        }
    }
    if (family == "datefmt") {
        if (operation == "datefmt_create") {
            std::string locale, zone, pattern, calendar_locale;
            int64_t date_style = UDAT_FULL, time_style = UDAT_FULL;
            int64_t calendar = UCAL_GREGORIAN;
            optional_string(args, 0, locale);
            optional_int(args, 1, UDAT_FULL, date_style);
            optional_int(args, 2, UDAT_FULL, time_style);
            if (!optional_string(args, 3, zone)) zone = "UTC";
            optional_int(args, 4, UCAL_GREGORIAN, calendar);
            optional_string(args, 5, pattern);
            if (!(date_style == UDAT_PATTERN || date_style == UDAT_NONE ||
                  (date_style >= UDAT_FULL && date_style <= UDAT_SHORT))) {
                set_global_error(U_ILLEGAL_ARGUMENT_ERROR,
                    "datefmt_create(): invalid date format style");
                return Value::null();
            }
            if (!(time_style == UDAT_PATTERN || time_style == UDAT_NONE ||
                  (time_style >= UDAT_FULL && time_style <= UDAT_SHORT))) {
                set_global_error(U_ILLEGAL_ARGUMENT_ERROR,
                    "datefmt_create(): invalid time format style");
                return Value::null();
            }
            UErrorCode create_status = U_ZERO_ERROR;
            auto object = std::make_unique<DateFormatterObject>();
            bool style_pattern = time_style == UDAT_PATTERN || date_style == UDAT_PATTERN;
            if (!style_pattern) {
                DateFormat *formatter = DateFormat::createDateTimeInstance(
                    DateFormat::EStyle(date_style), DateFormat::EStyle(time_style),
                    locale.empty() ? Locale::getDefault() : Locale(locale.c_str()));
                if (formatter == nullptr) return Value::null();
                formatter->adoptTimeZone(TimeZone::createTimeZone(from_utf8(zone)));
                // php-src opens by style and then applies the pattern, which keeps ICU's
                // resolved (actual) locale instead of pinning the requested one.
                if (!pattern.empty()) {
                    if (auto *simple = dynamic_cast<SimpleDateFormat *>(formatter)) {
                        simple->applyPattern(from_utf8(pattern));
                    }
                }
                object->simple = dynamic_cast<SimpleDateFormat *>(formatter);
                object->formatter.reset(formatter);
            } else {
                auto *simple = new SimpleDateFormat(
                    from_utf8(pattern), locale.empty() ? Locale::getDefault() : Locale(locale.c_str()),
                    create_status);
                if (U_FAILURE(create_status)) {
                    delete simple;
                    return Value::null();
                }
                simple->adoptTimeZone(TimeZone::createTimeZone(from_utf8(zone)));
                object->simple = simple;
                object->formatter.reset(simple);
            }
            object->date_style = date_style;
            object->time_style = time_style;
            object->calendar_kind = calendar;
            object->locale = locale;
            (void)calendar_locale;
            return wrap_object(insert(std::move(object)), "IntlDateFormatter");
        }
    }
    if (family == "bundle" || family == "resourcebundle") {
        if (operation == "resourcebundle_locales" || operation == "getLocales") {
            std::string bundle;
            if (!argument_string(args, 0, bundle)) return Value::boolean(false);
            UErrorCode locale_status = U_ZERO_ERROR;
            UEnumeration *locales = ures_openAvailableLocales(bundle.c_str(), &locale_status);
            if (U_FAILURE(locale_status) || locales == nullptr) return Value::boolean(false);
            Value result = Value::array();
            int32_t index = 0;
            while (const char *id = uenum_next(locales, nullptr, &locale_status)) {
                result.entries.emplace_back(Value::from_int(index++), Value::from_string(id));
            }
            uenum_close(locales);
            return result;
        }
        if (operation == "resourcebundle_create") {
            std::string locale, bundle;
            bool fallback = true;
            optional_string(args, 0, locale);
            optional_string(args, 1, bundle);
            optional_bool(args, 2, true, fallback);
            UErrorCode local_status = U_ZERO_ERROR;
            std::unique_ptr<ResourceBundle> resource;
            std::string context;
            if (!open_resource_bundle(locale, bundle, fallback, resource, local_status, context)) {
                set_global_error(local_status, "resourcebundle_create(): " + context);
                return Value::null();
            }
            auto object = std::make_unique<ResourceBundleObject>();
            object->bundle = std::move(resource);
            (void)fallback;
            if (local_status != U_ZERO_ERROR) set_global_error(local_status, "");
            else set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            return wrap_object(insert(std::move(object)), "ResourceBundle");
        }
    }
    if (family == "timezone" || family == "intltz") {
        if (operation == "intltz_create_time_zone" || operation == "intltz_create_default" ||
            operation == "intltz_get_gmt" || operation == "intltz_get_unknown") {
            std::string id;
            if (operation == "intltz_create_time_zone" && !argument_string(args, 0, id))
                return Value::null();
            if (operation == "intltz_create_time_zone" && !valid_utf8(id)) {
                set_global_error(U_INVALID_CHAR_FOUND,
                    "IntlTimeZone::createTimeZone(): could not convert time zone id to UTF-16");
                return Value::null();
            }
            const TimeZone *zone = operation == "intltz_create_default" ? TimeZone::createDefault()
                : operation == "intltz_get_gmt" ? TimeZone::getGMT()
                : TimeZone::createTimeZone(from_utf8(id));
            if (zone == nullptr) return Value::null();
            auto object = std::make_unique<TimeZoneObject>();
            object->timezone.reset(zone->clone());
            return wrap_object(insert(std::move(object)), "IntlTimeZone");
        }
        if (operation == "intltz_create_time_zone_id_enumeration") {
            int64_t type = 0;
            if (!argument_int(args, 0, type)) return Value::boolean(false);
            std::string region;
            int64_t raw_offset = 0;
            optional_string(args, 1, region);
            bool has_offset = args.size() > 2 && args[2].type == Value::Type::Int;
            if (has_offset) argument_int(args, 2, raw_offset);
            UErrorCode enum_status = U_ZERO_ERROR;
            int32_t offset_value = int32_t(raw_offset);
            icu::StringEnumeration *zones = TimeZone::createTimeZoneIDEnumeration(
                USystemTimeZoneType(type),
                region.empty() ? nullptr : region.c_str(),
                has_offset ? &offset_value : nullptr,
                enum_status);
            if (U_FAILURE(enum_status) || zones == nullptr) return Value::boolean(false);
            Value result = Value::array();
            int32_t index = 0;
            while (const char *id = zones->next(nullptr, enum_status)) {
                result.entries.emplace_back(Value::from_int(index++), Value::from_string(id));
            }
            delete zones;
            return result;
        }
        if (operation == "intltz_create_enumeration") {
            UErrorCode enum_status = U_ZERO_ERROR;
            std::string country;
            int64_t raw_offset = 0;
            std::unique_ptr<icu::StringEnumeration> zones;
            if (!args.empty() && args[0].type == Value::Type::Str &&
                optional_string(args, 0, country) && !country.empty()) {
                zones.reset(TimeZone::createEnumeration(country.c_str()));
            } else if (!args.empty() && args[0].type == Value::Type::Int) {
                argument_int(args, 0, raw_offset);
                zones.reset(TimeZone::createEnumerationForRawOffset(int32_t(raw_offset),
                                                                   enum_status));
            } else {
                zones.reset(TimeZone::createEnumeration());
            }
            if (!zones) return Value::boolean(false);
            Value result = Value::array();
            int32_t index = 0;
            while (const char *id = zones->next(nullptr, enum_status)) {
                result.entries.emplace_back(Value::from_int(index++), Value::from_string(id));
            }
            return result;
        }
        if (operation == "intltz_get_canonical_id") {
            std::string id;
            if (!argument_string(args, 0, id)) return Value::boolean(false);
            if (!valid_utf8(id)) {
                set_global_error(U_INVALID_CHAR_FOUND,
                    "IntlTimeZone::getCanonicalID(): could not convert time zone id to UTF-16");
                return Value::boolean(false);
            }
            UnicodeString canonical;
            UBool system = 0;
            UErrorCode canonical_status = U_ZERO_ERROR;
            TimeZone::getCanonicalID(from_utf8(id), canonical, system, canonical_status);
            if (U_FAILURE(canonical_status)) {
                set_global_error(canonical_status,
                    "IntlTimeZone::getCanonicalID(): error obtaining canonical ID");
                return Value::boolean(false);
            }
            set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            Value result = Value::array();
            result.entries = {
                {Value::from_int(0), Value::from_string(to_utf8(canonical))},
                {Value::from_int(1), Value::boolean(system != 0)},
            };
            return result;
        }
        if (operation == "intltz_count_equivalent_ids") {
            std::string id;
            if (!argument_string(args, 0, id)) return Value::boolean(false);
            if (!valid_utf8(id)) {
                set_global_error(U_INVALID_CHAR_FOUND,
                    "IntlTimeZone::countEquivalentIDs(): could not convert time zone id to UTF-16");
                return Value::boolean(false);
            }
            int32_t count = TimeZone::countEquivalentIDs(from_utf8(id));
            set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            return count < 0 ? Value::boolean(false) : Value::from_int(count);
        }
        if (operation == "intltz_get_equivalent_id") {
            if (args.size() > 0 && args[0].type == Value::Type::Str && !valid_utf8(args[0].text)) {
                set_global_error(U_INVALID_CHAR_FOUND,
                    "IntlTimeZone::getEquivalentID(): could not convert time zone id to UTF-16");
                return Value::boolean(false);
            }
            std::string id;
            int64_t offset = 0;
            if (!argument_string(args, 0, id) || !argument_int(args, 1, offset))
                return Value::boolean(false);
            UnicodeString equivalent = TimeZone::getEquivalentID(from_utf8(id),
                                                                 int32_t(offset));
            if (equivalent.isEmpty()) return Value::boolean(false);
            return Value::from_string(to_utf8(equivalent));
        }
        if (operation == "intltz_get_tz_data_version") {
            UErrorCode version_status = U_ZERO_ERROR;
            const char *version = TimeZone::getTZDataVersion(version_status);
            if (U_FAILURE(version_status) || version == nullptr) return Value::boolean(false);
            return Value::from_string(version);
        }
        if (operation == "intltz_get_id_for_windows_id") {
            if (args.size() > 0 && args[0].type == Value::Type::Str && !valid_utf8(args[0].text)) {
                set_global_error(U_INVALID_CHAR_FOUND,
                    "IntlTimeZone::getIDForWindowsID(): could not convert time zone id to UTF-16");
                return Value::boolean(false);
            }
        }
        if (operation == "intltz_get_windows_id") {
            std::string id;
            if (!argument_string(args, 0, id)) return Value::boolean(false);
            if (!valid_utf8(id)) {
                set_global_error(U_INVALID_CHAR_FOUND,
                    "IntlTimeZone::getWindowsID(): could not convert time zone id to UTF-16");
                return Value::boolean(false);
            }
            UnicodeString windows;
            UErrorCode windows_status = U_ZERO_ERROR;
            TimeZone::getWindowsID(from_utf8(id), windows, windows_status);
            if (U_FAILURE(windows_status) || windows.isEmpty()) {
                set_global_error(windows_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                                : windows_status,
                    "IntlTimeZone::getWindowsID(): unknown system timezone");
                return Value::boolean(false);
            }
            set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            return Value::from_string(to_utf8(windows));
        }
        if (operation == "intltz_get_id_for_windows_id") {
            std::string id, region;
            if (!argument_string(args, 0, id)) return Value::boolean(false);
            optional_string(args, 1, region);
            UnicodeString tzid;
            UErrorCode windows_status = U_ZERO_ERROR;
            TimeZone::getIDForWindowsID(from_utf8(id),
                region.empty() ? nullptr : region.c_str(), tzid, windows_status);
            if (U_FAILURE(windows_status) || tzid.isEmpty()) {
                set_global_error(windows_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                                : windows_status,
                    "IntlTimeZone::getIDForWindowsID(): unknown windows timezone");
                return Value::boolean(false);
            }
            set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            return Value::from_string(to_utf8(tzid));
        }
        if (operation == "intltz_get_iana_id") {
            std::string id;
            if (!argument_string(args, 0, id)) return Value::boolean(false);
            if (!valid_utf8(id)) {
                set_global_error(U_INVALID_CHAR_FOUND,
                    "IntlTimeZone::getIanaID(): could not convert time zone id to UTF-16");
                return Value::boolean(false);
            }
            UErrorCode iana_status = U_ZERO_ERROR;
            UnicodeString iana;
            TimeZone::getIanaID(from_utf8(id), iana, iana_status);
            if (U_FAILURE(iana_status) || iana.isEmpty()) {
                set_global_error(iana_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                             : iana_status,
                    "IntlTimeZone::getIanaID(): error obtaining IANA ID");
                return Value::boolean(false);
            }
            set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            return Value::from_string(to_utf8(iana));
        }
        if (operation == "intltz_get_region") {
            std::string id;
            if (!argument_string(args, 0, id)) return Value::boolean(false);
            if (!valid_utf8(id)) {
                set_global_error(U_INVALID_CHAR_FOUND,
                    "IntlTimeZone::getRegion(): could not convert time zone id to UTF-16");
                return Value::boolean(false);
            }
            char region[8];
            UErrorCode region_status = U_ZERO_ERROR;
            int32_t length = TimeZone::getRegion(from_utf8(id), region, sizeof(region),
                                                 region_status);
            if (U_FAILURE(region_status) || length == 0) {
                set_global_error(region_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                               : region_status,
                    "IntlTimeZone::getRegion(): error obtaining region");
                return Value::boolean(false);
            }
            set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            return Value::from_string(std::string(region, size_t(length)));
        }
    }
    if (family == "intlcal" || family == "intlgregcal") {
        std::string operation = operation_in;
        bool is_gregorian = family == "intlgregcal";
        if (operation == "intlcal_create_instance" || operation == "intlgregcal_create_instance") {
            UErrorCode create_status = U_ZERO_ERROR;
            TimeZone *timezone = nullptr;
            std::string locale;
            if (is_gregorian && !args.empty() && args[0].type == Value::Type::Int) {
                int64_t year = args[0].integer;
                int64_t month = args.size() > 1 && args[1].type == Value::Type::Int ? args[1].integer : 0;
                int64_t day = args.size() > 2 && args[2].type == Value::Type::Int ? args[2].integer : 1;
                int64_t hour = args.size() > 3 && args[3].type == Value::Type::Int ? args[3].integer : 0;
                int64_t minute = args.size() > 4 && args[4].type == Value::Type::Int ? args[4].integer : 0;
                int64_t second = args.size() > 5 && args[5].type == Value::Type::Int ? args[5].integer : 0;
                timezone = TimeZone::createDefault();
                auto object = std::make_unique<CalendarObject>();
                object->calendar.reset(new icu::GregorianCalendar(timezone, Locale::getDefault(),
                                                                  create_status));
                if (U_FAILURE(create_status)) return Value::null();
                object->calendar->set(int32_t(year), int32_t(month), int32_t(day), int32_t(hour),
                                      int32_t(minute), int32_t(second));
                return wrap_object(insert(std::move(object)), "IntlCalendar");
            }
            std::string zone;
            if (!args.empty() && args[0].type == Value::Type::Str) zone = args[0].text;
            if (args.size() > 1 && args[1].type == Value::Type::Str) locale = args[1].text;
            timezone = zone.empty() ? TimeZone::createDefault()
                : TimeZone::createTimeZone(from_utf8(zone));
            Locale actual(locale.empty() ? Locale::getDefault().getName() : locale.c_str());
            Calendar *calendar = is_gregorian
                ? new icu::GregorianCalendar(timezone, actual, create_status)
                : Calendar::createInstance(timezone, actual, create_status);
            if (U_FAILURE(create_status) || calendar == nullptr) return Value::null();
            auto object = std::make_unique<CalendarObject>();
            object->calendar.reset(calendar);
            return wrap_object(insert(std::move(object)), "IntlCalendar");
        }
        if (operation == "intlcal_get_now") {
            return Value::from_double(double(Calendar::getNow()));
        }
        if (operation == "intlcal_get_available_locales") {
            int32_t count = 0;
            const Locale *locales = Calendar::getAvailableLocales(count);
            Value result = Value::array();
            for (int32_t index = 0; index < count; index++) {
                result.entries.emplace_back(Value::from_int(index),
                                            Value::from_string(locales[index].getName()));
            }
            return result;
        }
        if (operation == "intlcal_get_keyword_values_for_locale") {
            std::string keyword, locale;
            bool commonly_used = false;
            if (!argument_string(args, 0, keyword) || !argument_string(args, 1, locale) ||
                !argument_bool(args, 2, commonly_used)) {
                return Value::boolean(false);
            }
            UErrorCode keyword_status = U_ZERO_ERROR;
            std::unique_ptr<icu::StringEnumeration> values(
                Calendar::getKeywordValuesForLocale(keyword.c_str(),
                    Locale::createFromName(locale.c_str()), UBool(commonly_used),
                    keyword_status));
            if (U_FAILURE(keyword_status) || !values) return Value::boolean(false);
            Value result = Value::array();
            int32_t index = 0;
            while (const char *value = values->next(nullptr, keyword_status)) {
                result.entries.emplace_back(Value::from_int(index++),
                                            Value::from_string(value));
            }
            return result;
        }
        if (operation == "intlcal_from_date_time" || operation == "intlcal_to_date_time") {
            return Value::null();
        }
        if (args.empty() || args[0].type != Value::Type::Int) return Value::null();
        IntlObject *object = lookup(args[0].integer);
        auto *calendar = dynamic_cast<CalendarObject *>(object);
        if (calendar == nullptr) return Value::boolean(false);
        std::vector<Value> rest(args.begin() + 1, args.end());
        Value result = Value::null();
        int32_t call_status = calendar->call(operation, rest, result);
        if (U_FAILURE(UErrorCode(call_status))) return Value::boolean(false);
        return result;
    }
    if (family == "calendar") {
        if (operation == "intlcal_create_instance") {
            std::string locale, zone;
            optional_string(args, 1, locale);
            optional_string(args, 0, zone);
            Locale actual(locale.empty() ? Locale::getDefault().getName() : locale.c_str());
            TimeZone *timezone = zone.empty() ? TimeZone::createDefault()
                : TimeZone::createTimeZone(from_utf8(zone));
            Calendar *calendar = Calendar::createInstance(timezone, actual, status);
            if (U_FAILURE(status)) return Value::null();
            auto object = std::make_unique<CalendarObject>();
            object->calendar.reset(calendar);
            return wrap_object(insert(std::move(object)), "IntlCalendar");
        }
        if (operation == "intlcal_get_now") {
            // Calendar::getNow() is already an ICU UDate in milliseconds.
            return Value::from_double(double(Calendar::getNow()));
        }
        if (operation == "intlcal_get_keyword_values_for_locale") {
            std::string keyword, locale;
            bool commonly_used = false;
            if (!argument_string(args, 0, keyword) || !argument_string(args, 1, locale) ||
                !argument_bool(args, 2, commonly_used)) {
                return Value::boolean(false);
            }
            UErrorCode keyword_status = U_ZERO_ERROR;
            std::unique_ptr<icu::StringEnumeration> values(
                Calendar::getKeywordValuesForLocale(keyword.c_str(),
                    Locale::createFromName(locale.c_str()), UBool(commonly_used),
                    keyword_status));
            if (U_FAILURE(keyword_status) || !values) {
                return Value::boolean(false);
            }
            Value result = Value::array();
            int32_t index = 0;
            while (const char *value = values->next(nullptr, keyword_status)) {
                result.entries.emplace_back(Value::from_int(index++),
                                            Value::from_string(value));
            }
            return result;
        }
        if (operation == "intlcal_get_available_locales") {
            int32_t count = 0;
            const Locale *locales = Calendar::getAvailableLocales(count);
            Value result = Value::array();
            for (int32_t index = 0; index < count; index++) {
                result.entries.emplace_back(Value::from_int(index),
                                            Value::from_string(locales[index].getName()));
            }
            return result;
        }
    }
    if (family == "uconverter") {
        if (operation == "transcode") {
            std::string input, to, from;
            if (!argument_string(args, 0, input) || !argument_string(args, 1, to) ||
                !argument_string(args, 2, from)) {
                return Value::boolean(false);
            }
            char substitution = '?';
            if (args.size() > 3 && args[3].type == Value::Type::Array) {
                for (const auto &entry : args[3].entries) {
                    if (entry.first.type != Value::Type::Str || entry.first.text != "to_subst") {
                        continue;
                    }
                    if (entry.second.type != Value::Type::Str || entry.second.text.size() != 1) {
                        set_global_error(U_ILLEGAL_ARGUMENT_ERROR,
                            "UConverter::transcode(): returned error 1: U_ILLEGAL_ARGUMENT_ERROR");
                        return Value::boolean(false);
                    }
                    substitution = entry.second.text[0];
                }
            }
            UErrorCode transcoder_status = U_ZERO_ERROR;
            int32_t needed = ucnv_convert(to.c_str(), from.c_str(), nullptr, 0,
                                          input.data(), int32_t(input.size()),
                                          &transcoder_status);
            if (transcoder_status == U_BUFFER_OVERFLOW_ERROR) transcoder_status = U_ZERO_ERROR;
            if (U_FAILURE(transcoder_status)) {
                set_global_error(transcoder_status, "UConverter::transcode()");
                return Value::boolean(false);
            }
            std::string output(size_t(needed), '\0');
            ucnv_convert(to.c_str(), from.c_str(), output.data(), needed,
                         input.data(), int32_t(input.size()), &transcoder_status);
            if (U_FAILURE(transcoder_status)) {
                set_global_error(transcoder_status, "UConverter::transcode()");
                return Value::boolean(false);
            }
            size_t position = 0;
            while ((position = output.find('\x1A', position)) != std::string::npos) {
                output[position] = substitution;
                position += 1;
            }
            set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            return Value::from_string(output);
        }
        if (operation == "get_available") {
            Value result = Value::array();
            int32_t count = ucnv_countAvailable();
            for (int32_t index = 0; index < count; index++) {
                const char *name = ucnv_getAvailableName(index);
                if (name != nullptr) {
                    result.entries.emplace_back(Value::from_int(index),
                                                Value::from_string(name));
                }
            }
            return result;
        }
        if (operation == "get_standards") {
            Value result = Value::array();
            uint16_t count = ucnv_countStandards();
            for (uint16_t index = 0; index < count; index++) {
                UErrorCode standard_status = U_ZERO_ERROR;
                const char *name = ucnv_getStandard(index, &standard_status);
                if (U_FAILURE(standard_status)) return Value::null();
                result.entries.emplace_back(Value::from_int(int64_t(index)),
                                            Value::from_string(name));
            }
            return result;
        }
        if (operation == "get_aliases") {
            std::string name;
            if (!argument_string(args, 0, name)) return Value::boolean(false);
            Value result = Value::boolean(false);
            ConverterObject::converter_aliases(name, result);
            return result;
        }
        if (operation == "reason_text") {
            int64_t reason = 0;
            if (!argument_int(args, 0, reason)) return Value::boolean(false);
            static const char *names[] = {"REASON_UNASSIGNED", "REASON_ILLEGAL",
                "REASON_IRREGULAR", "REASON_RESET", "REASON_CLOSE", "REASON_CLONE"};
            if (reason < 0 || reason > 5) {
                set_global_error(U_ILLEGAL_ARGUMENT_ERROR,
                                 "UConverter::reasonText(): Argument #1 ($reason) must be a UConverter::REASON_* constant");
                return Value::boolean(false);
            }
            set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
            return Value::from_string(names[reason]);
        }
    }
    if (family == "transliterator") {
        if (operation == "transliterator_create" || operation == "transliterator_create_from_rules") {
            std::string id;
            if (!argument_string(args, 0, id)) return Value::null();
            int64_t direction = 0;
            optional_int(args, 1, 0, direction);
            bool from_rules = operation == "transliterator_create_from_rules";
            if (!valid_utf8(id)) {
                set_global_error(U_INVALID_CHAR_FOUND,
                    from_rules
                        ? "Transliterator::createFromRules(): String conversion of rules to UTF-16 failed"
                        : "Transliterator::create(): String conversion of id to UTF-16 failed");
                return Value::null();
            }
            UParseError parse_error;
            // PHP's `createFromRules(string $rules)` uses the fixed id "RulesTransPHP".
            UnicodeString object_id = from_rules ? UnicodeString("RulesTransPHP") : from_utf8(id);
            Transliterator *transliterator = from_rules
                ? Transliterator::createFromRules(object_id, from_utf8(id),
                                                  UTransDirection(direction), parse_error,
                                                  status)
                : Transliterator::createInstance(from_utf8(id), UTransDirection(direction),
                                                 status);
            if (U_FAILURE(status) || transliterator == nullptr) {
                set_global_error(status,
                    from_rules
                        ? "Transliterator::createFromRules(): unable to create ICU transliterator from rules (" + parse_error_to_string(parse_error) + ")"
                        : "Transliterator::create(): unable to open ICU transliterator with id \"" + id + "\"");
                return Value::null();
            }
            auto object = std::make_unique<TransliteratorObject>();
            object->transliterator.reset(transliterator);
            object->id = to_utf8(object_id);
            return wrap_object(insert(std::move(object)), "Transliterator");
        }
        if (operation == "transliterator_utf16_length") {
            std::string text;
            if (!argument_string(args, 0, text)) return Value::boolean(false);
            return Value::from_int(int64_t(from_utf8(text).length()));
        }
        if (operation == "transliterator_list_ids") {
            std::unique_ptr<icu::StringEnumeration> ids(Transliterator::getAvailableIDs(status));
            if (!ids) return Value::boolean(false);
            Value result = Value::array();
            int32_t index = 0;
            UErrorCode id_status = U_ZERO_ERROR;
            while (const char *id = ids->next(nullptr, id_status)) {
                result.entries.emplace_back(Value::from_int(index++), Value::from_string(id));
            }
            return result;
        }
    }
    // Procedural entry points whose first argument is an Intl object handle share the object
    // body with the OO methods. Strip the function prefix and dispatch the rest.
    if (!args.empty() && args[0].type == Value::Type::Int) {
        IntlObject *object = lookup(args[0].integer);
        if (object != nullptr) {
            std::string method = operation;
            for (const char *prefix : {"datefmt_", "numfmt_", "msgfmt_", "collator_",
                                       "resourcebundle_", "uconverter_", "transliterator_",
                                       "intltz_"}) {
                size_t length = std::strlen(prefix);
                if (method.rfind(prefix, 0) == 0) {
                    method = method.substr(length);
                    break;
                }
            }
            if (method == "get_pattern") method = "pattern";
            else if (method == "get_locale") method = "locale";
            else if (method == "get_error_code") method = "error_code";
            else if (method == "get_error_message") method = "error_message";
            else if (method == "get_timezone_id") method = "timezone_id";
            else if (method == "set_timezone_id") method = "set_timezone";
            else if (method == "get_calendar") method = "calendar_kind";
            else if (method == "get_datetype") method = "datetype";
            else if (method == "get_timetype") method = "timetype";
            else if (method == "get_calendar_object") method = "calendar_object";
            else if (method == "get_sort_key") method = "sort_key";
            std::vector<Value> rest(args.begin() + 1, args.end());
            Value result = Value::null();
            int32_t call_status = object->call(method, rest, result);
            if (U_FAILURE(UErrorCode(call_status))) return Value::boolean(false);
            return result;
        }
    }
    return Value::null();
}

/* --------------------------------------------------------------------------------------- */
/* Open dispatch                                                                            */
/* --------------------------------------------------------------------------------------- */

std::unique_ptr<IntlObject> open_object(const std::string &kind,
                                        const std::vector<Value> &values,
                                        UErrorCode &status) {
    if (kind == "date") {
        auto object = std::make_unique<DateFormatterObject>();
        std::string locale, zone, pattern;
        int64_t date_style = UDAT_FULL, time_style = UDAT_FULL;
        int64_t calendar = UCAL_GREGORIAN;
        optional_string(values, 0, locale);
        locale = canonicalize_formatter_locale(locale);
        optional_int(values, 1, UDAT_FULL, date_style);
        optional_int(values, 2, UDAT_FULL, time_style);
        bool has_zone = optional_string(values, 3, zone);
        optional_int(values, 4, UCAL_GREGORIAN, calendar);
        bool has_pattern = optional_string(values, 5, pattern);
        int64_t calendar_handle = 0;
        optional_int(values, 6, 0, calendar_handle);
        bool timezone_given = false;
        optional_bool(values, 7, false, timezone_given);
        std::string call_prefix;
        optional_string(values, 8, call_prefix);
        auto creation_failed = [&](UErrorCode code) {
            set_global_error(code, call_prefix.empty()
                ? "date formatter creation failed"
                : call_prefix + ": date formatter creation failed");
        };
        bool style_pattern = time_style == UDAT_PATTERN || date_style == UDAT_PATTERN;
        if (!style_pattern) {
            DateFormat *formatter = DateFormat::createDateTimeInstance(
                DateFormat::EStyle(date_style), DateFormat::EStyle(time_style),
                locale.empty() ? Locale::getDefault() : Locale(locale.c_str()));
            if (formatter == nullptr) {
                status = U_MEMORY_ALLOCATION_ERROR;
                creation_failed(status);
                return nullptr;
            }
            if (has_pattern && !pattern.empty()) {
                if (auto *simple = dynamic_cast<SimpleDateFormat *>(formatter)) {
                    simple->applyPattern(from_utf8(pattern));
                }
            }
            object->simple = dynamic_cast<SimpleDateFormat *>(formatter);
            object->formatter.reset(formatter);
        } else {
            auto *simple = new SimpleDateFormat(from_utf8(pattern),
                locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), status);
            if (U_FAILURE(status)) {
                delete simple;
                creation_failed(status);
                return nullptr;
            }
            object->simple = simple;
            object->formatter.reset(simple);
        }
        // IntlDateFormatter::GREGORIAN (1) forces the Gregorian calendar; TRADITIONAL (0)
        // keeps the locale calendar. An IntlCalendar argument adopts its own calendar.
        if (calendar == 1) {
            std::unique_ptr<Calendar> gregorian(new icu::GregorianCalendar(
                object->formatter->getTimeZone().clone(),
                object->formatter->getLocale(ULOC_VALID_LOCALE, status), status));
            if (U_FAILURE(status)) {
                creation_failed(status);
                return nullptr;
            }
            object->formatter->adoptCalendar(gregorian.release());
        }
        if (calendar_handle > 0) {
            if (!adopt_calendar_from_handle(*object->formatter, calendar_handle)) {
                status = U_ILLEGAL_ARGUMENT_ERROR;
                creation_failed(status);
                return nullptr;
            }
            UnicodeString adopted;
            object->formatter->getTimeZone().getID(adopted);
        }
        // An explicit timezone string wins over the calendar's own zone; a NULL timezone
        // keeps the calendar's zone and otherwise falls back to PHP's default.
        if (has_zone && (timezone_given || calendar_handle == 0)) {
            object->formatter->adoptTimeZone(TimeZone::createTimeZone(from_utf8(zone)));
        }
        object->date_style = date_style;
        object->time_style = time_style;
        object->calendar_kind = calendar;
        object->locale = locale;
        return object;
    }
    if (kind == "number") {
        std::string locale, pattern;
        int64_t style = UNUM_DECIMAL;
        optional_string(values, 0, locale);
        locale = canonicalize_formatter_locale(locale);
        optional_int(values, 1, UNUM_DECIMAL, style);
        bool has_pattern = optional_string(values, 2, pattern) && !pattern.empty();
        UErrorCode local_status = U_ZERO_ERROR;
        bool pattern_style = style == UNUM_PATTERN_DECIMAL || style == UNUM_PATTERN_RULEBASED;
        NumberFormat *formatter = open_number_format(style, locale, pattern, has_pattern,
                                                     local_status);
        if (U_FAILURE(local_status) || formatter == nullptr) {
            delete formatter;
            status = local_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR : local_status;
            return nullptr;
        }
        if (has_pattern && !pattern_style) {
            auto *decimal = dynamic_cast<DecimalFormat *>(formatter);
            if (decimal != nullptr) decimal->applyPattern(from_utf8(pattern), local_status);
        }
        if (U_FAILURE(local_status)) {
            delete formatter;
            status = local_status;
            return nullptr;
        }
        auto object = std::make_unique<NumberFormatterObject>();
        object->formatter.reset(formatter);
        object->locale = locale;
        object->style = style;
        return object;
    }
    if (kind == "collator") {
        std::string locale;
        optional_string(values, 0, locale);
        if (locale.size() > 156) {
            set_global_error(U_ILLEGAL_ARGUMENT_ERROR,
                "Locale string too long, should be no longer than 156 characters");
            status = U_ILLEGAL_ARGUMENT_ERROR;
            return nullptr;
        }
        UErrorCode local_status = U_ZERO_ERROR;
        Collator *collator = Collator::createInstance(
            locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), local_status);
        if (U_FAILURE(local_status) || collator == nullptr) {
            set_global_error(U_ILLEGAL_ARGUMENT_ERROR, "unable to open ICU collator");
            status = local_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR : local_status;
            return nullptr;
        }
        auto object = std::make_unique<CollatorObject>();
        object->collator.reset(collator);
        object->locale = locale;
        return object;
    }
    if (kind == "message") {
        std::string locale, pattern;
        optional_string(values, 0, locale);
        if (!optional_string(values, 1, pattern)) {
            status = U_ILLEGAL_ARGUMENT_ERROR;
            return nullptr;
        }
        UParseError parse_error = {0};
        auto *formatter = new MessageFormat(from_utf8(pattern),
            locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), parse_error, status);
        if (U_FAILURE(status)) {
            delete formatter;
            set_message_formatter_creation_error(status, locale, parse_error, U_ZERO_ERROR);
            return nullptr;
        }
        auto object = std::make_unique<MessageFormatterObject>();
        object->formatter.reset(formatter);
        object->locale = locale;
        object->pattern = pattern;
        return object;
    }
    if (kind == "timezone") {
        std::string id;
        if (!optional_string(values, 0, id)) id = "GMT";
        if (!valid_utf8(id)) {
            set_global_error(U_INVALID_CHAR_FOUND,
                "IntlTimeZone::createTimeZone(): could not convert time zone id to UTF-16");
            status = U_INVALID_CHAR_FOUND;
            return nullptr;
        }
        auto object = std::make_unique<TimeZoneObject>();
        object->timezone.reset(TimeZone::createTimeZone(from_utf8(id)));
        if (!object->timezone) {
            status = U_ILLEGAL_ARGUMENT_ERROR;
            return nullptr;
        }
        set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
        return object;
    }
    if (kind == "calendar") {
        std::string locale, zone;
        optional_string(values, 0, zone);
        optional_string(values, 1, locale);
        TimeZone *timezone = zone.empty() ? TimeZone::createDefault()
            : TimeZone::createTimeZone(from_utf8(zone));
        Calendar *calendar = Calendar::createInstance(timezone,
            locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), status);
        if (U_FAILURE(status)) {
            return nullptr;
        }
        auto object = std::make_unique<CalendarObject>();
        object->calendar.reset(calendar);
        return object;
    }
    if (kind == "breakiter") {
        std::string locale, text;
        int64_t type = 0;
        optional_string(values, 0, locale);
        optional_int(values, 1, 0, type);
        UErrorCode local_status = U_ZERO_ERROR;
        BreakIterator *iterator = nullptr;
        switch (type) {
            case 1: iterator = BreakIterator::createWordInstance(
                locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), local_status); break;
            case 2: iterator = BreakIterator::createLineInstance(
                locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), local_status); break;
            case 3: iterator = BreakIterator::createSentenceInstance(
                locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), local_status); break;
            case 4: iterator = BreakIterator::createTitleInstance(
                locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), local_status); break;
            default: iterator = BreakIterator::createCharacterInstance(
                locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), local_status); break;
        }
        if (U_FAILURE(local_status) || iterator == nullptr) {
            status = local_status;
            return nullptr;
        }
        auto object = std::make_unique<BreakIteratorObject>();
        object->iterator.reset(iterator);
        if (optional_string(values, 2, text)) {
            object->text = from_utf8(text);
            iterator->setText(object->text);
        }
        return object;
    }
    if (kind == "codepointbreakiter") {
        std::string locale;
        optional_string(values, 0, locale);
        UErrorCode local_status = U_ZERO_ERROR;
        BreakIterator *iterator = BreakIterator::createCharacterInstance(
            locale.empty() ? Locale::getDefault() : Locale(locale.c_str()), local_status);
        if (U_FAILURE(local_status) || iterator == nullptr) {
            status = local_status;
            return nullptr;
        }
        auto object = std::make_unique<BreakIteratorObject>();
        object->iterator.reset(iterator);
        object->code_point = true;
        return object;
    }
    if (kind == "rulebasedbreakiter") {
        std::string rules;
        bool compiled = false;
        if (!optional_string(values, 0, rules)) {
            status = U_ILLEGAL_ARGUMENT_ERROR;
            return nullptr;
        }
        optional_bool(values, 1, false, compiled);
        UErrorCode local_status = U_ZERO_ERROR;
        UParseError parse_error;
        BreakIterator *iterator = nullptr;
        if (compiled) {
            iterator = new icu::RuleBasedBreakIterator(
                reinterpret_cast<const uint8_t *>(rules.data()), int32_t(rules.size()),
                local_status);
        } else {
            iterator = new icu::RuleBasedBreakIterator(from_utf8(rules), parse_error,
                                                       local_status);
        }
        if (U_FAILURE(local_status) || iterator == nullptr) {
            if (compiled) {
                set_global_error(local_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR : local_status,
                    "unable to create instance from compiled rules");
            } else {
                char detail[128];
                std::snprintf(detail, sizeof(detail), " (parse error on line %d, offset %d)",
                              parse_error.line, parse_error.offset);
                set_global_error(local_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR : local_status,
                    "unable to create RuleBasedBreakIterator from rules" + std::string(detail));
            }
            status = local_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR : local_status;
            return nullptr;
        }
        auto object = std::make_unique<BreakIteratorObject>();
        object->iterator.reset(iterator);
        return object;
    }
    if (kind == "transliterator") {
        std::string id;
        int64_t direction = 0;
        if (!optional_string(values, 0, id)) {
            status = U_ILLEGAL_ARGUMENT_ERROR;
            return nullptr;
        }
        optional_int(values, 1, 0, direction);
        Transliterator *transliterator = Transliterator::createInstance(
            from_utf8(id), UTransDirection(direction), status);
        if (U_FAILURE(status) || transliterator == nullptr) return nullptr;
        auto object = std::make_unique<TransliteratorObject>();
        object->transliterator.reset(transliterator);
        object->id = id;
        return object;
    }
    if (kind == "spoof") {
        UErrorCode local_status = U_ZERO_ERROR;
        auto object = std::make_unique<SpoofCheckerObject>();
        object->checker = uspoof_open(&local_status);
        if (U_FAILURE(local_status)) {
            status = local_status;
            return nullptr;
        }
        return object;
    }
    if (kind == "bundle") {
        std::string locale, path;
        bool fallback = true;
        optional_string(values, 0, locale);
        optional_string(values, 1, path);
        optional_bool(values, 2, true, fallback);
        UErrorCode local_status = U_ZERO_ERROR;
        std::unique_ptr<ResourceBundle> bundle;
        std::string context;
        if (!open_resource_bundle(locale, path, fallback, bundle, local_status, context)) {
            set_global_error(local_status, "ResourceBundle::__construct(): " + context);
            status = local_status;
            return nullptr;
        }
        auto object = std::make_unique<ResourceBundleObject>();
        object->bundle = std::move(bundle);
        (void)fallback;
        // php keeps ICU's fallback/default warnings visible through intl_get_error_*.
        if (local_status != U_ZERO_ERROR) set_global_error(local_status, "");
        else set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
        return object;
    }
    if (kind == "datepatterngenerator") {
        std::string locale;
        optional_string(values, 0, locale);
        UErrorCode local_status = U_ZERO_ERROR;
        Locale actual(locale.empty() ? Locale::getDefault().getName() : locale.c_str());
        auto generator = icu::DateTimePatternGenerator::createInstance(actual, local_status);
        if (U_FAILURE(local_status) || generator == nullptr) {
            status = local_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR : local_status;
            return nullptr;
        }
        auto object = std::make_unique<DatePatternGeneratorObject>();
        object->generator.reset(generator);
        return object;
    }
    if (kind == "listformatter") {
        std::string locale;
        int64_t type = 0, width = 0;
        optional_string(values, 0, locale);
        optional_int(values, 1, 0, type);
        optional_int(values, 2, 0, width);
        UErrorCode local_status = U_ZERO_ERROR;
        Locale actual(locale.empty() ? Locale::getDefault().getName() : locale.c_str());
        auto formatter = icu::ListFormatter::createInstance(
            actual, UListFormatterType(type), UListFormatterWidth(width), local_status);
        if (U_FAILURE(local_status) || formatter == nullptr) {
            status = local_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR : local_status;
            return nullptr;
        }
        auto object = std::make_unique<ListFormatterObject>();
        object->formatter.reset(formatter);
        set_global_error(U_ZERO_ERROR, "U_ZERO_ERROR");
        return object;
    }
    if (kind == "converter") {
        std::string destination, source;
        bool has_destination = false, has_source = false;
        if (!values.empty()) {
            switch (values[0].type) {
                case Value::Type::Str: destination = values[0].text; has_destination = true; break;
                case Value::Type::Int: destination = std::to_string(values[0].integer); has_destination = true; break;
                case Value::Type::Float: destination = php_number_to_string(values[0].number); has_destination = true; break;
                case Value::Type::True: destination = "1"; has_destination = true; break;
                default: break;
            }
        }
        if (values.size() > 1) {
            switch (values[1].type) {
                case Value::Type::Str: source = values[1].text; has_source = true; break;
                case Value::Type::Int: source = std::to_string(values[1].integer); has_source = true; break;
                case Value::Type::Float: source = php_number_to_string(values[1].number); has_source = true; break;
                case Value::Type::True: source = "1"; has_source = true; break;
                default: break;
            }
        }
        if (!has_destination || destination.empty()) destination = "UTF-8";
        if (!has_source || source.empty()) source = "UTF-8";
        UErrorCode destination_status = U_ZERO_ERROR;
        std::string canonical_destination =
            ConverterObject::canonical_name(destination, destination_status);
        if (U_FAILURE(destination_status) || canonical_destination.empty()) {
            set_global_error(destination_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                                : destination_status,
                "UConverter::__construct(): returned error " +
                std::to_string(int(destination_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                                      : destination_status)));
            status = destination_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                        : destination_status;
            return nullptr;
        }
        UErrorCode source_status = U_ZERO_ERROR;
        std::string canonical_source = ConverterObject::canonical_name(source, source_status);
        if (U_FAILURE(source_status) || canonical_source.empty()) {
            set_global_error(source_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                           : source_status,
                "UConverter::__construct(): returned error " +
                std::to_string(int(source_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR
                                                                 : source_status)));
            status = source_status == U_ZERO_ERROR ? U_ILLEGAL_ARGUMENT_ERROR : source_status;
            return nullptr;
        }
        auto object = std::make_unique<ConverterObject>();
        object->destination = canonical_destination;
        object->source = canonical_source;
        object->converter = ucnv_open(canonical_destination.c_str(), &status);
        if (U_FAILURE(status)) return nullptr;
        return object;
    }
    status = U_UNSUPPORTED_ERROR;
    return nullptr;
}

} // namespace

/* --------------------------------------------------------------------------------------- */
/* Exported ABI                                                                             */
/* --------------------------------------------------------------------------------------- */

extern "C" int64_t elephc_icu_intl_open(const char *kind, const uint8_t *spec,
                                        uint64_t spec_len, int32_t *error) {
    if (error != nullptr) *error = U_ZERO_ERROR;
    std::vector<Value> values;
    if (!read_arguments(spec, spec_len, values)) {
        if (error != nullptr) *error = U_ILLEGAL_ARGUMENT_ERROR;
        return 0;
    }
    UErrorCode status = U_ZERO_ERROR;
    std::unique_ptr<IntlObject> object = open_object(kind == nullptr ? "" : kind, values, status);
    if (error != nullptr) *error = status;
    if (!object || U_FAILURE(status)) return 0;
    return insert(std::move(object));
}

extern "C" void elephc_icu_intl_close(int64_t handle) {
    std::lock_guard<std::mutex> lock(registry_mutex());
    registry().erase(handle);
}

extern "C" int64_t elephc_icu_intl_clone(int64_t handle, int32_t *error) {
    if (error != nullptr) *error = U_ZERO_ERROR;
    IntlObject *object = lookup(handle);
    if (object == nullptr) {
        if (error != nullptr) *error = U_ILLEGAL_ARGUMENT_ERROR;
        return 0;
    }
    std::unique_ptr<IntlObject> copy = object->clone();
    if (!copy) {
        if (error != nullptr) *error = U_MEMORY_ALLOCATION_ERROR;
        return 0;
    }
    return insert(std::move(copy));
}

extern "C" int32_t elephc_icu_intl_call(int64_t handle, const char *method,
                                        const uint8_t *args, uint64_t args_len,
                                        uint8_t *out, uint64_t out_cap, uint64_t *needed,
                                        int32_t *error) {
    if (error != nullptr) *error = U_ZERO_ERROR;
    if (needed == nullptr) return U_ILLEGAL_ARGUMENT_ERROR;
    *needed = 0;
    IntlObject *object = lookup(handle);
    if (object == nullptr) {
        if (error != nullptr) *error = U_ILLEGAL_ARGUMENT_ERROR;
        return U_ILLEGAL_ARGUMENT_ERROR;
    }
    std::vector<Value> values;
    if (!read_arguments(args, args_len, values)) {
        if (error != nullptr) *error = U_ILLEGAL_ARGUMENT_ERROR;
        return U_ILLEGAL_ARGUMENT_ERROR;
    }
    Value result = Value::null();
    UErrorCode status = UErrorCode(object->call(method == nullptr ? "" : method, values, result));
    if (error != nullptr) *error = status;
    std::string payload = encode_value(result);
    int32_t written = write_output(payload, out, out_cap, needed);
    if (U_SUCCESS(status) && U_FAILURE(UErrorCode(written))) return written;
    return status;
}

extern "C" int32_t elephc_icu_intl_static(const char *kind, const char *method,
                                          const uint8_t *args, uint64_t args_len,
                                          uint8_t *out, uint64_t out_cap, uint64_t *needed,
                                          int32_t *error) {
    if (error != nullptr) *error = U_ZERO_ERROR;
    if (needed == nullptr) return U_ILLEGAL_ARGUMENT_ERROR;
    *needed = 0;
    std::vector<Value> values;
    if (!read_arguments(args, args_len, values)) {
        if (error != nullptr) *error = U_ILLEGAL_ARGUMENT_ERROR;
        return U_ILLEGAL_ARGUMENT_ERROR;
    }
    Value result = static_dispatch(kind == nullptr ? "" : kind, method == nullptr ? "" : method,
                                   values);
    UErrorCode status = result.type == Value::Type::Null && global_error_code != 0
        ? UErrorCode(global_error_code) : U_ZERO_ERROR;
    if (error != nullptr) *error = status;
    std::string payload = encode_value(result);
    int32_t written = write_output(payload, out, out_cap, needed);
    if (U_SUCCESS(status) && U_FAILURE(UErrorCode(written))) return written;
    return status;
}

extern "C" int32_t elephc_icu_intl_error_code(int64_t handle) {
    IntlObject *object = lookup(handle);
    return object == nullptr ? 0 : object->error_code;
}

extern "C" int32_t elephc_icu_intl_error_message(int64_t handle, char *buffer,
                                                 uint64_t capacity, uint64_t *needed) {
    IntlObject *object = lookup(handle);
    std::string message = object == nullptr ? std::string() : object->error_message;
    if (needed == nullptr) return U_ILLEGAL_ARGUMENT_ERROR;
    *needed = uint64_t(message.size());
    if (capacity == 0) return U_ZERO_ERROR;
    if (buffer == nullptr || capacity < message.size()) return U_BUFFER_OVERFLOW_ERROR;
    std::memcpy(buffer, message.data(), message.size());
    return U_ZERO_ERROR;
}

extern "C" int64_t elephc_icu_intl_live_count(void) {
    std::lock_guard<std::mutex> lock(registry_mutex());
    return int64_t(registry().size());
}

extern "C" int32_t elephc_icu_intl_error_name(int32_t code, char *buffer, uint64_t capacity,
                                               uint64_t *needed) {
    std::string value = u_errorName(UErrorCode(code));
    return write_output(value, reinterpret_cast<uint8_t *>(buffer), capacity, needed);
}

extern "C" int32_t elephc_icu_intl_version(char *buffer, uint64_t capacity, uint64_t *needed) {
    std::string value = U_ICU_VERSION;
    return write_output(value, reinterpret_cast<uint8_t *>(buffer), capacity, needed);
}

extern "C" int32_t elephc_icu_intl_data_version(char *buffer, uint64_t capacity,
                                                uint64_t *needed) {
    std::string value = U_ICU_DATA_VERSION;
    return write_output(value, reinterpret_cast<uint8_t *>(buffer), capacity, needed);
}
