<?php

namespace Snmp {
    enum Mib
    {
        case AllowUnderscores;
        case CommentTerm;
        case Replace;
    }

    enum OidOutput
    {
        case Suffix;
        case Module;
        case Full;
        case Numeric;
        case Ucd;
        case None;
    }

    enum Output
    {
        case NumericIndex;
        case EnumPrint;
        case EscapeQuotes;
        case QuickPrint;
        case NumericTimeticks;
        case HexText;
        case DontPrintUnits;
        case ExtendedIndex;
    }

    enum StringOutput
    {
        case Guess;
        case Ascii;
        case Hex;
    }
}
