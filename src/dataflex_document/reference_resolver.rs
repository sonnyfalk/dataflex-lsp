use super::*;
use index::{
    ClassSymbol, DataFlexDataType, IndexFileRef, IndexSymbolIter, IndexSymbolType, MethodKind,
    QualifiedDataFlexTableRef, QualifiedIndexSymbol, ReadableIndexRef, StructSymbol, SymbolName,
    VariableSymbol,
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
            DocumentContext::Expression => self.resolve_expr_reference(position),
            DocumentContext::ParenExpression => self.resolve_paren_expr_reference(position),
            DocumentContext::DotMemberExpression => self.resolve_member_expr_reference(position),
            DocumentContext::CommandReference => self.resolve_type_reference(position),
            DocumentContext::FileDependency => IndexSymbolIter::empty(),
            DocumentContext::MethodDeclaration(_) => IndexSymbolIter::empty(),
            DocumentContext::TypeReference => self.resolve_type_reference(position),
        }
    }

    pub fn resolve_type_of_variable(
        &self,
        scope: Point,
        name: &SymbolName,
    ) -> Option<DataFlexDataType> {
        self.find_local_variable(&name, scope)
            .as_ref()
            .or_else(|| {
                self.index
                    .find_global_variables(&name)
                    .next()
                    .and_then(|v| self.index.resolve_symbol(v))
                    .and_then(|s| VariableSymbol::from_index_symbol(s.symbol))
            })
            .map(|variable| variable.data_type.clone())
    }

    pub fn resolve_local_variable(&self, position: Point) -> Option<VariableSymbol> {
        self.doc
            .symbol_at_position(position)
            .and_then(|name| self.find_local_variable(&name, position))
    }

    pub fn resolve_table_reference(
        &self,
        position: Point,
    ) -> Option<QualifiedDataFlexTableRef<'_>> {
        let mut cursor = self.doc.cursor()?;
        if !cursor.goto_descendant_for_point(position) {
            return None;
        }
        let node = if cursor.goto_enclosing_postfix_expression() {
            cursor.node().child_by_field_name("name")
        } else {
            cursor.is_identifier().then_some(cursor.node())
        };
        node.and_then(|n| {
            self.index
                .find_dataflex_table(&self.doc.line_map.text_for_node(&n).into())
        })
    }

    pub fn find_local_variable(
        &self,
        name: &SymbolName,
        scope_point: Point,
    ) -> Option<VariableSymbol> {
        self.local_variables(scope_point)
            .find(|variable| variable.symbol_path.name() == name)
    }

    pub fn local_variables(
        &self,
        scope_point: Point,
    ) -> impl Iterator<Item = index::VariableSymbol> + use<'a> {
        let Some(method_node) = self
            .doc
            .cursor()
            .and_then(|mut cursor| {
                cursor
                    .goto_leaf_node_at_or_after_point(scope_point)
                    .then(|| {
                        cursor
                            .goto_enclosing_method_definition()
                            .then(|| cursor.node())
                    })
            })
            .flatten()
        else {
            return Vec::new().into_iter();
        };

        let query = tree_sitter::Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            r#"
            (parameter
              [
                (system_typedecl
                  (system_type) @type
                  (array_decl)* @array_decl)
                (custom_typedecl
                  (identifier) @type
                  (array_decl)* @array_decl)
              ]
              name: (identifier)+ @name)

            (variable_declaration
              (system_typedecl
                (system_type) @type
                (array_decl)* @array)
              (identifier)+ @name)

            (potential_variable_declaration
              (custom_typedecl
                (identifier) @type
                (array_decl)* @array)
              (identifier)+ @name)
            "#,
        )
        .expect("Error loading local variables query");

        let name_capture_index = query.capture_index_for_name("name").unwrap();
        let type_capture_index = query.capture_index_for_name("type").unwrap();
        let array_capture_index = query.capture_index_for_name("array").unwrap();

        let mut query_cursor = tree_sitter::QueryCursor::new();
        let matches = query_cursor.matches(&query, method_node, self.doc.line_map.text_provider());

        let vars: Vec<index::VariableSymbol> = matches.fold(Vec::new(), |mut vars, query_match| {
            if let Some(type_node) = query_match
                .nodes_for_capture_index(type_capture_index)
                .next()
            {
                let variable_type = self.doc.line_map.text_for_node(&type_node);
                if type_node.kind() != "system_type"
                    && !self.index.is_known_struct(&variable_type.clone().into())
                {
                    return vars;
                }

                let array_dimension_count = query_match
                    .nodes_for_capture_index(array_capture_index)
                    .count();
                for name_node in query_match.nodes_for_capture_index(name_capture_index) {
                    let variable_name = self.doc.line_map.text_for_node(&name_node);
                    let variable_type = if array_dimension_count == 0 {
                        index::DataFlexDataType::Simple(variable_type.clone().into())
                    } else {
                        index::DataFlexDataType::Array(
                            variable_type.clone().into(),
                            array_dimension_count,
                        )
                    };
                    vars.push(index::VariableSymbol {
                        location: name_node.start_position().into(),
                        range: name_node.range().into(),
                        symbol_path: index::SymbolPath::with_name(variable_name),
                        data_type: variable_type,
                        metadata: Vec::new(),
                    });
                }
            }
            vars
        });
        vars.into_iter()
    }

    fn resolve_class_reference(&self, position: Point) -> IndexSymbolIter<'_> {
        let Some(name) = self.doc.symbol_at_position(position) else {
            return IndexSymbolIter::empty();
        };

        IndexSymbolIter::new(
            self.index
                .find_class(&name)
                .and_then(|s| self.index.resolve_symbol(s))
                .into_iter(),
        )
    }

    fn resolve_method_reference(&self, position: Point, kind: MethodKind) -> IndexSymbolIter<'_> {
        let Some(name) = self.doc.symbol_at_position(position) else {
            return IndexSymbolIter::empty();
        };

        let member = self.resolve_call_receiver(position).and_then(|class| {
            let members: Vec<&index::IndexSymbolRef> =
                self.index.find_members(&name, kind).collect();
            self.index
                .class_hierarchy(class)
                .filter_map(|qualified_symbol| {
                    ClassSymbol::from_index_symbol(qualified_symbol.symbol)
                })
                .find_map(|class| {
                    members.iter().find(|member| {
                        member.symbol_path.parent_slice() == class.symbol_path.as_slice()
                    })
                })
                .cloned()
        });

        if let Some(member) = member {
            IndexSymbolIter::new(
                std::iter::once(member)
                    .filter_map(|member_ref| self.index.resolve_symbol(member_ref)),
            )
        } else {
            let members = self.index.find_members(&name, kind);
            IndexSymbolIter::new(
                members.filter_map(|member_ref| self.index.resolve_symbol(member_ref)),
            )
        }
    }

    fn resolve_call_receiver(&self, position: Point) -> Option<QualifiedIndexSymbol<'_>> {
        let mut cursor = self.doc.cursor()?;
        cursor
            .goto_leaf_node_at_or_after_point(position)
            .then(|| cursor.goto_enclosing_method_call());

        if cursor.is_method_call_with_dynamic_receiver() {
            // Don't try to filter on the receiver if this is `Delegate`, `Broadcast`, or `Broadcast_Focus`.
            return None;
        }

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
                        index::SymbolPath::try_from(cursor.clone())
                            .ok()
                            .map(|symbol_path| index::IndexSymbolRef {
                                file_ref: index::IndexFileRef::from(&self.doc.file_path),
                                symbol_path,
                            })
                            .and_then(|symbol_ref| self.index.resolve_symbol(&symbol_ref))
                    } else {
                        cursor
                            .node()
                            .child(0)
                            .and_then(|n| n.child_by_field_name("name"))
                            .and_then(|n| {
                                self.index
                                    .find_class(&self.doc.line_map.text_for_node(&n).into())
                            })
                            .and_then(|symbol_ref| self.index.resolve_symbol(symbol_ref))
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
                .chain(self.index.find_global_variables(&name))
                .chain(self.index.find_alias_symbols(&name))
                .filter_map(|s| self.index.resolve_symbol(s)),
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
                .chain(self.index.find_global_variables(&name))
                .chain(self.index.find_alias_symbols(&name))
                .chain(self.index.find_class(&name))
                .chain(
                    //FIXME: Filter on call receiver, like `resolve_method_reference()`.
                    self.index.find_members(&name, MethodKind::Get),
                )
                .filter_map(|s| self.index.resolve_symbol(s)),
        )
    }

    fn resolve_member_expr_reference(&self, position: Point) -> IndexSymbolIter<'_> {
        let Some(postfix_expr_node) = self.doc.cursor().and_then(|mut cursor| {
            (cursor.goto_leaf_node_at_or_before_point(position)
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
              (member_access)+ @member-access)
            "#,
        )
        .expect("Error loading member_expr query");

        let variable_capture_index = query.capture_index_for_name("variable-name").unwrap();
        let member_capture_index = query.capture_index_for_name("member-access").unwrap();

        let mut query_cursor = tree_sitter::QueryCursor::new();
        let mut matches =
            query_cursor.matches(&query, postfix_expr_node, self.doc.line_map.text_provider());

        let symbol = if let Some(query_match) = matches.next()
            && let Some(variable_name) = query_match
                .nodes_for_capture_index(variable_capture_index)
                .next()
                .map(|n| SymbolName::from(self.doc.line_map.text_for_node(&n)))
        {
            let current_symbol = self
                .resolve_type_of_variable(position, &variable_name)
                .and_then(|data_type| self.index.find_struct(data_type.name()))
                .and_then(|struct_ref| self.index.resolve_symbol(struct_ref));

            query_match
                .nodes_for_capture_index(member_capture_index)
                .take_while(|node| node.start_position() < position)
                .fold(current_symbol, |current_symbol, node| {
                    let current_symbol = if let Some(variable) = current_symbol
                        .as_ref()
                        .and_then(|cs| VariableSymbol::from_index_symbol(cs.symbol))
                    {
                        self.index
                            .find_struct(&variable.data_type.name())
                            .and_then(|struct_ref| self.index.resolve_symbol(struct_ref))
                    } else {
                        current_symbol
                    };

                    let Some(member_node) = node
                        .child_by_field_name("name")
                        .filter(|n| n.start_position() < position)
                    else {
                        return if node.end_position() >= position {
                            current_symbol
                        } else {
                            None
                        };
                    };

                    if let Some(current_symbol) = current_symbol
                        && let Some(struct_symbol) =
                            StructSymbol::from_index_symbol(current_symbol.symbol)
                    {
                        let member_name =
                            SymbolName::from(self.doc.line_map.text_for_node(&member_node));

                        struct_symbol
                            .members
                            .iter()
                            .find(|member| *member.name() == member_name)
                            .map(|member| index::QualifiedIndexSymbol {
                                file: current_symbol.file,
                                symbol: member,
                            })
                    } else {
                        None
                    }
                })
        } else {
            None
        };

        IndexSymbolIter::new(symbol.into_iter())
    }

    fn resolve_type_reference(&self, position: Point) -> IndexSymbolIter<'_> {
        let Some(name) = self.doc.symbol_at_position(position) else {
            return IndexSymbolIter::empty();
        };
        IndexSymbolIter::new(
            self.index
                .find_struct(&name)
                .into_iter()
                .chain(self.index.find_alias_symbols(&name))
                .filter_map(|s| self.index.resolve_symbol(s)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index;

    #[test]
    fn test_local_variables() {
        let test_content = r#"
Object oMyObject is a cObject
    Procedure foo
        Integer iMyInt
        String sMyStr
        Move 1 to iMyInt
        Move "hello" to sMyStr
    End_Procedure

    Procedure bar Integer iArg1 String sArg2
        Integer iMyOtherInt iMyOtherIntOnSameLine
        Move 1 to iMyOtherInt
        Move i
    End_Procedure
End_Object

Send foo of oMyObject
            "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let reference_resolver = ReferenceResolver::new(&doc);

        let mut variables = reference_resolver.local_variables(Point::new(5, 21));
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 3, column: 16 }, range: SourceRange { start: SourceLocation { line: 3, column: 16 }, end: SourceLocation { line: 3, column: 22 } }, symbol_path: SymbolPath(\"iMyInt\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 4, column: 15 }, range: SourceRange { start: SourceLocation { line: 4, column: 15 }, end: SourceLocation { line: 4, column: 21 } }, symbol_path: SymbolPath(\"sMyStr\"), data_type: DataFlexDataType(\"String\"), metadata: [] })"
        );
        assert_eq!(format!("{:?}", variables.next()), "None");

        let mut variables = reference_resolver.local_variables(Point::new(11, 23));
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 9, column: 26 }, range: SourceRange { start: SourceLocation { line: 9, column: 26 }, end: SourceLocation { line: 9, column: 31 } }, symbol_path: SymbolPath(\"iArg1\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 9, column: 39 }, range: SourceRange { start: SourceLocation { line: 9, column: 39 }, end: SourceLocation { line: 9, column: 44 } }, symbol_path: SymbolPath(\"sArg2\"), data_type: DataFlexDataType(\"String\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 10, column: 16 }, range: SourceRange { start: SourceLocation { line: 10, column: 16 }, end: SourceLocation { line: 10, column: 27 } }, symbol_path: SymbolPath(\"iMyOtherInt\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 10, column: 28 }, range: SourceRange { start: SourceLocation { line: 10, column: 28 }, end: SourceLocation { line: 10, column: 49 } }, symbol_path: SymbolPath(\"iMyOtherIntOnSameLine\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(format!("{:?}", variables.next()), "None");

        let mut variables = reference_resolver.local_variables(Point::new(12, 14));
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 9, column: 26 }, range: SourceRange { start: SourceLocation { line: 9, column: 26 }, end: SourceLocation { line: 9, column: 31 } }, symbol_path: SymbolPath(\"iArg1\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 9, column: 39 }, range: SourceRange { start: SourceLocation { line: 9, column: 39 }, end: SourceLocation { line: 9, column: 44 } }, symbol_path: SymbolPath(\"sArg2\"), data_type: DataFlexDataType(\"String\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 10, column: 16 }, range: SourceRange { start: SourceLocation { line: 10, column: 16 }, end: SourceLocation { line: 10, column: 27 } }, symbol_path: SymbolPath(\"iMyOtherInt\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 10, column: 28 }, range: SourceRange { start: SourceLocation { line: 10, column: 28 }, end: SourceLocation { line: 10, column: 49 } }, symbol_path: SymbolPath(\"iMyOtherIntOnSameLine\"), data_type: DataFlexDataType(\"Integer\"), metadata: [] })"
        );
        assert_eq!(format!("{:?}", variables.next()), "None");
    }

    #[test]
    fn test_struct_local_variables() {
        let test_content = r#"
Struct tMyStruct
End_Struct

Procedure testIt
    tMyStruct myStructVar
    tNotExistingStruct myOtherStructVar
End_Procedure
            "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let reference_resolver = ReferenceResolver::new(&doc);

        let mut variables = reference_resolver.local_variables(Point::new(5, 21));
        assert_eq!(
            format!("{:?}", variables.next()),
            "Some(VariableSymbol { location: SourceLocation { line: 5, column: 14 }, range: SourceRange { start: SourceLocation { line: 5, column: 14 }, end: SourceLocation { line: 5, column: 25 } }, symbol_path: SymbolPath(\"myStructVar\"), data_type: DataFlexDataType(\"tMyStruct\"), metadata: [] })"
        );
        assert_eq!(format!("{:?}", variables.next()), "None");
    }

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
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Class(ClassSymbol { location: SourceLocation { line: 1, column: 6 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 2, column: 9 } }, symbol_path: SymbolPath(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), mixins: [], members: [], metadata: [] }) })"
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
        let doc_content = r#"
Use test.pkg
Object oMyObject is a cMyClass
    Procedure foo
        Send testIt
    End_Procedure
End_Object
            "#;
        index::Indexer::index_test_content(doc_content, "other.pkg".into(), &index);
        let doc = DataFlexDocument::new("other.pkg".into(), doc_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol =
            reference_resolver.resolve_method_reference(Point::new(4, 16), MethodKind::Msg);
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Method(MethodSymbol { location: SourceLocation { line: 2, column: 14 }, range: SourceRange { start: SourceLocation { line: 2, column: 4 }, end: SourceLocation { line: 3, column: 17 } }, symbol_path: SymbolPath(\"cMyClass.testIt\"), kind: Msg, parameters: [], return_type: None, metadata: [] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_method_reference_with_self() {
        let test_content = r#"
Class cMyClass is a cBaseClass
End_Class

Object oMyObject is a cMyClass
    Procedure foo
    End_Procedure

    Procedure test
        Send foo
    End_Procedure
End_Object
            "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol =
            reference_resolver.resolve_method_reference(Point::new(9, 15), MethodKind::Msg);
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Method(MethodSymbol { location: SourceLocation { line: 5, column: 14 }, range: SourceRange { start: SourceLocation { line: 5, column: 4 }, end: SourceLocation { line: 6, column: 17 } }, symbol_path: SymbolPath(\"oMyObject.foo\"), kind: Msg, parameters: [], return_type: None, metadata: [] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_expr_reference_at_call_receiver() {
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
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Object(ClassSymbol { location: SourceLocation { line: 1, column: 7 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 10 } }, symbol_path: SymbolPath(\"oMyObject\"), superclass: SymbolName(\"cObject\"), mixins: [], members: [Method(MethodSymbol { location: SourceLocation { line: 2, column: 14 }, range: SourceRange { start: SourceLocation { line: 2, column: 4 }, end: SourceLocation { line: 3, column: 17 } }, symbol_path: SymbolPath(\"oMyObject.foo\"), kind: Msg, parameters: [], return_type: None, metadata: [] })], metadata: [] }) })"
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
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Variable(VariableSymbol { location: SourceLocation { line: 2, column: 11 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct.sName\"), data_type: DataFlexDataType(\"String\"), metadata: [] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");

        let mut symbol = reference_resolver.resolve_member_expr_reference(Point::new(8, 18));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Struct(StructSymbol { location: SourceLocation { line: 1, column: 7 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct\"), members: [Variable(VariableSymbol { location: SourceLocation { line: 2, column: 11 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct.sName\"), data_type: DataFlexDataType(\"String\"), metadata: [] })] }) })"
        );
    }

    #[test]
    fn test_resolve_false_member_expr_reference() {
        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    String sName
    Move myStruct.sName. to sName
End_Procedure
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_member_expr_reference(Point::new(8, 24));
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
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Variable(VariableSymbol { location: SourceLocation { line: 2, column: 11 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct.sName\"), data_type: DataFlexDataType(\"String\"), metadata: [] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");

        let mut symbol = reference_resolver.resolve_member_expr_reference(Point::new(12, 25));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Variable(VariableSymbol { location: SourceLocation { line: 6, column: 14 }, range: SourceRange { start: SourceLocation { line: 5, column: 0 }, end: SourceLocation { line: 8, column: 0 } }, symbol_path: SymbolPath(\"tMyOtherStruct.myStruct\"), data_type: DataFlexDataType(\"tMyStruct\"), metadata: [] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");

        let mut symbol = reference_resolver.resolve_member_expr_reference(Point::new(12, 32));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Struct(StructSymbol { location: SourceLocation { line: 1, column: 7 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct\"), members: [Variable(VariableSymbol { location: SourceLocation { line: 2, column: 11 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct.sName\"), data_type: DataFlexDataType(\"String\"), metadata: [] })] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_incomplete_member_expr_reference() {
        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    Move myStruct.
End_Procedure
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_member_expr_reference(Point::new(7, 18));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Struct(StructSymbol { location: SourceLocation { line: 1, column: 7 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct\"), members: [Variable(VariableSymbol { location: SourceLocation { line: 2, column: 11 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct.sName\"), data_type: DataFlexDataType(\"String\"), metadata: [] })] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_incomplete_nested_member_expr_reference() {
        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Struct tMyOtherStruct
    tMyStruct myStruct
End_Struct

Procedure test
    tMyOtherStruct myOtherStruct
    Move myOtherStruct.myStruct.
End_Procedure
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_member_expr_reference(Point::new(11, 32));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Struct(StructSymbol { location: SourceLocation { line: 1, column: 7 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct\"), members: [Variable(VariableSymbol { location: SourceLocation { line: 2, column: 11 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 4, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct.sName\"), data_type: DataFlexDataType(\"String\"), metadata: [] })] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_paren_expr_reference() {
        let test_content = r#"
Object oTest is a cObject
    Function MyMethod String sArg1 Integer iArg2 Returns Integer
    End_Function
End_Object

Integer iTest
Move (MyMethod(oTest, "test", 1234)) to iTest
            "#;

        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_paren_expr_reference(Point::new(7, 10));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Method(MethodSymbol { location: SourceLocation { line: 2, column: 13 }, range: SourceRange { start: SourceLocation { line: 2, column: 4 }, end: SourceLocation { line: 3, column: 16 } }, symbol_path: SymbolPath(\"oTest.MyMethod\"), kind: Get, parameters: [(SymbolName(\"sArg1\"), DataFlexDataType(\"String\")), (SymbolName(\"iArg2\"), DataFlexDataType(\"Integer\"))], return_type: Some(DataFlexDataType(\"Integer\")), metadata: [] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_type_reference() {
        let test_content = r#"
Struct tMyStruct
End_Struct

Procedure test tMyStruct myStruct
End_Procedure
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_type_reference(Point::new(4, 20));
        assert_eq!(
            format!("{:?}", symbol.next()),
            "Some(QualifiedIndexSymbol { file.path: \"test.pkg\", symbol: Struct(StructSymbol { location: SourceLocation { line: 1, column: 7 }, range: SourceRange { start: SourceLocation { line: 1, column: 0 }, end: SourceLocation { line: 3, column: 0 } }, symbol_path: SymbolPath(\"tMyStruct\"), members: [] }) })"
        );
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }
}
