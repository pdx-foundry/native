// Finite observations of the macOS library imported by the game's numeric readers.
// Overflow results are platform observations, not portable C guarantees.
#include <ctype.h>
#include <dlfcn.h>
#include <errno.h>
#include <inttypes.h>
#include <locale.h>
#include <mach-o/loader.h>
#include <runetype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/sysctl.h>

#if !defined(__aarch64__)
#error The recorded scanner probe requires ARM64
#endif

static void library_identity(const char *symbol) {
    Dl_info image;
    if (!dladdr(dlsym(RTLD_DEFAULT, symbol), &image)) {
        exit(2);
    }
    const struct mach_header_64 *header = image.dli_fbase;
    if (header->magic != MH_MAGIC_64) {
        exit(2);
    }
    const struct load_command *command = (const void *)(header + 1);
    for (uint32_t index = 0; index < header->ncmds; index++) {
        if (command->cmd == LC_UUID) {
            const struct uuid_command *uuid = (const void *)command;
            printf("{\"symbol\":\"%s\",\"library\":\"%s\",\"uuid\":\"",
                   symbol, image.dli_fname);
            for (size_t byte = 0; byte < sizeof(uuid->uuid); byte++) {
                printf("%02x", uuid->uuid[byte]);
            }
            puts("\"}");
            return;
        }
        command = (const void *)((const char *)command + command->cmdsize);
    }
    exit(2);
}

enum storage { SIGNED32, UNSIGNED32, SIGNED64, UNSIGNED64, FLOAT32, COMPONENTS };

static void scan(const char *format, enum storage storage, const char *input) {
    int signed32 = 7;
    unsigned unsigned32 = 7;
    long long signed64 = 7;
    unsigned long long unsigned64 = 7;
    float float32 = 7;
    double fraction = 0;
    void *destination;
    size_t width;
    switch (storage) {
        case SIGNED32:
            destination = &signed32;
            width = sizeof(signed32);
            break;
        case UNSIGNED32:
            destination = &unsigned32;
            width = sizeof(unsigned32);
            break;
        case SIGNED64:
            destination = &signed64;
            width = sizeof(signed64);
            break;
        case UNSIGNED64:
            destination = &unsigned64;
            width = sizeof(unsigned64);
            break;
        case FLOAT32:
            destination = &float32;
            width = sizeof(float32);
            break;
        case COMPONENTS:
            signed64 = 0;
            destination = &signed64;
            width = sizeof(signed64);
            break;
        default: exit(2);
    }
    errno = 0;
    int assignments = storage == COMPONENTS
        ? sscanf(input, format, destination, &fraction)
        : sscanf(input, format, destination);
    int error = errno;
    uint64_t bits = 0;
    memcpy(&bits, destination, width);
    uint64_t fraction_bits;
    memcpy(&fraction_bits, &fraction, sizeof(fraction_bits));

    // A separate call adds %n to measure consumption. The original call above stays exact.
    char counted_format[32];
    snprintf(counted_format, sizeof(counted_format), "%s%%n", format);
    int consumed = -1;
    int counted_assignments = storage == COMPONENTS
        ? sscanf(input, counted_format, destination, &fraction, &consumed)
        : sscanf(input, counted_format, destination, &consumed);
    uint64_t counted_bits = 0;
    memcpy(&counted_bits, destination, width);
    uint64_t counted_fraction_bits;
    memcpy(&counted_fraction_bits, &fraction, sizeof(counted_fraction_bits));
    if (counted_assignments != assignments || counted_bits != bits || counted_fraction_bits != fraction_bits) {
        exit(2);
    }
    printf("{\"format\":\"%s\",\"input\":\"%s\",\"assignments\":%d,"
           "\"errno\":%d,\"bits\":\"%016" PRIx64 "\","
           "\"fraction_bits\":\"%016" PRIx64 "\","
           "\"counted_assignments\":%d,\"consumed\":%d}\n",
           format, input, assignments, error, bits, fraction_bits, counted_assignments, consumed);
}

static void cases(const char *format, enum storage storage, const char *const *inputs, size_t count) {
    for (size_t index = 0; index < count; index++) {
        scan(format, storage, inputs[index]);
    }
}

#define CASES(format, storage, inputs) cases(format, storage, inputs, sizeof(inputs) / sizeof(*inputs))

// The text lexer's whitespace test: the C-locale table for a byte up to 0x7f, and __maskrune for
// a higher byte, which the lexer sign-extends before the call.
static void lexer_spaces(void) {
    printf("{\"function\":\"lexer_space\",\"bytes\":[");
    const char *separator = "";
    for (int byte = 0; byte <= 0xff; byte++) {
        int rune = (uint16_t)(int8_t)byte;
        unsigned long space = rune <= 0x7f
            ? _DefaultRuneLocale.__runetype[rune] & _CTYPE_S
            : (unsigned long)__maskrune(rune, _CTYPE_S);
        if (space) {
            printf("%s%d", separator, byte);
            separator = ",";
        }
    }
    puts("]}");
}

static void decimal_integer(const char *input) {
    errno = 0;
    long long value = atoll(input);
    int error = errno;
    printf("{\"function\":\"atoll\",\"input\":\"%s\",\"value\":%lld,\"errno\":%d}\n",
           input, value, error);
}

int main(void) {
    _Static_assert(sizeof(int) == 4 && sizeof(long long) == 8 && sizeof(float) == 4,
                   "The probe requires the game's scalar widths");
    if (!setlocale(LC_ALL, "C")) {
        return 2;
    }
    char os_build[64];
    size_t size = sizeof(os_build);
    if (sysctlbyname("kern.osversion", os_build, &size, NULL, 0)) {
        return 2;
    }
    printf("{\"os_build\":\"%s\",\"locale\":\"C\",\"architecture\":\"arm64\"}\n", os_build);
    library_identity("sscanf");
    library_identity("atoll");
    library_identity("__maskrune");

    const char *signed32[] = {
        "-2147483649", "-2147483648", "-2147483647",
        "2147483646", "2147483647", "2147483648",
        "-9223372036854775809", "9223372036854775808"
    };
    const char *unsigned32[] = {
        "-1", "0", "1", "4294967294", "4294967295", "4294967296",
        "18446744073709551615", "18446744073709551616"
    };
    const char *signed64[] = {
        "-9223372036854775809", "-9223372036854775808", "-9223372036854775807",
        "9223372036854775806", "9223372036854775807", "9223372036854775808",
        "18446744073709551615", "18446744073709551616"
    };
    const char *unsigned64[] = {
        "-18446744073709551616", "-18446744073709551615", "-1", "0", "1",
        "18446744073709551614", "18446744073709551615", "18446744073709551616"
    };
    const char *float32[] = {
        "-340282346638528859811704183484516925440",
        "-340282326356119256160033759537265639424",
        "-340282366920938463463374607431768211456",
        "340282326356119256160033759537265639424",
        "340282346638528859811704183484516925440",
        "340282366920938463463374607431768211456",
        "1.1754943508222875e-38", "1.1754942106924411e-38",
        "1.401298464324817e-45", "7.006492321624085e-46", "0", "-0",
        "1e999", "-1e999", "inf", "-inf", "nan"
    };
    const char *lexical[] = {
        "12tail", "12 34", " 12", "+7", "-7", "0x10", "010", "1e2",
        "not_a_number", "", "1.25tail"
    };
    CASES("%i", SIGNED32, signed32);
    CASES("%d", SIGNED32, signed32);
    CASES("%u", UNSIGNED32, unsigned32);
    CASES("%lli", SIGNED64, signed64);
    CASES("%lld", SIGNED64, signed64);
    CASES("%llu", UNSIGNED64, unsigned64);
    CASES("%f", FLOAT32, float32);
    const char *narrow[] = {
        "-129", "-128", "-127", "126", "127", "128", "254", "255", "256",
        "-32769", "-32768", "-32767", "32766", "32767", "32768",
        "65534", "65535", "65536"
    };
    CASES("%d", SIGNED32, narrow);
    CASES("%u", UNSIGNED32, narrow);
    const struct { const char *format; enum storage storage; } formats[] = {
        {"%i", SIGNED32}, {"%d", SIGNED32}, {"%u", UNSIGNED32},
        {"%lli", SIGNED64}, {"%lld", SIGNED64}, {"%llu", UNSIGNED64},
        {"%f", FLOAT32}, {"%lld%lf", COMPONENTS}
    };
    for (size_t index = 0; index < sizeof(formats) / sizeof(*formats); index++) {
        CASES(formats[index].format, formats[index].storage, lexical);
    }
    CASES("%lld%lf", COMPONENTS, signed64);
    const char *components[] = {
        "0 1.7976931348623155e308", "0 1.7976931348623157e308", "0 1.7976931348623159e308",
        "0 -1.7976931348623155e308", "0 -1.7976931348623157e308", "0 -1.7976931348623159e308",
        "0 2.2250738585072014e-308", "0 2.225073858507201e-308",
        "0 4.9406564584124654e-324", "0 2.4703282292062327e-324",
        "0.1e309", "0.1e310", "0.1e-999", "-1.25", "281474976710656.0"
    };
    CASES("%lld%lf", COMPONENTS, components);
    for (size_t index = 0; index < sizeof(signed64) / sizeof(*signed64); index++) {
        decimal_integer(signed64[index]);
    }
    for (size_t index = 0; index < sizeof(lexical) / sizeof(*lexical); index++) {
        decimal_integer(lexical[index]);
    }
    lexer_spaces();
    return 0;
}
