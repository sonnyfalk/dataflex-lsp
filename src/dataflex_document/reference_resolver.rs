use super::*;
use index::{
    ClassSymbol, IndexFileRef, IndexSymbolIter, IndexSymbolType, MethodKind, ReadableIndexRef,
};

pub struct ReferenceResolver<'a> {
    doc: &'a DataFlexDocument,
    index: ReadableIndexRef<'a>,
}

impl<'a> ReferenceResolver<'a> {
    pub fn new(doc: &'a DataFlexDocument) -> Self {
        Self {
            doc,
            index: doc.index.get(),
        }
    }

    pub fn resolve_reference(
        &self,
        context: DocumentContext,
        position: Point,
    ) -> IndexSymbolIter<'_> {
        match context {
            DocumentContext::ClassReference => self.resolve_class_reference(position),
            DocumentContext::MethodReference(kind) => self.resolve_method_reference(position, kind),
            DocumentContext::CallReceiverReference => self.resolve_expr_reference(position),
            DocumentContext::Expression => self.resolve_expr_reference(position),
            DocumentContext::ParenExpression => self.resolve_paren_expr_reference(position),
            DocumentContext::DotMemberExpression => self.resolve_member_expr_reference(position),
        }
    }

    fn resolve_class_reference(&self, position: Point) -> IndexSymbolIter<'_> {
        let Some(name) = self.doc.symbol_at_position(position) else {
            return IndexSymbolIter::empty();
        };

        IndexSymbolIter::new(
            self.index
                .find_class(&name)
                .and_then(|s| self.index.symbol_snapshot(s))
                .into_iter(),
        )
    }

    fn resolve_method_reference(&self, position: Point, kind: MethodKind) -> IndexSymbolIter<'_> {
        let Some(name) = self.doc.symbol_at_position(position) else {
            return IndexSymbolIter::empty();
        };

        if let Some(class) = self.resolve_call_receiver(position) {
            let members: Vec<&index::IndexSymbolRef> =
                self.index.find_members(&name, kind).collect();
            let member = self
                .index
                .class_hierarchy(class)
                .find_map(|class| {
                    members.iter().find(|member| {
                        member.symbol_path.parent_slice() == class.symbol_path.as_slice()
                    })
                })
                .cloned();
            IndexSymbolIter::new(
                member
                    .into_iter()
                    .filter_map(|member_ref| self.index.symbol_snapshot(&member_ref)),
            )
        } else {
            let members = self.index.find_members(&name, kind);
            IndexSymbolIter::new(
                members.filter_map(|member_ref| self.index.symbol_snapshot(member_ref)),
            )
        }
    }

    fn resolve_call_receiver(&self, position: Point) -> Option<&ClassSymbol> {
        let mut cursor = self.doc.cursor()?;
        cursor
            .goto_first_leaf_node_for_point(position)
            .then(|| cursor.goto_enclosing_method_call());

        let receiver = cursor
            .node()
            .child_by_field_name("receiver")
            .map(|n| self.doc.line_map.text_for_node(&n))
            .unwrap_or(String::from("self"));

        if receiver.eq_ignore_ascii_case("self") {
            cursor
                .goto_enclosing_object_or_class()
                .then(|| {
                    if cursor.is_object_definition() {
                        cursor
                            .node()
                            .child(0)
                            .and_then(|n| n.child_by_field_name("superclass"))
                            .and_then(|n| {
                                self.index
                                    .find_class(&self.doc.line_map.text_for_node(&n).into())
                            })
                            .and_then(|symbol_ref| self.index.symbol_snapshot(symbol_ref))
                            .and_then(|symbol_snapshot| {
                                ClassSymbol::from_index_symbol(symbol_snapshot.symbol)
                            })
                    } else {
                        cursor
                            .node()
                            .child(0)
                            .and_then(|n| n.child_by_field_name("name"))
                            .and_then(|n| {
                                self.index
                                    .find_class(&self.doc.line_map.text_for_node(&n).into())
                            })
                            .and_then(|symbol_ref| self.index.symbol_snapshot(symbol_ref))
                            .and_then(|symbol_snapshot| {
                                ClassSymbol::from_index_symbol(symbol_snapshot.symbol)
                            })
                    }
                })
                .flatten()
        } else {
            // FIXME: Handle non-self receiver.
            None
        }
    }

    fn resolve_expr_reference(&self, position: Point) -> IndexSymbolIter<'_> {
        let Some(name) = self.doc.symbol_at_position(position) else {
            return IndexSymbolIter::empty();
        };
        let file_ref = IndexFileRef::from(&self.doc.file_path);
        IndexSymbolIter::new(
            self.index
                .find_objects(&name)
                .filter(move |&s| s.symbol_path.is_top_level() || s.file_ref == file_ref)
                .filter_map(|s| self.index.symbol_snapshot(s))
                .chain(
                    self.index
                        .find_global_variables(&name)
                        .filter_map(|s| self.index.symbol_snapshot(s)),
                ),
        )
    }

    fn resolve_paren_expr_reference(&self, position: Point) -> IndexSymbolIter<'_> {
        let Some(name) = self.doc.symbol_at_position(position) else {
            return IndexSymbolIter::empty();
        };
        let file_ref = IndexFileRef::from(&self.doc.file_path);
        IndexSymbolIter::new(
            self.index
                .find_objects(&name)
                .filter(move |&s| s.symbol_path.is_top_level() || s.file_ref == file_ref)
                .filter_map(|s| self.index.symbol_snapshot(s))
                .chain(
                    self.index
                        .find_global_variables(&name)
                        .filter_map(|s| self.index.symbol_snapshot(s)),
                )
                .chain(
                    //FIXME: Filter on call receiver, like `resolve_method_reference()`.
                    self.index
                        .find_members(&name, MethodKind::Get)
                        .filter_map(|s| self.index.symbol_snapshot(s)),
                ),
        )
    }

    fn resolve_member_expr_reference(&self, position: Point) -> IndexSymbolIter<'_> {
        let Some(postfix_expr_node) = self.doc.cursor().and_then(|mut cursor| {
            (cursor.goto_descendant_for_point(position)
                && cursor.goto_enclosing_postfix_expression())
            .then_some(cursor.node())
        }) else {
            return IndexSymbolIter::empty();
        };

        let query = tree_sitter::Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            r#"
            (postfix_expression
              name: (identifier) @variable-name
              (member_access
                name: (identifier) @member-name)+)
            "#,
        )
        .expect("Error loading member_expr query");

        let variable_capture_index = query.capture_index_for_name("variable-name").unwrap();
        let member_capture_index = query.capture_index_for_name("member-name").unwrap();

        let mut query_cursor = tree_sitter::QueryCursor::new();
        let mut matches =
            query_cursor.matches(&query, postfix_expr_node, self.doc.line_map.text_provider());

        let symbol = if let Some(query_match) = matches.next()
            && let Some(variable_name) = query_match
                .nodes_for_capture_index(variable_capture_index)
                .next()
                .map(|n| index::SymbolName::from(self.doc.line_map.text_for_node(&n)))
        {
            let current_symbol = self
                .doc
                .find_local_variable(position, &variable_name)
                .as_ref()
                .or_else(|| {
                    self.index
                        .find_global_variables(&variable_name)
                        .next()
                        .and_then(|v| {
                            self.index
                                .symbol_snapshot(v)
                                .and_then(|s| index::VariableSymbol::from_index_symbol(s.symbol))
                        })
                })
                .and_then(|variable| self.index.find_struct(&variable.type_name))
                .and_then(|struct_ref| self.index.symbol_snapshot(struct_ref));

            query_match
                .nodes_for_capture_index(member_capture_index)
                .take_while(|n| n.start_position() < position)
                .map(|n| index::SymbolName::from(self.doc.line_map.text_for_node(&n)))
                .fold(current_symbol, |current_symbol, member_name| {
                    let Some(current_symbol) = current_symbol
                        .as_ref()
                        .and_then(|cs| index::VariableSymbol::from_index_symbol(cs.symbol))
                        .and_then(|variable| self.index.find_struct(&variable.type_name))
                        .and_then(|struct_ref| self.index.symbol_snapshot(struct_ref))
                        .or(current_symbol)
                    else {
                        return None;
                    };

                    if let Some(struct_symbol) =
                        index::StructSymbol::from_index_symbol(current_symbol.symbol)
                    {
                        struct_symbol
                            .members
                            .iter()
                            .find(|member| *member.name() == member_name)
                            .map(|member| index::IndexSymbolSnapshot {
                                path: current_symbol.path,
                                symbol: member,
                            })
                    } else {
                        Some(current_symbol)
                    }
                })
        } else {
            None
        };

        IndexSymbolIter::new(symbol.into_iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index;

    #[test]
    fn test_resolve_class_reference() {
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(
            r#"
Class cMyClass is a cBaseClass
End_Class
            "#,
            "test.pkg".into(),
            &index,
        );
        let doc = DataFlexDocument::new(
            "other.pkg".into(),
            r#"
Use test.pkg
Object oMyObject is a cMyClass
End_Object
            "#,
            index.clone(),
        );

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_class_reference(Point::new(2, 25));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: Class(ClassSymbol { location: Point { row: 1, column: 6 }, symbol_path: SymbolPath(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), members: [] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_method_reference() {
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(
            r#"
Class cMyClass is a cBaseClass
    Procedure testIt
    End_Procedure
End_Class

Class cMyOtherClass is a cBaseClass
    Procedure testIt
    End_Procedure
End_Class
            "#,
            "test.pkg".into(),
            &index,
        );
        let doc = DataFlexDocument::new(
            "other.pkg".into(),
            r#"
Use test.pkg
Object oMyObject is a cMyClass
    Procedure foo
        Send testIt
    End_Procedure
End_Object
            "#,
            index.clone(),
        );

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol =
            reference_resolver.resolve_method_reference(Point::new(4, 16), MethodKind::Msg);
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: Method(MethodSymbol { location: Point { row: 2, column: 14 }, symbol_path: SymbolPath(\"cMyClass.testIt\"), kind: Msg }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_call_receiver_reference() {
        let test_content = r#"
Object oMyObject is a cObject
    Procedure foo
    End_Procedure
End_Object

Send foo of oMyObject
            "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_expr_reference(Point::new(6, 16));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: Object(ClassSymbol { location: Point { row: 1, column: 7 }, symbol_path: SymbolPath(\"oMyObject\"), superclass: SymbolName(\"cObject\"), members: [Method(MethodSymbol { location: Point { row: 2, column: 14 }, symbol_path: SymbolPath(\"oMyObject.foo\"), kind: Msg })] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_member_expr_reference() {
        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    String sName
    Move myStruct.sName to sName
End_Procedure
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_member_expr_reference(Point::new(8, 21));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: Variable(VariableSymbol { location: Point { row: 2, column: 11 }, symbol_path: SymbolPath(\"tMyStruct.sName\"), type_name: SymbolName(\"String\") }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_nested_member_expr_reference() {
        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Struct tMyOtherStruct
    tMyStruct myStruct
End_Struct

Procedure test
    tMyOtherStruct myOtherStruct
    String sName
    Move myOtherStruct.myStruct.sName to sName
End_Procedure
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_member_expr_reference(Point::new(12, 35));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: Variable(VariableSymbol { location: Point { row: 2, column: 11 }, symbol_path: SymbolPath(\"tMyStruct.sName\"), type_name: SymbolName(\"String\") }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");

        let mut symbol = reference_resolver.resolve_member_expr_reference(Point::new(12, 25));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: Variable(VariableSymbol { location: Point { row: 6, column: 14 }, symbol_path: SymbolPath(\"tMyOtherStruct.myStruct\"), type_name: SymbolName(\"tMyStruct\") }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }
}
