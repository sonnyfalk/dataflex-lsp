use std::{collections::HashMap, ffi::OsStr, path::PathBuf};

use multimap::MultiMap;
use streaming_iterator::StreamingIterator;
use strum::EnumString;

mod index_file;
mod index_symbol;
mod indexer;
mod lookup_tables;
mod symbols_diff;
mod workspace;

pub use index_symbol::*;

pub use indexer::{Indexer, IndexerConfig, IndexerObserver, IndexerState};
pub use workspace::{DataFlexVersion, WorkspaceInfo};

pub use index_file::{IndexFile, IndexFileRef};

use lookup_tables::LookupTables;

#[derive(Debug)]
pub struct Index {
    workspace: WorkspaceInfo,
    files: HashMap<IndexFileRef, IndexFile>,
    lookup_tables: LookupTables,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct IndexRef {
    index: std::sync::Arc<std::sync::RwLock<Index>>,
}

pub type ReadableIndexRef<'a> = std::sync::RwLockReadGuard<'a, Index>;
pub type WriteableIndexRef<'a> = std::sync::RwLockWriteGuard<'a, Index>;

impl Index {
    pub fn new(workspace: WorkspaceInfo) -> Self {
        Self {
            workspace,
            files: HashMap::new(),
            lookup_tables: LookupTables::new(),
        }
    }

    pub fn find_class(&self, name: &SymbolName) -> Option<&IndexSymbolRef> {
        self.lookup_tables.class_lookup_table().get(name)
    }

    pub fn is_known_class(&self, name: &SymbolName) -> bool {
        self.lookup_tables.class_lookup_table().get(name).is_some()
    }

    pub fn all_known_classes(&self) -> Vec<SymbolName> {
        self.lookup_tables
            .class_lookup_table()
            .keys()
            .cloned()
            .collect()
    }

    pub fn is_known_property(&self, name: &SymbolName) -> bool {
        self.lookup_tables
            .property_lookup_table()
            .get(name)
            .is_some()
    }

    pub fn all_known_properties(&self) -> Vec<SymbolName> {
        self.lookup_tables
            .property_lookup_table()
            .keys()
            .cloned()
            .collect()
    }

    pub fn find_properties(&self, name: &SymbolName) -> core::slice::Iter<'_, IndexSymbolRef> {
        self.lookup_tables
            .property_lookup_table()
            .get_vec(name)
            .map(|v| v.iter())
            .unwrap_or_default()
    }

    pub fn is_known_method(&self, name: &SymbolName, kind: MethodKind) -> bool {
        self.lookup_tables
            .method_lookup_table(kind)
            .get(name)
            .is_some()
    }

    pub fn all_known_methods(&self, kind: MethodKind) -> Vec<SymbolName> {
        self.lookup_tables
            .method_lookup_table(kind)
            .keys()
            .cloned()
            .collect()
    }

    pub fn find_methods(
        &self,
        name: &SymbolName,
        kind: MethodKind,
    ) -> core::slice::Iter<'_, IndexSymbolRef> {
        self.lookup_tables
            .method_lookup_table(kind)
            .get_vec(name)
            .map(|v| v.iter())
            .unwrap_or_default()
    }

    pub fn find_members(
        &self,
        name: &SymbolName,
        kind: MethodKind,
    ) -> impl Iterator<Item = &IndexSymbolRef> + use<'_> {
        let methods = self.find_methods(name, kind);
        let properties = match kind {
            MethodKind::Get | MethodKind::Set => Some(self.find_properties(name)),
            MethodKind::Msg => None,
        };
        methods.chain(properties.unwrap_or_default())
    }

    pub fn is_known_object(&self, name: &SymbolName) -> bool {
        self.lookup_tables.object_lookup_table().get(name).is_some()
    }

    pub fn all_known_objects(&self) -> Vec<SymbolName> {
        self.lookup_tables
            .object_lookup_table()
            .keys()
            .cloned()
            .collect()
    }

    pub fn find_objects(&self, name: &SymbolName) -> core::slice::Iter<'_, IndexSymbolRef> {
        self.lookup_tables
            .object_lookup_table()
            .get_vec(name)
            .map(|v| v.iter())
            .unwrap_or_default()
    }

    pub fn all_known_global_variables(&self) -> Vec<SymbolName> {
        self.lookup_tables
            .global_variable_lookup_table()
            .keys()
            .cloned()
            .collect()
    }

    pub fn find_global_variables(
        &self,
        name: &SymbolName,
    ) -> impl Iterator<Item = &IndexSymbolRef> + use<'_> {
        self.lookup_tables
            .global_variable_lookup_table()
            .get(name)
            .into_iter()
    }

    pub fn is_known_alias_symbol(&self, name: &SymbolName) -> bool {
        self.lookup_tables.alias_lookup_table().get(name).is_some()
    }

    pub fn all_known_alias_symbols(&self) -> Vec<SymbolName> {
        self.lookup_tables
            .alias_lookup_table()
            .keys()
            .cloned()
            .collect()
    }

    pub fn find_alias_symbols(
        &self,
        name: &SymbolName,
    ) -> impl Iterator<Item = &IndexSymbolRef> + use<'_> {
        self.lookup_tables
            .alias_lookup_table()
            .get(name)
            .into_iter()
    }

    #[allow(dead_code)]
    pub fn find_struct(&self, name: &SymbolName) -> Option<&IndexSymbolRef> {
        self.lookup_tables.struct_lookup_table().get(name)
    }

    pub fn is_known_struct(&self, name: &SymbolName) -> bool {
        self.lookup_tables.struct_lookup_table().get(name).is_some()
    }

    #[allow(dead_code)]
    pub fn all_known_structs(&self) -> Vec<SymbolName> {
        self.lookup_tables
            .struct_lookup_table()
            .keys()
            .cloned()
            .collect()
    }

    pub fn all_system_functions(&self) -> Vec<SymbolName> {
        let functions = [
            "Abs",
            "Acos",
            "AddBitValue",
            "AddressOf",
            "Alloc",
            "AnsiToUtf8",
            "Append",
            "AppendArray",
            "Ascii",
            "Asin",
            "Atan",
            "B_From_RGB",
            "Base64Decode",
            "Base64Encode",
            "BinarySearchInsertPos",
            "BinarySearchArray",
            "Cast",
            "Center",
            "Character",
            "Convert",
            "ConvertFromClient",
            "ConvertToClient",
            "CopyArray",
            "CountArray",
            "CString",
            "CStringSize",
            "CStringLength",
            "Cos",
            "CurrentDateTime",
            "Date",
            "DateAddDay",
            "DateAddHour",
            "DateAddMillisecond",
            "DateAddMinute",
            "DateAddMonth",
            "DateAddSecond",
            "DateAddYear",
            "DateGetDay",
            "DateGetDayOfWeek",
            "DateGetDayofWeek_WDS",
            "DateGetDayofWeekISO",
            "DateGetDayOfYear",
            "DateGetHour",
            "DateGetMillisecond",
            "DateGetMinute",
            "DateGetMonth",
            "DateGetSecond",
            "DateGetWeekOfYear",
            "DateGetWeekOfYear_WDS",
            "DateGetWeekOfYearISO",
            "DateGetYear",
            "DateGetYearOfWeekISO",
            "DateSet",
            "DateSetDay",
            "DateSetHour",
            "DateSetMillisecond",
            "DateSetMinute",
            "DateSetMonth",
            "DateSetSecond",
            "DateSetYear",
            "Default_Currency_Symbol",
            "DeRefC",
            "DeRefDw",
            "DeRefW",
            "DeSerializeRowId",
            "Eval",
            "Exp",
            "ExtractFileName",
            "ExtractFilePath",
            "Field_Number_Default_Mask",
            "FillArray",
            "FillString",
            "FindByRowId",
            "FormatCurrency",
            "FormatNumber",
            "FormatValue",
            "Free",
            "G_From_RGB",
            "GetRowID",
            "GUIScreen_Size",
            "Hi",
            "HTMLEncode",
            "HTMLEncodeNoCRLF",
            "If",
            "Info_Box",
            "Insert",
            "InsertInArray",
            "Integer",
            "IsAdministrator",
            "IsCOMObject",
            "IsDateValid",
            "IsDebuggerPresent",
            "IsFlagIn",
            "IsFileNameQualified",
            "IsNullCOMObject",
            "IsNullDateTime",
            "IsNullRowID",
            "IsSameArray",
            "IsSameCOMObject",
            "IsSameRowID",
            "IsSameStruct",
            "IsTimeSpanValid",
            "IsTimeValid",
            "Left",
            "Length",
            "Log",
            "Low",
            "Lowercase",
            "LTrim",
            "MaxArray",
            "MemCompare",
            "MemCopy",
            "MemSet",
            "Message_Box",
            "Mid",
            "MinArray",
            "Mod",
            "ModAlt",
            "NamedValueAdd",
            "NamedValueGet",
            "NamedValueIndex",
            "NormalizeString",
            "NamedValueRemove",
            "Not Function",
            "NullCOMObject",
            "NullDateTime",
            "NullRowID",
            "Number",
            "Number_Default_Mask",
            "OemToUtf8",
            "OemToUtf8Buffer",
            "Overstrike",
            "Pad",
            "PointerToString",
            "PointerToWString",
            "Pos",
            "R_From_RGB",
            "Random",
            "RandomHexUUID",
            "Real",
            "ReAlloc",
            "RefClass",
            "RefFunc",
            "RefProc",
            "RefProcSet",
            "RefTable",
            "Remove",
            "RemoveBitValue",
            "RemoveFromArray",
            "Repeat",
            "Replace",
            "Replaces",
            "ResizeArray",
            "ReverseArray",
            "RGB",
            "Right",
            "RightPos",
            "Round",
            "RTrim",
            "SearchArray",
            "SerializeRowId",
            "Seq_New_Channel",
            "Seq_Release_Channel",
            "SeqHexUUID",
            "SFormat",
            "ShowLastError",
            "ShuffleArray",
            "Sin",
            "SizeOfArray",
            "SizeOfType",
            "SizeOfString",
            "SizeOfWString",
            "SortArray",
            "SpanAddDay",
            "SpanAddHour",
            "SpanAddMillisecond",
            "SpanAddMinute",
            "SpanAddSecond",
            "SpanDays",
            "SpanHours",
            "SpanMilliseconds",
            "SpanMinutes",
            "SpanSeconds",
            "SpanTotalDays",
            "SpanTotalHours",
            "SpanTotalMilliseconds",
            "SpanTotalMinutes",
            "SpanTotalSeconds",
            "Sqrt",
            "Stop_Box",
            "StoreC",
            "StoreDw",
            "StoreW",
            "String",
            "StringToUCharArray",
            "StrJoinFromArray",
            "StrSplitToArray",
            "SysConf",
            "Tan",
            "Trim",
            "ToANSI",
            "ToOEM",
            "UCharArrayToString",
            "UCharArrayToWString",
            "UCharToString",
            "Uppercase",
            "UserError",
            "Utf8ToAnsi",
            "Utf8ToOem",
            "Utf8ToOemBuffer",
            "WindowsMessage",
            "WMLEncode",
            "WMLEncodeNOCRLF",
            "WStringToUCharArray",
            "VariantStringLength",
            "YesNo_Box",
            "YesNoCancel_Box",
            "ZeroString",
        ];

        functions.into_iter().map(SymbolName::from).collect()
    }

    pub fn is_system_function(&self, name: &SymbolName) -> bool {
        self.all_system_functions()
            .into_iter()
            .find(|f| f == name)
            .is_some()
    }

    pub fn class_hierarchy<'a>(&'a self, class: &'a ClassSymbol) -> ClassHierarchyIter<'a> {
        ClassHierarchyIter {
            index: self,
            current: Some(class),
            mixins: Default::default(),
        }
    }

    pub fn symbol_snapshot(
        &self,
        symbol_ref: &IndexSymbolRef,
    ) -> Option<IndexSymbolSnapshot<'_, IndexSymbol>> {
        if let Some(index_file) = self.files.get(&symbol_ref.file_ref) {
            index_file
                .resolve(&symbol_ref.symbol_path)
                .map(|index_symbol| IndexSymbolSnapshot {
                    path: &index_file.path,
                    symbol: index_symbol,
                })
        } else {
            None
        }
    }
}

pub struct ClassHierarchyIter<'a> {
    index: &'a Index,
    current: Option<&'a ClassSymbol>,
    mixins: core::slice::Iter<'a, SymbolName>,
}

impl<'a> Iterator for ClassHierarchyIter<'a> {
    type Item = &'a ClassSymbol;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(mixin) = self
            .mixins
            .next()
            .and_then(|class_name| self.index.find_class(class_name))
            .and_then(|symbol_ref| self.index.symbol_snapshot(symbol_ref))
            .and_then(|symbol_snapshot| ClassSymbol::from_index_symbol(symbol_snapshot.symbol))
        {
            Some(mixin)
        } else {
            self.mixins = self
                .current
                .map(|class| class.mixins.iter())
                .unwrap_or_default();
            let next = self
                .current
                .and_then(|class| self.index.find_class(&class.superclass))
                .and_then(|symbol_ref| self.index.symbol_snapshot(symbol_ref))
                .and_then(|symbol_snapshot| ClassSymbol::from_index_symbol(symbol_snapshot.symbol));
            if let Some(next) = next {
                self.current.replace(next)
            } else {
                self.current.take()
            }
        }
    }
}

pub struct IndexSymbolIter<'a> {
    inner: Box<dyn Iterator<Item = IndexSymbolSnapshot<'a, IndexSymbol>> + 'a>,
}

impl<'a> IndexSymbolIter<'a> {
    pub fn new(inner: impl Iterator<Item = IndexSymbolSnapshot<'a, IndexSymbol>> + 'a) -> Self {
        Self {
            inner: Box::new(inner),
        }
    }

    pub fn empty() -> Self {
        Self::new(std::iter::empty())
    }
}

impl<'a> Iterator for IndexSymbolIter<'a> {
    type Item = IndexSymbolSnapshot<'a, IndexSymbol>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl IndexRef {
    pub fn new(index: Index) -> Self {
        Self {
            index: std::sync::Arc::new(std::sync::RwLock::new(index)),
        }
    }

    pub fn get(&self) -> ReadableIndexRef<'_> {
        self.index
            .read()
            .expect("unable to acquire index read lock")
    }

    pub fn get_mut(&self) -> WriteableIndexRef<'_> {
        self.index
            .write()
            .expect("unable to acquire index write lock")
    }
}

#[cfg(test)]
impl Index {
    pub fn make_test_index() -> Self {
        Self::new(WorkspaceInfo::new())
    }
}

#[cfg(test)]
impl IndexRef {
    pub fn make_test_index_ref() -> Self {
        Self::new(Index::make_test_index())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_class() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\nEnd_Class\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!("{:?}", index_ref.get().find_class(&"cMyClass".into())),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath(\"cMyClass\") })"
        );
    }

    #[test]
    fn test_find_methods() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure SayHello\n    End_Procedure\nEnd_Class\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref
                    .get()
                    .find_methods(&"SayHello".into(), MethodKind::Msg)
                    .next()
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath(\"cMyClass.SayHello\") })"
        );
    }

    #[test]
    fn test_find_properties() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyClass is a cBaseClass\n    Procedure Construct_Object\n        Property Integer piTest 0\n    End_Procedure\nEnd_Class\n",
            "test.pkg".into(),
            &index_ref,
        );

        assert_eq!(
            format!(
                "{:?}",
                index_ref.get().find_properties(&"piTest".into()).next()
            ),
            "Some(IndexSymbolRef { file_ref: IndexFileRef(\"test.pkg\"), symbol_path: SymbolPath(\"cMyClass.piTest\") })"
        );
    }

    #[test]
    fn test_class_hierarchy() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            "Class cMyBaseClass is a cBaseClass\nEnd_Class\nClass cMySubClass is a cMyBaseClass\nEnd_Class\n",
            "test.pkg".into(),
            &index_ref,
        );
        let index = index_ref.get();
        let class = index
            .find_class(&"cMySubClass".into())
            .and_then(|symbol_ref| index.symbol_snapshot(symbol_ref))
            .and_then(|symbol_snapshot| ClassSymbol::from_index_symbol(symbol_snapshot.symbol))
            .unwrap();

        let mut class_hierarchy = index.class_hierarchy(class);
        assert_eq!(
            format!("{:?}", class_hierarchy.next()),
            "Some(ClassSymbol { location: SourceLocation { line: 2, column: 6 }, range: SourceRange { start: SourceLocation { line: 2, column: 0 }, end: SourceLocation { line: 3, column: 9 } }, symbol_path: SymbolPath(\"cMySubClass\"), superclass: SymbolName(\"cMyBaseClass\"), mixins: [], members: [] })"
        );
        assert_eq!(
            format!("{:?}", class_hierarchy.next()),
            "Some(ClassSymbol { location: SourceLocation { line: 0, column: 6 }, range: SourceRange { start: SourceLocation { line: 0, column: 0 }, end: SourceLocation { line: 1, column: 9 } }, symbol_path: SymbolPath(\"cMyBaseClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [] })"
        );
        assert_eq!(format!("{:?}", class_hierarchy.next()), "None");
    }

    #[test]
    fn test_class_hierarchy_with_mixins() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_test_content(
            r#"
Class cMyMixin is a cMixin
End_Class

Class cMyOtherMixin is a cMixin
End_Class

Class cMyBaseClass is a cBaseClass
    Import_Class_Protocol cMyMixin
End_Class


Class cMySubClass is a cMyBaseClass
    Import_Class_Protocol cMyOtherMixin
End_Class
            "#,
            "test.pkg".into(),
            &index_ref,
        );
        let index = index_ref.get();
        let class = index
            .find_class(&"cMySubClass".into())
            .and_then(|symbol_ref| index.symbol_snapshot(symbol_ref))
            .and_then(|symbol_snapshot| ClassSymbol::from_index_symbol(symbol_snapshot.symbol))
            .unwrap();

        let mut class_hierarchy = index.class_hierarchy(class);
        assert_eq!(
            format!("{:?}", class_hierarchy.next()),
            "Some(ClassSymbol { location: SourceLocation { line: 12, column: 6 }, range: SourceRange { start: SourceLocation { line: 12, column: 0 }, end: SourceLocation { line: 14, column: 9 } }, symbol_path: SymbolPath(\"cMySubClass\"), superclass: SymbolName(\"cMyBaseClass\"), mixins: [SymbolName(\"cMyOtherMixin\")], members: [] })"
        );
        assert_eq!(
            format!("{:?}", class_hierarchy.next()),
            "Some(ClassSymbol { location: SourceLocation { line: 4, column: 6 }, range: SourceRange { start: SourceLocation { line: 4, column: 0 }, end: SourceLocation { line: 5, column: 9 } }, symbol_path: SymbolPath(\"cMyOtherMixin\"), superclass: SymbolName(\"cMixin\"), mixins: [], members: [] })"
        );
        assert_eq!(
            format!("{:?}", class_hierarchy.next()),
            "Some(ClassSymbol { location: SourceLocation { line: 7, column: 6 }, range: SourceRange { start: SourceLocation { line: 7, column: 0 }, end: SourceLocation { line: 9, column: 9 } }, symbol_path: SymbolPath(\"cMyBaseClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [SymbolName(\"cMyMixin\")], members: [] })"
        );
        assert_eq!(
            format!("{:?}", class_hierarchy.next()),
            "Some(ClassSymbol { location: SourceLocation { line: 1, column: 6 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 2, column: 9 } }, symbol_path: SymbolPath(\"cMyMixin\"), superclass: SymbolName(\"cMixin\"), mixins: [], members: [] })"
        );
        assert_eq!(format!("{:?}", class_hierarchy.next()), "None");
    }
}
