#!/usr/bin/env python3
"""Verify the dynamic Clang parser required by the libpq-sys bindgen producer."""

import ctypes as c
from ctypes.util import find_library


def main():
    """Parse the actual PostgreSQL public header, including its standard C headers."""
    library = find_library('clang-18')
    if not library:
        raise RuntimeError('The CI producer image is missing libclang-18')
    clang = c.CDLL(library)
    clang.clang_createIndex.argtypes = [c.c_int, c.c_int]
    clang.clang_createIndex.restype = c.c_void_p
    clang.clang_parseTranslationUnit.argtypes = [c.c_void_p, c.c_char_p, c.POINTER(c.c_char_p),
                                               c.c_int, c.c_void_p, c.c_uint, c.c_uint]
    clang.clang_parseTranslationUnit.restype = c.c_void_p
    clang.clang_getNumDiagnostics.argtypes = [c.c_void_p]
    clang.clang_getNumDiagnostics.restype = c.c_uint
    clang.clang_getDiagnostic.argtypes = [c.c_void_p, c.c_uint]
    clang.clang_getDiagnostic.restype = c.c_void_p
    clang.clang_getDiagnosticSeverity.argtypes = [c.c_void_p]
    clang.clang_getDiagnosticSeverity.restype = c.c_uint
    for name in ('clang_disposeDiagnostic', 'clang_disposeTranslationUnit', 'clang_disposeIndex'):
        getattr(clang, name).argtypes = [c.c_void_p]
    index = clang.clang_createIndex(0, 0)
    unit = None
    try:
        arguments = (c.c_char_p * 3)(b'-x', b'c', b'-I/usr/include/postgresql')
        unit = clang.clang_parseTranslationUnit(index, b'/usr/include/postgresql/libpq-fe.h',
                                               arguments, len(arguments), None, 0, 0)
        if not unit:
            raise RuntimeError('libclang could not parse the PostgreSQL public header')
        for number in range(clang.clang_getNumDiagnostics(unit)):
            diagnostic = clang.clang_getDiagnostic(unit, number)
            try:
                if clang.clang_getDiagnosticSeverity(diagnostic) >= 3:
                    raise RuntimeError('libclang reported a PostgreSQL header or C include error')
            finally:
                clang.clang_disposeDiagnostic(diagnostic)
    finally:
        if unit:
            clang.clang_disposeTranslationUnit(unit)
        clang.clang_disposeIndex(index)
    print('libclang parsed the PostgreSQL and standard C headers successfully')


if __name__ == '__main__':
    main()
