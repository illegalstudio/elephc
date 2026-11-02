//! Purpose:
//! Preserves PHP declaring-class names for internal method dumps when injected classes flatten them.
//!
//! Called from:
//! - The callable dump renderer when choosing inherited-method headers.
//!
//! Key details:
//! - Measured with PHP 8.5.11 for the shared class catalog, including inherited abstract owners.
//! - These names are display metadata and do not allocate unsupported ReflectionClass objects.

/// Returns the PHP declaring class when an internal method is inherited from another owner.
pub fn inherited_declaring_class(class: &str, method: &str) -> Option<&'static str> {
    let groups: &[(&str, &str)] = match class.trim_start_matches('\\').to_ascii_lowercase().as_str() {
        "appenditerator" => &[
            ("IteratorIterator", "getinneriterator key"),
        ],
        "argumentcounterror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "arithmeticerror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "assertionerror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "backedenum" => &[
            ("UnitEnum", "cases"),
        ],
        "badfunctioncallexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "badmethodcallexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "cachingiterator" => &[
            ("IteratorIterator", "getinneriterator key current"),
        ],
        "callbackfilteriterator" => &[
            ("FilterIterator", "rewind next"),
            ("IteratorIterator", "getinneriterator valid key current"),
        ],
        "closedgeneratorexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "compileerror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "dateerror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "dateexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "dateinvalidoperationexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "dateinvalidtimezoneexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "datemalformedintervalstringexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "datemalformedperiodstringexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "datemalformedstringexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "dateobjecterror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "daterangeerror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "directoryiterator" => &[
            ("SplFileInfo", "getpath getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime \
                gettype iswritable isreadable isexecutable isfile isdir islink getlinktarget getrealpath \
                getfileinfo getpathinfo openfile setfileclass setinfoclass __debuginfo _bad_state_ex"),
        ],
        "divisionbyzeroerror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "domainexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "domcharacterdata" => &[
            ("DOMNode", "appendchild c14n c14nfile clonenode getlineno getnodepath hasattributes haschildnodes \
                insertbefore isdefaultnamespace issamenode isequalnode issupported lookupnamespaceuri \
                lookupprefix normalize removechild replacechild contains getrootnode comparedocumentposition \
                __sleep __wakeup"),
        ],
        "domcomment" => &[
            ("DOMCharacterData", "appenddata substringdata insertdata deletedata replacedata replacewith remove before after"),
            ("DOMNode", "appendchild c14n c14nfile clonenode getlineno getnodepath hasattributes haschildnodes \
                insertbefore isdefaultnamespace issamenode isequalnode issupported lookupnamespaceuri \
                lookupprefix normalize removechild replacechild contains getrootnode comparedocumentposition \
                __sleep __wakeup"),
        ],
        "domdocument" => &[
            ("DOMNode", "appendchild c14n c14nfile clonenode getlineno getnodepath hasattributes haschildnodes \
                insertbefore isdefaultnamespace issamenode isequalnode issupported lookupnamespaceuri \
                lookupprefix normalize removechild replacechild contains getrootnode comparedocumentposition \
                __sleep __wakeup"),
        ],
        "domelement" => &[
            ("DOMNode", "appendchild c14n c14nfile clonenode getlineno getnodepath hasattributes haschildnodes \
                insertbefore isdefaultnamespace issamenode isequalnode issupported lookupnamespaceuri \
                lookupprefix normalize removechild replacechild contains getrootnode comparedocumentposition \
                __sleep __wakeup"),
        ],
        "domprocessinginstruction" => &[
            ("DOMNode", "appendchild c14n c14nfile clonenode getlineno getnodepath hasattributes haschildnodes \
                insertbefore isdefaultnamespace issamenode isequalnode issupported lookupnamespaceuri \
                lookupprefix normalize removechild replacechild contains getrootnode comparedocumentposition \
                __sleep __wakeup"),
        ],
        "domtext" => &[
            ("DOMCharacterData", "appenddata substringdata insertdata deletedata replacedata replacewith remove before after"),
            ("DOMNode", "appendchild c14n c14nfile clonenode getlineno getnodepath hasattributes haschildnodes \
                insertbefore isdefaultnamespace issamenode isequalnode issupported lookupnamespaceuri \
                lookupprefix normalize removechild replacechild contains getrootnode comparedocumentposition \
                __sleep __wakeup"),
        ],
        "errorexception" => &[
            ("Exception", "__wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring __tostring"),
        ],
        "fibererror" => &[
            ("Error", "__wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring __tostring"),
        ],
        "filesystemiterator" => &[
            ("DirectoryIterator", "getfilename getextension getbasename isdot valid next seek __tostring"),
            ("SplFileInfo", "getpath getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime \
                gettype iswritable isreadable isexecutable isfile isdir islink getlinktarget getrealpath \
                getfileinfo getpathinfo openfile setfileclass setinfoclass __debuginfo _bad_state_ex"),
        ],
        "filteriterator" => &[
            ("IteratorIterator", "getinneriterator valid key current"),
        ],
        "globiterator" => &[
            ("DirectoryIterator", "getfilename getextension getbasename isdot valid next seek __tostring"),
            ("FilesystemIterator", "rewind key current getflags setflags"),
            ("SplFileInfo", "getpath getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime \
                gettype iswritable isreadable isexecutable isfile isdir islink getlinktarget getrealpath \
                getfileinfo getpathinfo openfile setfileclass setinfoclass __debuginfo _bad_state_ex"),
        ],
        "infiniteiterator" => &[
            ("IteratorIterator", "getinneriterator rewind valid key current"),
        ],
        "invalidargumentexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "jsonexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "lengthexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "limititerator" => &[
            ("IteratorIterator", "getinneriterator key current"),
        ],
        "logicexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "mysqli_sql_exception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "norewinditerator" => &[
            ("IteratorIterator", "getinneriterator"),
        ],
        "outeriterator" => &[
            ("Iterator", "current next key valid rewind"),
        ],
        "outofboundsexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "outofrangeexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "overflowexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "parentiterator" => &[
            ("FilterIterator", "rewind next"),
            ("IteratorIterator", "getinneriterator valid key current"),
            ("RecursiveFilterIterator", "haschildren getchildren"),
        ],
        "parseerror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "pdo\\mysql" => &[
            ("PDO", "__construct connect begintransaction commit errorcode errorinfo exec getattribute \
                getavailabledrivers intransaction lastinsertid prepare query quote rollback setattribute"),
        ],
        "pdo\\pgsql" => &[
            ("PDO", "__construct connect begintransaction commit errorcode errorinfo exec getattribute \
                getavailabledrivers intransaction lastinsertid prepare query quote rollback setattribute"),
        ],
        "pdo\\sqlite" => &[
            ("PDO", "__construct connect begintransaction commit errorcode errorinfo exec getattribute \
                getavailabledrivers intransaction lastinsertid prepare query quote rollback setattribute"),
        ],
        "pdoexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "phar" => &[
            ("DirectoryIterator", "getfilename getextension getbasename isdot valid next seek __tostring"),
            ("FilesystemIterator", "rewind key current getflags setflags"),
            ("RecursiveDirectoryIterator", "haschildren getchildren getsubpath getsubpathname"),
            ("SplFileInfo", "getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime gettype \
                isreadable isexecutable isfile isdir islink getlinktarget getrealpath getfileinfo getpathinfo \
                openfile setfileclass setinfoclass __debuginfo _bad_state_ex"),
        ],
        "phardata" => &[
            ("DirectoryIterator", "getfilename getextension getbasename isdot valid next seek __tostring"),
            ("FilesystemIterator", "rewind key current getflags setflags"),
            ("RecursiveDirectoryIterator", "haschildren getchildren getsubpath getsubpathname"),
            ("SplFileInfo", "getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime gettype \
                isreadable isexecutable isfile isdir islink getlinktarget getrealpath getfileinfo getpathinfo \
                openfile setfileclass setinfoclass __debuginfo _bad_state_ex"),
        ],
        "pharfileinfo" => &[
            ("SplFileInfo", "getpath getfilename getextension getbasename getpathname getperms getinode getsize getowner \
                getgroup getatime getmtime getctime gettype iswritable isreadable isexecutable isfile isdir \
                islink getlinktarget getrealpath getfileinfo getpathinfo openfile setfileclass setinfoclass \
                __tostring __debuginfo _bad_state_ex"),
        ],
        "rangeexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "recursivearrayiterator" => &[
            ("ArrayIterator", "__construct offsetexists offsetget offsetset offsetunset append getarraycopy count getflags \
                setflags asort ksort uasort uksort natsort natcasesort unserialize serialize __serialize \
                __unserialize rewind current key next valid seek __debuginfo"),
        ],
        "recursivecachingiterator" => &[
            ("CachingIterator", "rewind valid next hasnext __tostring getflags setflags offsetget offsetset offsetunset \
                offsetexists getcache count"),
            ("IteratorIterator", "getinneriterator key current"),
        ],
        "recursivecallbackfilteriterator" => &[
            ("CallbackFilterIterator", "accept"),
            ("FilterIterator", "rewind next"),
            ("IteratorIterator", "getinneriterator valid key current"),
        ],
        "recursivedirectoryiterator" => &[
            ("DirectoryIterator", "getfilename getextension getbasename isdot valid next seek __tostring"),
            ("FilesystemIterator", "rewind key current getflags setflags"),
            ("SplFileInfo", "getpath getpathname getperms getinode getsize getowner getgroup getatime getmtime getctime \
                gettype iswritable isreadable isexecutable isfile isdir islink getlinktarget getrealpath \
                getfileinfo getpathinfo openfile setfileclass setinfoclass __debuginfo _bad_state_ex"),
        ],
        "recursivefilteriterator" => &[
            ("FilterIterator", "accept rewind next"),
            ("IteratorIterator", "getinneriterator valid key current"),
        ],
        "recursiveiterator" => &[
            ("Iterator", "current next key valid rewind"),
        ],
        "recursiveregexiterator" => &[
            ("FilterIterator", "rewind next"),
            ("IteratorIterator", "getinneriterator valid key current"),
            ("RegexIterator", "getmode setmode getflags setflags getregex getpregflags setpregflags"),
        ],
        "reflectionenum" => &[
            ("ReflectionClass", "__tostring getname isinternal isuserdefined isanonymous isinstantiable iscloneable getfilename \
                getstartline getendline getdoccomment getconstructor hasmethod getmethod getmethods hasproperty \
                getproperty getproperties hasconstant getconstants getreflectionconstants getconstant \
                getreflectionconstant getinterfaces getinterfacenames isinterface gettraits gettraitnames \
                gettraitaliases istrait isenum isabstract isfinal isreadonly getmodifiers isinstance newinstance \
                newinstancewithoutconstructor newinstanceargs newlazyghost newlazyproxy resetaslazyghost \
                resetaslazyproxy initializelazyobject isuninitializedlazyobject marklazyobjectasinitialized \
                getlazyinitializer getparentclass issubclassof getstaticproperties getstaticpropertyvalue \
                setstaticpropertyvalue getdefaultproperties isiterable isiterateable implementsinterface \
                getextension getextensionname innamespace getnamespacename getshortname getattributes"),
        ],
        "reflectionenumbackedcase" => &[
            ("ReflectionClassConstant", "__tostring getname ispublic isprivate isprotected isfinal getmodifiers getdeclaringclass \
                getdoccomment getattributes isenumcase isdeprecated hastype gettype"),
            ("ReflectionEnumUnitCase", "getenum getvalue"),
        ],
        "reflectionenumunitcase" => &[
            ("ReflectionClassConstant", "__tostring getname ispublic isprivate isprotected isfinal getmodifiers getdeclaringclass \
                getdoccomment getattributes isenumcase isdeprecated hastype gettype"),
        ],
        "reflectionexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "reflectionfunction" => &[
            ("ReflectionFunctionAbstract", "innamespace isclosure isdeprecated isinternal isuserdefined isgenerator isvariadic isstatic \
                getclosurethis getclosurescopeclass getclosurecalledclass getclosureusedvariables getdoccomment \
                getendline getextension getextensionname getfilename getname getnamespacename \
                getnumberofparameters getnumberofrequiredparameters getparameters getshortname getstartline \
                getstaticvariables returnsreference hasreturntype getreturntype hastentativereturntype \
                gettentativereturntype getattributes"),
        ],
        "reflectionintersectiontype" => &[
            ("ReflectionType", "allowsnull __tostring"),
        ],
        "reflectionmethod" => &[
            ("ReflectionFunctionAbstract", "innamespace isclosure isdeprecated isinternal isuserdefined isgenerator isvariadic isstatic \
                getclosurethis getclosurescopeclass getclosurecalledclass getclosureusedvariables getdoccomment \
                getendline getextension getextensionname getfilename getname getnamespacename \
                getnumberofparameters getnumberofrequiredparameters getparameters getshortname getstartline \
                getstaticvariables returnsreference hasreturntype getreturntype hastentativereturntype \
                gettentativereturntype getattributes"),
        ],
        "reflectionnamedtype" => &[
            ("ReflectionType", "allowsnull __tostring"),
        ],
        "reflectionobject" => &[
            ("ReflectionClass", "__tostring getname isinternal isuserdefined isanonymous isinstantiable iscloneable getfilename \
                getstartline getendline getdoccomment getconstructor hasmethod getmethod getmethods hasproperty \
                getproperty getproperties hasconstant getconstants getreflectionconstants getconstant \
                getreflectionconstant getinterfaces getinterfacenames isinterface gettraits gettraitnames \
                gettraitaliases istrait isenum isabstract isfinal isreadonly getmodifiers isinstance newinstance \
                newinstancewithoutconstructor newinstanceargs newlazyghost newlazyproxy resetaslazyghost \
                resetaslazyproxy initializelazyobject isuninitializedlazyobject marklazyobjectasinitialized \
                getlazyinitializer getparentclass issubclassof getstaticproperties getstaticpropertyvalue \
                setstaticpropertyvalue getdefaultproperties isiterable isiterateable implementsinterface \
                getextension getextensionname innamespace getnamespacename getshortname getattributes"),
        ],
        "reflectionuniontype" => &[
            ("ReflectionType", "allowsnull __tostring"),
        ],
        "regexiterator" => &[
            ("FilterIterator", "rewind next"),
            ("IteratorIterator", "getinneriterator valid key current"),
        ],
        "requestparsebodyexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "runtimeexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "seekableiterator" => &[
            ("Iterator", "current next key valid rewind"),
        ],
        "splfileobject" => &[
            ("SplFileInfo", "getpath getfilename getextension getbasename getpathname getperms getinode getsize getowner \
                getgroup getatime getmtime getctime gettype iswritable isreadable isexecutable isfile isdir \
                islink getlinktarget getrealpath getfileinfo getpathinfo openfile setfileclass setinfoclass \
                __debuginfo _bad_state_ex"),
        ],
        "splmaxheap" => &[
            ("SplHeap", "extract insert top count isempty rewind current key next valid recoverfromcorruption iscorrupted \
                __debuginfo __serialize __unserialize"),
        ],
        "splminheap" => &[
            ("SplHeap", "extract insert top count isempty rewind current key next valid recoverfromcorruption iscorrupted \
                __debuginfo __serialize __unserialize"),
        ],
        "splqueue" => &[
            ("SplDoublyLinkedList", "add pop shift push unshift top bottom __debuginfo count isempty setiteratormode getiteratormode \
                offsetexists offsetget offsetset offsetunset rewind current key prev next valid unserialize \
                serialize __serialize __unserialize"),
        ],
        "splstack" => &[
            ("SplDoublyLinkedList", "add pop shift push unshift top bottom __debuginfo count isempty setiteratormode getiteratormode \
                offsetexists offsetget offsetset offsetunset rewind current key prev next valid unserialize \
                serialize __serialize __unserialize"),
        ],
        "spltempfileobject" => &[
            ("SplFileInfo", "getpath getfilename getextension getbasename getpathname getperms getinode getsize getowner \
                getgroup getatime getmtime getctime gettype iswritable isreadable isexecutable isfile isdir \
                islink getlinktarget getrealpath getfileinfo getpathinfo openfile setfileclass setinfoclass \
                __debuginfo _bad_state_ex"),
            ("SplFileObject", "rewind eof valid fgets fread fgetcsv fputcsv setcsvcontrol getcsvcontrol flock fflush ftell \
                fseek fgetc fpassthru fscanf fwrite fstat ftruncate current key next setflags getflags \
                setmaxlinelen getmaxlinelen haschildren getchildren seek getcurrentline __tostring"),
        ],
        "throwable" => &[
            ("Stringable", "__tostring"),
        ],
        "typeerror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "underflowexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "unexpectedvalueexception" => &[
            ("Exception", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "unhandledmatcherror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        "valueerror" => &[
            ("Error", "__construct __wakeup getmessage getcode getfile getline gettrace getprevious gettraceasstring \
                __tostring"),
        ],
        _ => return None,
    };
    groups.iter().find_map(|(owner, methods)| methods.split_ascii_whitespace()
        .any(|name| name.eq_ignore_ascii_case(method)).then_some(*owner))
}

/// Flattened ReflectionFunctionAbstract methods retain their source owner without false inheritance.
#[test]
fn reflection_internal_declaring_class() {
    assert_eq!(inherited_declaring_class("ReflectionFunction", "GETNAME"),
        Some("ReflectionFunctionAbstract"));
    assert_eq!(inherited_declaring_class("ReflectionFunction", "getClosure"), None);
    assert_eq!(inherited_declaring_class("UserClass", "getName"), None);
    assert_eq!(inherited_declaring_class("Pdo\\Sqlite", "errorCode"), Some("PDO"));
}
