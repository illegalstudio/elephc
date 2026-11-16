//! Purpose:
//! Records which internal method return types PHP Reflection labels as tentative.
//!
//! Called from:
//! - The callable dump renderer, using the method's declaring class.
//!
//! Key details:
//! - Measured from PHP 8.5.11 for the classes in the shared class catalog.
//! - Inherited methods are included because injected classes can flatten PHP's hierarchy.
//! - Constructors and methods with enforced return types are deliberately absent.

/// Returns whether an internal method has a tentative PHP return type.
pub fn has_tentative_return(class: &str, method: &str) -> bool {
    let methods = match class.trim_start_matches('\\').to_ascii_lowercase().as_str() {
        "appenditerator" => "append rewind valid current next getiteratorindex getarrayiterator getinneriterator key",
        "argumentcounterror" => "__wakeup",
        "arithmeticerror" => "__wakeup",
        "arrayaccess" => "offsetexists offsetget offsetset offsetunset",
        "arrayiterator" => "offsetexists offsetget offsetset offsetunset append getarraycopy count getflags setflags asort ksort \
            uasort uksort natsort natcasesort unserialize serialize __serialize __unserialize rewind current key \
            next valid seek __debuginfo",
        "arrayobject" => "offsetexists offsetget offsetset offsetunset append getarraycopy count getflags setflags asort ksort \
            uasort uksort natsort natcasesort unserialize serialize __serialize __unserialize getiterator \
            exchangearray setiteratorclass getiteratorclass __debuginfo",
        "assertionerror" => "__wakeup",
        "badfunctioncallexception" => "__wakeup",
        "badmethodcallexception" => "__wakeup",
        "cachingiterator" => "rewind valid next hasnext getflags setflags offsetget offsetset offsetunset offsetexists getcache \
            count getinneriterator key current",
        "callbackfilteriterator" => "accept rewind next getinneriterator valid key current",
        "closedgeneratorexception" => "__wakeup",
        "compileerror" => "__wakeup",
        "countable" => "count",
        "curlfile" => "getfilename getmimetype getpostfilename setmimetype setpostfilename",
        "dateerror" => "__wakeup",
        "dateexception" => "__wakeup",
        "dateinterval" => "createfromdatestring format __wakeup __set_state",
        "dateinvalidoperationexception" => "__wakeup",
        "dateinvalidtimezoneexception" => "__wakeup",
        "datemalformedintervalstringexception" => "__wakeup",
        "datemalformedperiodstringexception" => "__wakeup",
        "datemalformedstringexception" => "__wakeup",
        "dateobjecterror" => "__wakeup",
        "dateperiod" => "getstartdate getenddate getdateinterval getrecurrences __wakeup __set_state",
        "daterangeerror" => "__wakeup",
        "datetime" => "__wakeup __set_state createfromimmutable createfromformat createfromtimestamp getlasterrors format \
            modify add sub gettimezone settimezone getoffset settime setdate setisodate settimestamp \
            gettimestamp diff",
        "datetimeimmutable" => "__wakeup __set_state createfromformat createfromtimestamp getlasterrors format gettimezone getoffset \
            gettimestamp diff modify add sub settimezone settime setdate setisodate settimestamp \
            createfrommutable",
        "datetimeinterface" => "format gettimezone getoffset gettimestamp diff __wakeup",
        "datetimezone" => "getname getoffset gettransitions getlocation listabbreviations listidentifiers __wakeup __set_state",
        "directoryiterator" => "getfilename getextension getbasename isdot rewind valid key current next seek getpath getpathname \
            getperms getinode getsize getowner getgroup getatime getmtime getctime gettype iswritable isreadable \
            isexecutable isfile isdir islink getlinktarget getrealpath getfileinfo getpathinfo openfile \
            setfileclass setinfoclass __debuginfo",
        "divisionbyzeroerror" => "__wakeup",
        "domainexception" => "__wakeup",
        "domcharacterdata" => "appenddata insertdata deletedata replacedata c14n c14nfile getlineno getnodepath hasattributes \
            haschildnodes isdefaultnamespace issamenode issupported lookupnamespaceuri lookupprefix normalize",
        "domcomment" => "appenddata insertdata deletedata replacedata c14n c14nfile getlineno getnodepath hasattributes \
            haschildnodes isdefaultnamespace issamenode issupported lookupnamespaceuri lookupprefix normalize",
        "domdocument" => "createcomment createdocumentfragment createtextnode getelementbyid getelementsbytagname \
            getelementsbytagnamens load loadxml normalizedocument registernodeclass save loadhtml loadhtmlfile \
            savehtml savehtmlfile savexml schemavalidate schemavalidatesource relaxngvalidate \
            relaxngvalidatesource validate xinclude adoptnode c14n c14nfile getlineno getnodepath hasattributes \
            haschildnodes isdefaultnamespace issamenode issupported lookupnamespaceuri lookupprefix normalize",
        "domelement" => "getattribute getattributens getelementsbytagname getelementsbytagnamens hasattribute hasattributens \
            removeattribute removeattributens setattributens setidattribute setidattributens setidattributenode \
            c14n c14nfile getlineno getnodepath hasattributes haschildnodes isdefaultnamespace issamenode \
            issupported lookupnamespaceuri lookupprefix normalize",
        "domnode" => "c14n c14nfile getlineno getnodepath hasattributes haschildnodes isdefaultnamespace issamenode \
            issupported lookupnamespaceuri lookupprefix normalize",
        "domnodelist" => "count",
        "domprocessinginstruction" => "c14n c14nfile getlineno getnodepath hasattributes haschildnodes isdefaultnamespace issamenode \
            issupported lookupnamespaceuri lookupprefix normalize",
        "domtext" => "iswhitespaceinelementcontent iselementcontentwhitespace appenddata insertdata deletedata replacedata \
            c14n c14nfile getlineno getnodepath hasattributes haschildnodes isdefaultnamespace issamenode \
            issupported lookupnamespaceuri lookupprefix normalize",
        "emptyiterator" => "current next key valid rewind",
        "error" => "__wakeup",
        "errorexception" => "__wakeup",
        "exception" => "__wakeup",
        "fibererror" => "__wakeup",
        "filesystemiterator" => "rewind key current getflags setflags getfilename getextension getbasename isdot valid next seek \
            getpath getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime gettype \
            iswritable isreadable isexecutable isfile isdir islink getlinktarget getrealpath getfileinfo \
            getpathinfo openfile setfileclass setinfoclass __debuginfo",
        "filteriterator" => "accept rewind next getinneriterator valid key current",
        "globiterator" => "count rewind key current getflags setflags getfilename getextension getbasename isdot valid next \
            seek getpath getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime \
            gettype iswritable isreadable isexecutable isfile isdir islink getlinktarget getrealpath getfileinfo \
            getpathinfo openfile setfileclass setinfoclass __debuginfo",
        "infiniteiterator" => "next getinneriterator rewind valid key current",
        "invalidargumentexception" => "__wakeup",
        "iterator" => "current next key valid rewind",
        "iteratoraggregate" => "getiterator",
        "iteratoriterator" => "getinneriterator rewind valid key current next",
        "jsonexception" => "__wakeup",
        "jsonserializable" => "jsonserialize",
        "lengthexception" => "__wakeup",
        "limititerator" => "rewind valid next seek getposition getinneriterator key current",
        "logicexception" => "__wakeup",
        "multipleiterator" => "getflags setflags attachiterator detachiterator containsiterator countiterators rewind valid key \
            current next __debuginfo",
        "mysqli" => "autocommit begin_transaction change_user character_set_name close commit connect dump_debug_info \
            debug get_charset get_client_info get_connection_stats get_server_info get_warnings kill multi_query \
            more_results next_result ping poll prepare query real_connect real_escape_string reap_async_query \
            escape_string real_query release_savepoint rollback savepoint select_db set_charset options set_opt \
            ssl_set stat stmt_init store_result thread_safe use_result refresh",
        "mysqli_result" => "close free data_seek fetch_field fetch_fields fetch_field_direct fetch_all fetch_array fetch_assoc \
            fetch_object fetch_row field_seek free_result",
        "mysqli_sql_exception" => "__wakeup",
        "mysqli_stmt" => "attr_get attr_set bind_param bind_result close data_seek execute fetch get_warnings result_metadata \
            more_results next_result num_rows send_long_data free_result reset prepare store_result get_result",
        "norewinditerator" => "rewind valid key current next getinneriterator",
        "outeriterator" => "getinneriterator current next key valid rewind",
        "outofboundsexception" => "__wakeup",
        "outofrangeexception" => "__wakeup",
        "overflowexception" => "__wakeup",
        "parentiterator" => "accept haschildren getchildren rewind next getinneriterator valid key current",
        "parseerror" => "__wakeup",
        "pdo" => "begintransaction commit errorcode errorinfo exec getattribute getavailabledrivers intransaction \
            lastinsertid prepare query quote rollback setattribute",
        "pdo\\mysql" => "begintransaction commit errorcode errorinfo exec getattribute getavailabledrivers intransaction \
            lastinsertid prepare query quote rollback setattribute",
        "pdo\\pgsql" => "begintransaction commit errorcode errorinfo exec getattribute getavailabledrivers intransaction \
            lastinsertid prepare query quote rollback setattribute",
        "pdo\\sqlite" => "begintransaction commit errorcode errorinfo exec getattribute getavailabledrivers intransaction \
            lastinsertid prepare query quote rollback setattribute",
        "pdoexception" => "__wakeup",
        "pdostatement" => "bindcolumn bindparam bindvalue closecursor columncount debugdumpparams errorcode errorinfo execute \
            fetch fetchall fetchcolumn fetchobject getattribute getcolumnmeta nextrowset rowcount setattribute \
            setfetchmode",
        "phar" => "addemptydir addfile addfromstring buildfromdirectory buildfromiterator compressfiles decompressfiles \
            compress decompress converttoexecutable converttodata copy count delete delmetadata extractto \
            getalias getpath getmetadata getmodified getsignature getstub getversion hasmetadata isbuffering \
            iscompressed isfileformat iswritable offsetexists offsetget offsetset offsetunset setalias \
            setdefaultstub setmetadata setsignaturealgorithm setstub startbuffering stopbuffering haschildren \
            getchildren getsubpath getsubpathname rewind key current getflags setflags getfilename getextension \
            getbasename isdot valid next seek getpathname getperms getinode getsize getowner getgroup getatime \
            getmtime getctime gettype isreadable isexecutable isfile isdir islink getlinktarget getrealpath \
            getfileinfo getpathinfo openfile setfileclass setinfoclass __debuginfo",
        "phardata" => "addemptydir addfile addfromstring buildfromdirectory buildfromiterator compressfiles decompressfiles \
            compress decompress converttoexecutable converttodata copy count delete delmetadata extractto \
            getalias getpath getmetadata getmodified getsignature getstub getversion hasmetadata isbuffering \
            iscompressed isfileformat iswritable offsetexists offsetget offsetset offsetunset setalias \
            setdefaultstub setmetadata setsignaturealgorithm startbuffering stopbuffering haschildren \
            getchildren getsubpath getsubpathname rewind key current getflags setflags getfilename getextension \
            getbasename isdot valid next seek getpathname getperms getinode getsize getowner getgroup getatime \
            getmtime getctime gettype isreadable isexecutable isfile isdir islink getlinktarget getrealpath \
            getfileinfo getpathinfo openfile setfileclass setinfoclass __debuginfo",
        "finfo" => "__construct file buffer set_flags",
        "pharfileinfo" => "chmod compress decompress delmetadata getcompressedsize getcrc32 getcontent getmetadata getpharflags \
            hasmetadata iscompressed iscrcchecked setmetadata getpath getfilename getextension getbasename \
            getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime gettype \
            iswritable isreadable isexecutable isfile isdir islink getlinktarget getrealpath getfileinfo \
            getpathinfo openfile setfileclass setinfoclass __debuginfo",
        "php_user_filter" => "filter oncreate onclose",
        "rangeexception" => "__wakeup",
        "recursivearrayiterator" => "haschildren getchildren offsetexists offsetget offsetset offsetunset append getarraycopy count \
            getflags setflags asort ksort uasort uksort natsort natcasesort unserialize serialize __serialize \
            __unserialize rewind current key next valid seek __debuginfo",
        "recursivecachingiterator" => "haschildren getchildren rewind valid next hasnext getflags setflags offsetget offsetset offsetunset \
            offsetexists getcache count getinneriterator key current",
        "recursivecallbackfilteriterator" => "haschildren getchildren accept rewind next getinneriterator valid key current",
        "recursivedirectoryiterator" => "haschildren getchildren getsubpath getsubpathname rewind key current getflags setflags getfilename \
            getextension getbasename isdot valid next seek getpath getpathname getperms getinode getsize \
            getowner getgroup getatime getmtime getctime gettype iswritable isreadable isexecutable isfile isdir \
            islink getlinktarget getrealpath getfileinfo getpathinfo openfile setfileclass setinfoclass \
            __debuginfo",
        "recursivefilteriterator" => "haschildren getchildren accept rewind next getinneriterator valid key current",
        "recursiveiterator" => "haschildren getchildren current next key valid rewind",
        "recursiveiteratoriterator" => "rewind valid key current next getdepth getsubiterator getinneriterator beginiteration enditeration \
            callhaschildren callgetchildren beginchildren endchildren nextelement setmaxdepth getmaxdepth",
        "recursiveregexiterator" => "accept haschildren getchildren getmode setmode getflags setflags getregex getpregflags setpregflags \
            rewind next getinneriterator valid key current",
        "reflectionclass" => "getname isinternal isuserdefined isanonymous isinstantiable iscloneable getfilename getstartline \
            getendline getdoccomment getconstructor hasmethod getmethod getmethods hasproperty getproperty \
            getproperties hasconstant getconstants getreflectionconstants getconstant getreflectionconstant \
            getinterfaces getinterfacenames isinterface gettraits gettraitnames gettraitaliases istrait \
            isabstract isfinal getmodifiers isinstance newinstance newinstancewithoutconstructor newinstanceargs \
            getparentclass issubclassof getstaticproperties getstaticpropertyvalue setstaticpropertyvalue \
            getdefaultproperties isiterable isiterateable implementsinterface getextension getextensionname \
            innamespace getnamespacename getshortname",
        "reflectionclassconstant" => "getname getvalue ispublic isprivate isprotected getmodifiers getdeclaringclass getdoccomment",
        "reflectionenum" => "getname isinternal isuserdefined isanonymous isinstantiable iscloneable getfilename getstartline \
            getendline getdoccomment getconstructor hasmethod getmethod getmethods hasproperty getproperty \
            getproperties hasconstant getconstants getreflectionconstants getconstant getreflectionconstant \
            getinterfaces getinterfacenames isinterface gettraits gettraitnames gettraitaliases istrait \
            isabstract isfinal getmodifiers isinstance newinstance newinstancewithoutconstructor newinstanceargs \
            getparentclass issubclassof getstaticproperties getstaticpropertyvalue setstaticpropertyvalue \
            getdefaultproperties isiterable isiterateable implementsinterface getextension getextensionname \
            innamespace getnamespacename getshortname",
        "reflectionenumbackedcase" => "getname ispublic isprivate isprotected getmodifiers getdeclaringclass getdoccomment",
        "reflectionenumunitcase" => "getname ispublic isprivate isprotected getmodifiers getdeclaringclass getdoccomment",
        "reflectionexception" => "__wakeup",
        "reflectionfunction" => "isdisabled invoke invokeargs getclosure innamespace isclosure isdeprecated isinternal isuserdefined \
            isgenerator isvariadic isstatic getclosurethis getclosurescopeclass getclosurecalledclass \
            getdoccomment getendline getextension getextensionname getfilename getname getnamespacename \
            getnumberofparameters getnumberofrequiredparameters getparameters getshortname getstartline \
            getstaticvariables returnsreference hasreturntype getreturntype",
        "reflectionintersectiontype" => "allowsnull",
        "reflectionmethod" => "ispublic isprivate isprotected isabstract isfinal isconstructor isdestructor getclosure getmodifiers \
            invoke invokeargs getdeclaringclass getprototype setaccessible innamespace isclosure isdeprecated \
            isinternal isuserdefined isgenerator isvariadic isstatic getclosurethis getclosurescopeclass \
            getclosurecalledclass getdoccomment getendline getextension getextensionname getfilename getname \
            getnamespacename getnumberofparameters getnumberofrequiredparameters getparameters getshortname \
            getstartline getstaticvariables returnsreference hasreturntype getreturntype",
        "reflectionnamedtype" => "getname isbuiltin allowsnull",
        "reflectionobject" => "getname isinternal isuserdefined isanonymous isinstantiable iscloneable getfilename getstartline \
            getendline getdoccomment getconstructor hasmethod getmethod getmethods hasproperty getproperty \
            getproperties hasconstant getconstants getreflectionconstants getconstant getreflectionconstant \
            getinterfaces getinterfacenames isinterface gettraits gettraitnames gettraitaliases istrait \
            isabstract isfinal getmodifiers isinstance newinstance newinstancewithoutconstructor newinstanceargs \
            getparentclass issubclassof getstaticproperties getstaticpropertyvalue setstaticpropertyvalue \
            getdefaultproperties isiterable isiterateable implementsinterface getextension getextensionname \
            innamespace getnamespacename getshortname",
        "reflectionparameter" => "getname ispassedbyreference canbepassedbyvalue getdeclaringfunction getdeclaringclass getclass \
            hastype gettype isarray iscallable allowsnull getposition isoptional isdefaultvalueavailable \
            getdefaultvalue isdefaultvalueconstant getdefaultvalueconstantname isvariadic",
        "reflectionproperty" => "getname getvalue setvalue isinitialized ispublic isprivate isprotected isstatic isdefault \
            getmodifiers getdeclaringclass getdoccomment setaccessible gettype hastype getdefaultvalue",
        "reflectionuniontype" => "allowsnull",
        "regexiterator" => "accept getmode setmode getflags setflags getregex getpregflags setpregflags rewind next \
            getinneriterator valid key current",
        "requestparsebodyexception" => "__wakeup",
        "runtimeexception" => "__wakeup",
        "seekableiterator" => "seek current next key valid rewind",
        "sessionhandler" => "open close read write destroy gc create_sid",
        "sessionhandlerinterface" => "open close read write destroy gc",
        "sessionidinterface" => "create_sid",
        "sessionupdatetimestamphandlerinterface" => "validateid updatetimestamp",
        "spldoublylinkedlist" => "add pop shift push unshift top bottom __debuginfo count isempty setiteratormode getiteratormode \
            offsetexists offsetget offsetset offsetunset rewind current key prev next valid unserialize \
            serialize __serialize __unserialize",
        "splfileinfo" => "getpath getfilename getextension getbasename getpathname getperms getinode getsize getowner getgroup \
            getatime getmtime getctime gettype iswritable isreadable isexecutable isfile isdir islink \
            getlinktarget getrealpath getfileinfo getpathinfo openfile setfileclass setinfoclass __debuginfo",
        "splfileobject" => "rewind eof valid fgets fread fgetcsv fputcsv setcsvcontrol getcsvcontrol flock fflush ftell fseek \
            fgetc fpassthru fscanf fwrite fstat ftruncate current key next setflags getflags setmaxlinelen \
            getmaxlinelen haschildren getchildren seek getcurrentline getpath getfilename getextension \
            getbasename getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime \
            gettype iswritable isreadable isexecutable isfile isdir islink getlinktarget getrealpath getfileinfo \
            getpathinfo openfile setfileclass setinfoclass __debuginfo",
        "splfixedarray" => "__wakeup count toarray fromarray getsize setsize offsetexists offsetget offsetset offsetunset",
        "splheap" => "extract insert top count isempty rewind current key next valid recoverfromcorruption compare \
            iscorrupted __debuginfo __serialize __unserialize",
        "splmaxheap" => "compare extract insert top count isempty rewind current key next valid recoverfromcorruption \
            iscorrupted __debuginfo __serialize __unserialize",
        "splminheap" => "compare extract insert top count isempty rewind current key next valid recoverfromcorruption \
            iscorrupted __debuginfo __serialize __unserialize",
        "splobjectstorage" => "attach detach contains addall removeall removeallexcept getinfo setinfo count rewind valid key \
            current next unserialize serialize offsetexists offsetget offsetset offsetunset gethash __serialize \
            __unserialize __debuginfo",
        "splobserver" => "update",
        "splpriorityqueue" => "compare insert setextractflags top extract count isempty rewind current key next valid \
            recoverfromcorruption iscorrupted getextractflags __debuginfo __serialize __unserialize",
        "splqueue" => "enqueue dequeue add pop shift push unshift top bottom __debuginfo count isempty setiteratormode \
            getiteratormode offsetexists offsetget offsetset offsetunset rewind current key prev next valid \
            unserialize serialize __serialize __unserialize",
        "splstack" => "add pop shift push unshift top bottom __debuginfo count isempty setiteratormode getiteratormode \
            offsetexists offsetget offsetset offsetunset rewind current key prev next valid unserialize \
            serialize __serialize __unserialize",
        "splsubject" => "attach detach notify",
        "spltempfileobject" => "rewind eof valid fgets fread fgetcsv fputcsv setcsvcontrol getcsvcontrol flock fflush ftell fseek \
            fgetc fpassthru fscanf fwrite fstat ftruncate current key next setflags getflags setmaxlinelen \
            getmaxlinelen haschildren getchildren seek getcurrentline getpath getfilename getextension \
            getbasename getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime \
            gettype iswritable isreadable isexecutable isfile isdir islink getlinktarget getrealpath getfileinfo \
            getpathinfo openfile setfileclass setinfoclass __debuginfo",
        "typeerror" => "__wakeup",
        "underflowexception" => "__wakeup",
        "unexpectedvalueexception" => "__wakeup",
        "unhandledmatcherror" => "__wakeup",
        "valueerror" => "__wakeup",
        "xmlwriter" => "openuri openmemory setindent setindentstring startcomment endcomment startattribute endattribute \
            writeattribute startattributens writeattributens startelement endelement fullendelement \
            startelementns writeelement writeelementns startpi endpi writepi startcdata endcdata writecdata text \
            writeraw startdocument enddocument writecomment startdtd enddtd writedtd startdtdelement \
            enddtdelement writedtdelement startdtdattlist enddtdattlist writedtdattlist startdtdentity \
            enddtdentity writedtdentity outputmemory flush",
        _ => return false,
    };
    methods.split_ascii_whitespace().any(|name| name.eq_ignore_ascii_case(method))
}

/// Tentative declarations are case insensitive and exclude enforced types and constructors.
#[test]
fn reflection_tentative_method_catalog() {
    assert!(has_tentative_return("\\Iterator", "CURRENT"));
    assert!(has_tentative_return("DateTime", "format"));
    assert!(has_tentative_return("ReflectionFunction", "getName"));
    assert!(has_tentative_return("Pdo\\Sqlite", "errorCode"));
    assert!(!has_tentative_return("Iterator", "__construct"));
    assert!(!has_tentative_return("ReflectionMethod", "__toString"));
    assert!(!has_tentative_return("UserClass", "current"));
}
