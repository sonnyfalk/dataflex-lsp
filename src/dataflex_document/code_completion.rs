use std::fmt::Write;

use super::*;
use index::{IndexSymbolType, MethodKind, MethodSymbol, StructSymbol};

pub struct CodeCompletion {}

#[derive(Debug, Default)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub details: Option<String>,
    pub insert_text: Option<String>,
}

#[derive(Debug, Default)]
pub enum CompletionItemKind {
    #[default]
    Text,
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
    Struct,
}

impl CodeCompletion {
    pub fn code_completion(
        doc: &DataFlexDocument,
        position: Point,
        auto_complete: bool,
    ) -> Option<Vec<CompletionItem>> {
        let context = DocumentContext::context(doc, position)?;
        if auto_complete && !Self::should_auto_complete_with_context(&context) {
            return None;
        }

        let completions = match context {
            DocumentContext::ClassReference => Some(Self::class_completions(doc)),
            DocumentContext::MethodReference(kind) => {
                Some(Self::method_completions(doc, position, kind))
            }
            DocumentContext::Expression => Some(Self::expr_completions(doc, position)),
            DocumentContext::ParenExpression => Some(Self::paren_expr_completions(doc, position)),
            DocumentContext::DotMemberExpression => Some(Self::dot_completions(doc, position)),
            DocumentContext::CommandReference => Some(Self::command_completions(doc)),
            DocumentContext::FileDependency => Some(Self::file_completions(doc)),
            DocumentContext::MethodDeclaration(kind) => {
                Some(Self::override_completions(doc, position, kind))
            }
            DocumentContext::TypeReference => Some(Self::type_completions(doc)),
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
            DocumentContext::MethodDeclaration(_) => false,
            DocumentContext::TypeReference => false,
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
                ..Default::default()
            })
            .collect()
    }

    fn method_completions(
        doc: &DataFlexDocument,
        position: Point,
        kind: index::MethodKind,
    ) -> Vec<CompletionItem> {
        let completions: Vec<CompletionItem> =
            match kind {
                MethodKind::Msg => doc
                    .index
                    .get()
                    .all_known_methods(kind)
                    .drain(..)
                    .map(|method_name| CompletionItem {
                        label: method_name.to_string(),
                        kind: CompletionItemKind::Method,
                        ..Default::default()
                    })
                    .collect(),
                MethodKind::Get | MethodKind::Set => {
                    doc.index
                        .get()
                        .all_known_methods(kind)
                        .drain(..)
                        .map(|method_name| CompletionItem {
                            label: method_name.to_string(),
                            kind: CompletionItemKind::Method,
                            ..Default::default()
                        })
                        .chain(doc.index.get().all_known_properties().drain(..).map(
                            |property_name| CompletionItem {
                                label: property_name.to_string(),
                                kind: CompletionItemKind::Property,
                                ..Default::default()
                            },
                        ))
                        .collect()
                }
            };

        if let Some(mut cursor) = doc.cursor()
            && cursor.goto_leaf_node_at_or_before_point(position)
            && let Some(filter_text) = cursor
                .is_identifier()
                .then(|| doc.line_map.text_for_node(&cursor.node()))
                .filter(|text| text.contains('.'))
            && let Some(filter_text) = filter_text.rfind('.').map(|indx| &filter_text[..=indx])
        {
            // Code completion with embedded dot, e.g. Send Private.MyMethod.
            // Filter out prefix before the last dot, since code completion context is after the dot.
            completions
                .into_iter()
                .filter_map(|mut cc| {
                    if cc.label.len() >= filter_text.len()
                        && cc.label[..filter_text.len()].eq_ignore_ascii_case(filter_text)
                    {
                        cc.label = cc.label[filter_text.len()..].into();
                        Some(cc)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            completions
        }
    }

    fn override_completions(
        doc: &DataFlexDocument,
        position: Point,
        kind: index::MethodKind,
    ) -> Vec<CompletionItem> {
        let index = doc.index.get();
        if let Some(mut cursor) = doc.cursor()
            && cursor.goto_descendant_for_point(position)
            && cursor.goto_enclosing_object_or_class()
        {
            let superclass = cursor
                .node()
                .child(0)
                .and_then(|n| n.child_by_field_name("superclass"))
                .map(|n| doc.line_map.text_for_node(&n))
                .and_then(|superclass_name| index.find_class(&superclass_name.into()))
                .and_then(|symbol_ref| index.resolve_symbol(symbol_ref));

            // TODO: Filter out already overridden methods.
            superclass
                .into_iter()
                .flat_map(|superclass| index.inherited_class_members(superclass, kind))
                .map(|m| {
                    let mut details = String::new();
                    if let Some(method_symbol) = MethodSymbol::from_index_symbol(m.symbol) {
                        for (name, data_type) in &method_symbol.parameters {
                            _ = write!(details, " {} {}", data_type, name);
                        }
                        if let Some(return_type) = &method_symbol.return_type {
                            _ = write!(details, " Returns {}", return_type);
                        }
                    }
                    CompletionItem {
                        label: m.symbol.name().to_string(),
                        kind: CompletionItemKind::Method,
                        details: Some(details.clone()),
                        insert_text: Some(format!("{}{}\n    ", m.symbol.name(), details)),
                    }
                })
                .collect()
        } else {
            vec![]
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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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

        let qualified_symbol = if let Some(name) = root_name.as_ref() {
            reference_resolver
                .resolve_type_of_variable(cursor.node().start_position(), name)
                .and_then(|data_type| index.find_struct(data_type.name()))
                .and_then(|struct_ref| index.resolve_symbol(struct_ref))
        } else {
            reference_resolver
                .resolve_reference(DocumentContext::DotMemberExpression, position)
                .next()
        };

        if let Some(qualified_symbol) = qualified_symbol {
            StructSymbol::from_index_symbol(qualified_symbol.symbol)
                .map(|struct_symbol| {
                    struct_symbol
                        .members
                        .iter()
                        .map(|member| CompletionItem {
                            label: member.name().to_string(),
                            kind: CompletionItemKind::StructMember,
                            ..Default::default()
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else if let Some(table) = root_name
            .as_ref()
            .and_then(|name| index.find_dataflex_table(name).map(|t| t.table))
        {
            table
                .columns
                .iter()
                .map(|column| CompletionItem {
                    label: column.to_string(),
                    kind: CompletionItemKind::TableColumn,
                    ..Default::default()
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn command_completions(doc: &DataFlexDocument) -> Vec<CompletionItem> {
        Self::system_commands(doc)
            .chain(
                doc.index
                    .get()
                    .all_known_structs()
                    .into_iter()
                    .chain(doc.index.get().all_system_types())
                    .map(|name| CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Struct,
                        ..Default::default()
                    }),
            )
            .collect()
    }

    fn file_completions(doc: &DataFlexDocument) -> Vec<CompletionItem> {
        doc.index
            .get()
            .all_known_files()
            .into_iter()
            .filter_map(|file_ref| {
                Some(CompletionItem {
                    label: file_ref.try_into().ok()?,
                    kind: CompletionItemKind::File,
                    ..Default::default()
                })
            })
            .collect()
    }

    fn type_completions(doc: &DataFlexDocument) -> Vec<CompletionItem> {
        doc.index
            .get()
            .all_known_structs()
            .into_iter()
            .chain(doc.index.get().all_system_types())
            .map(|name| CompletionItem {
                label: name.to_string(),
                kind: CompletionItemKind::Struct,
                ..Default::default()
            })
            .collect()
    }

    fn local_variable_completions(
        doc: &DataFlexDocument,
        position: Point,
    ) -> impl Iterator<Item = CompletionItem> {
        let reference_resolver = ReferenceResolver::new(doc);
        reference_resolver
            .local_variables(position)
            .map(|variable| CompletionItem {
                label: variable.symbol_path.name().to_string(),
                kind: CompletionItemKind::LocalVariable,
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
