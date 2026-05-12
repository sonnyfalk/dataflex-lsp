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
            DocumentContext::DotMemberExpression => Some(Self::dot_completions(doc, position)),
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

    fn dot_completions(doc: &DataFlexDocument, position: Point) -> Vec<CompletionItem> {
        let Some(mut cursor) = doc.cursor().and_then(|mut cursor| {
            cursor
                .goto_leaf_node_before_point(position)
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

        let symbol_snapshot = if cursor.goto_enclosing_member_access()
            && cursor.goto_previous_sibling()
            && cursor.is_identifier()
        {
            let name = index::SymbolName::from(doc.line_map.text_for_node(&cursor.node()));
            reference_resolver
                .resolve_type_of_variable(cursor.node().start_position(), &name)
                .and_then(|type_name| index.find_struct(&type_name))
                .and_then(|struct_ref| index.symbol_snapshot(struct_ref))
        } else {
            reference_resolver
                .resolve_reference(DocumentContext::DotMemberExpression, position)
                .next()
        };

        symbol_snapshot
            .and_then(|s| StructSymbol::from_index_symbol(s.symbol))
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
        let completions = CodeCompletion::code_completion(&doc, Point::new(7, 28)).unwrap();
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
        let completions = CodeCompletion::code_completion(&doc, Point::new(7, 29)).unwrap();
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
        let completions = CodeCompletion::code_completion(&doc, Point::new(7, 29)).unwrap();
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
        let completions = CodeCompletion::code_completion(&doc, Point::new(7, 34)).unwrap();
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
        let completions = CodeCompletion::code_completion(&doc, Point::new(11, 42)).unwrap();
        assert_eq!(completions.len(), 1);
    }
}
