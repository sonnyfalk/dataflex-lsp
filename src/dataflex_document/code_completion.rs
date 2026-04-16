use super::*;
use index::MethodKind;

pub struct CodeCompletion {}

#[derive(Debug)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
}

#[derive(Debug)]
pub enum CompletionItemKind {
    Class,
    Object,
    Method,
    Property,
    LocalVariable,
    GlobalVariable,
    Function,
}

impl CodeCompletion {
    pub fn code_completion(doc: &DataFlexDocument, position: Point) -> Option<Vec<CompletionItem>> {
        let Some(context) = DocumentContext::context(doc, position) else {
            return None;
        };

        let completions = match context {
            DocumentContext::ClassReference => Some(Self::class_completions(doc)),
            DocumentContext::MethodReference(kind) => Some(Self::method_completions(doc, kind)),
            DocumentContext::CallReceiverReference => Some(Self::expr_completions(doc, position)),
            DocumentContext::Expression => Some(Self::expr_completions(doc, position)),
            DocumentContext::ParenExpression => Some(Self::paren_expr_completions(doc, position)),
        };

        completions
    }

    fn class_completions(doc: &DataFlexDocument) -> Vec<CompletionItem> {
        doc.index
            .get()
            .all_known_classes()
            .drain(..)
            .map(|class_name| CompletionItem {
                label: class_name.to_string(),
                kind: CompletionItemKind::Class,
            })
            .collect()
    }

    fn method_completions(doc: &DataFlexDocument, kind: index::MethodKind) -> Vec<CompletionItem> {
        match kind {
            MethodKind::Msg => doc
                .index
                .get()
                .all_known_methods(kind)
                .drain(..)
                .map(|method_name| CompletionItem {
                    label: method_name.to_string(),
                    kind: CompletionItemKind::Method,
                })
                .collect(),
            MethodKind::Get | MethodKind::Set => doc
                .index
                .get()
                .all_known_methods(kind)
                .drain(..)
                .map(|method_name| CompletionItem {
                    label: method_name.to_string(),
                    kind: CompletionItemKind::Method,
                })
                .chain(
                    doc.index
                        .get()
                        .all_known_properties()
                        .drain(..)
                        .map(|property_name| CompletionItem {
                            label: property_name.to_string(),
                            kind: CompletionItemKind::Property,
                        }),
                )
                .collect(),
        }
    }

    fn expr_completions(doc: &DataFlexDocument, position: Point) -> Vec<CompletionItem> {
        Self::local_variable_completions(doc, position)
            .chain(
                doc.index
                    .get()
                    .all_known_global_variables()
                    .drain(..)
                    .map(|variable_name| CompletionItem {
                        label: variable_name.to_string(),
                        kind: CompletionItemKind::GlobalVariable,
                    }),
            )
            .chain(
                doc.index
                    .get()
                    .all_known_objects()
                    .drain(..)
                    .map(|object_name| CompletionItem {
                        label: object_name.to_string(),
                        kind: CompletionItemKind::Object,
                    }),
            )
            .collect()
    }

    fn paren_expr_completions(doc: &DataFlexDocument, position: Point) -> Vec<CompletionItem> {
        Self::local_variable_completions(doc, position)
            .chain(Self::system_functions())
            .chain(
                doc.index
                    .get()
                    .all_known_global_variables()
                    .drain(..)
                    .map(|variable_name| CompletionItem {
                        label: variable_name.to_string(),
                        kind: CompletionItemKind::GlobalVariable,
                    }),
            )
            .chain(
                doc.index
                    .get()
                    .all_known_objects()
                    .drain(..)
                    .map(|object_name| CompletionItem {
                        label: object_name.to_string(),
                        kind: CompletionItemKind::Object,
                    }),
            )
            .chain(
                doc.index
                    .get()
                    .all_known_methods(MethodKind::Get)
                    .drain(..)
                    .map(|method_name| CompletionItem {
                        label: method_name.to_string(),
                        kind: CompletionItemKind::Method,
                    }),
            )
            .chain(
                doc.index
                    .get()
                    .all_known_properties()
                    .drain(..)
                    .map(|property_name| CompletionItem {
                        label: property_name.to_string(),
                        kind: CompletionItemKind::Property,
                    }),
            )
            .collect()
    }

    fn local_variable_completions(
        doc: &DataFlexDocument,
        position: Point,
    ) -> impl Iterator<Item = CompletionItem> {
        doc.local_variables(position)
            .map(|variable| CompletionItem {
                label: variable.symbol_path.name().to_string(),
                kind: CompletionItemKind::LocalVariable,
            })
    }

    fn system_functions() -> impl Iterator<Item = CompletionItem> {
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

        functions.into_iter().map(|f| CompletionItem {
            label: f.to_string(),
            kind: CompletionItemKind::Function,
        })
    }
}
