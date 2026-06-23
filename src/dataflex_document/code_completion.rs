use super::*;
use index::{IndexSymbolType, MethodKind, StructSymbol};

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
    StructMember,
    EnumMember,
    TableName,
    TableColumn,
    Command,
    File,
}

impl CodeCompletion {
    pub fn code_completion(
        doc: &DataFlexDocument,
        position: Point,
        auto_complete: bool,
    ) -> Option<Vec<CompletionItem>> {
        let Some(context) = DocumentContext::context(doc, position) else {
            return None;
        };
        if auto_complete && !Self::should_auto_complete_with_context(&context) {
            return None;
        }

        let completions = match context {
            DocumentContext::ClassReference => Some(Self::class_completions(doc)),
            DocumentContext::MethodReference(kind) => Some(Self::method_completions(doc, kind)),
            DocumentContext::Expression => Some(Self::expr_completions(doc, position)),
            DocumentContext::ParenExpression => Some(Self::paren_expr_completions(doc, position)),
            DocumentContext::DotMemberExpression => Some(Self::dot_completions(doc, position)),
            DocumentContext::CommandReference => Some(Self::command_completions(doc)),
            DocumentContext::FileDependency => Some(Self::file_completions(doc, position)),
        };

        completions
    }

    fn should_auto_complete_with_context(context: &DocumentContext) -> bool {
        match context {
            DocumentContext::ClassReference => true,
            DocumentContext::MethodReference(_) => true,
            DocumentContext::DotMemberExpression => true,
            DocumentContext::FileDependency => true,
            DocumentContext::Expression => false,
            DocumentContext::ParenExpression => false,
            DocumentContext::CommandReference => false,
        }
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
            .chain(
                doc.index
                    .get()
                    .all_known_alias_symbols()
                    .drain(..)
                    .map(|alias_name| CompletionItem {
                        label: alias_name.to_string(),
                        kind: CompletionItemKind::EnumMember,
                    }),
            )
            .chain(
                doc.index
                    .get()
                    .all_known_dataflex_tables()
                    .drain(..)
                    .map(|table_name| CompletionItem {
                        label: table_name.to_string(),
                        kind: CompletionItemKind::TableName,
                    }),
            )
            .collect()
    }

    fn paren_expr_completions(doc: &DataFlexDocument, position: Point) -> Vec<CompletionItem> {
        Self::local_variable_completions(doc, position)
            .chain(Self::system_functions(doc))
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
                    .all_known_alias_symbols()
                    .drain(..)
                    .map(|alias_name| CompletionItem {
                        label: alias_name.to_string(),
                        kind: CompletionItemKind::EnumMember,
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
            .chain(
                doc.index
                    .get()
                    .all_known_classes()
                    .drain(..)
                    .map(|class_name| CompletionItem {
                        label: class_name.to_string(),
                        kind: CompletionItemKind::Class,
                    }),
            )
            .chain(
                doc.index
                    .get()
                    .all_known_dataflex_tables()
                    .drain(..)
                    .map(|table_name| CompletionItem {
                        label: table_name.to_string(),
                        kind: CompletionItemKind::TableName,
                    }),
            )
            .collect()
    }

    fn dot_completions(doc: &DataFlexDocument, position: Point) -> Vec<CompletionItem> {
        let Some(mut cursor) = doc.cursor().and_then(|mut cursor| {
            cursor
                .goto_leaf_node_at_or_before_point(position)
                .then_some(cursor)
        }) else {
            return vec![];
        };

        while !cursor.is_dot() {
            cursor.goto_previous_leaf_node();
        }

        let position = cursor.node().end_position();
        let index = doc.index.get();
        let reference_resolver = ReferenceResolver::new(doc);

        let root_name = if cursor.goto_enclosing_member_access()
            && cursor.goto_previous_sibling()
            && cursor.is_identifier()
        {
            Some(index::SymbolName::from(
                doc.line_map.text_for_node(&cursor.node()),
            ))
        } else {
            None
        };

        let symbol_snapshot = if let Some(name) = root_name.as_ref() {
            reference_resolver
                .resolve_type_of_variable(cursor.node().start_position(), name)
                .and_then(|data_type| index.find_struct(data_type.name()))
                .and_then(|struct_ref| index.symbol_snapshot(struct_ref))
        } else {
            reference_resolver
                .resolve_reference(DocumentContext::DotMemberExpression, position)
                .next()
        };

        if let Some(symbol_snapshot) = symbol_snapshot {
            StructSymbol::from_index_symbol(symbol_snapshot.symbol)
                .map(|struct_symbol| {
                    struct_symbol
                        .members
                        .iter()
                        .map(|member| CompletionItem {
                            label: member.name().to_string(),
                            kind: CompletionItemKind::StructMember,
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else if let Some(table) = root_name
            .as_ref()
            .and_then(|name| index.find_dataflex_table(name))
        {
            table
                .columns
                .iter()
                .map(|column| CompletionItem {
                    label: column.to_string(),
                    kind: CompletionItemKind::TableColumn,
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn command_completions(doc: &DataFlexDocument) -> Vec<CompletionItem> {
        Self::system_commands(doc).collect()
    }

    fn file_completions(doc: &DataFlexDocument, _position: Point) -> Vec<CompletionItem> {
        doc.index
            .get()
            .all_known_files()
            .into_iter()
            .filter_map(|file_ref| {
                Some(CompletionItem {
                    label: file_ref.try_into().ok()?,
                    kind: CompletionItemKind::File,
                })
            })
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

    fn system_functions(doc: &DataFlexDocument) -> impl Iterator<Item = CompletionItem> {
        doc.index
            .get()
            .all_system_functions()
            .into_iter()
            .map(|f| CompletionItem {
                label: f.to_string(),
                kind: CompletionItemKind::Function,
            })
    }

    fn system_commands(doc: &DataFlexDocument) -> impl Iterator<Item = CompletionItem> {
        doc.index
            .get()
            .all_commands()
            .into_iter()
            .map(|command| CompletionItem {
                label: command.to_string(),
                kind: CompletionItemKind::Command,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dot_completions() {
        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    Move "test" to myStruct.
End_Procedure
                "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let completions = CodeCompletion::code_completion(&doc, Point::new(7, 28), false).unwrap();
        assert_eq!(completions.len(), 1);

        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    Move "test" to myStruct.s
End_Procedure
                "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let completions = CodeCompletion::code_completion(&doc, Point::new(7, 29), false).unwrap();
        assert_eq!(completions.len(), 1);

        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    Move "test" to myStruct.s
End_Procedure
                "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let completions = CodeCompletion::code_completion(&doc, Point::new(7, 29), false).unwrap();
        assert_eq!(completions.len(), 1);

        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    Move "test" to myStruct.sName.
End_Procedure
                "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let completions = CodeCompletion::code_completion(&doc, Point::new(7, 34), false).unwrap();
        assert_eq!(completions.len(), 0);

        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Struct tMyOtherStruct
    tMyStruct myStruct
End_Struct

Procedure test
    tMyOtherStruct myOtherStruct
    Move "test" to myOtherStruct.myStruct.
End_Procedure
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let completions = CodeCompletion::code_completion(&doc, Point::new(11, 42), false).unwrap();
        assert_eq!(completions.len(), 1);
    }
}
