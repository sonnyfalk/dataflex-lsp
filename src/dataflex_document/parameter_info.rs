use super::*;

#[derive(Debug)]
pub struct ParameterInfo {
    pub signature: String,
    pub parameters: Vec<String>,
    pub active_parameter: usize,
}

impl ParameterInfo {
    pub fn parameter_info(doc: &DataFlexDocument, position: Point) -> Option<Vec<ParameterInfo>> {
        let mut cursor = doc.cursor()?;
        if !cursor.goto_leaf_node_before_point(position) {
            return None;
        }
        if cursor.goto_enclosing_call_expression() || cursor.goto_enclosing_method_call() {
            Self::parameter_info_for_method_call(doc, position, cursor)
        } else {
            None
        }
    }

    fn parameter_info_for_method_call(
        doc: &DataFlexDocument,
        position: Point,
        cursor: DataFlexTreeCursor,
    ) -> Option<Vec<ParameterInfo>> {
        let name_position = cursor
            .node()
            .child_by_field_name("name")
            .map(|n| n.start_position())?;
        let context = DocumentContext::context(doc, name_position)?;
        let in_expression = matches!(context, DocumentContext::ParenExpression);
        let resolver = ReferenceResolver::new(doc);
        let mut dedup_set = std::collections::HashSet::new();
        let parameter_info: Vec<ParameterInfo> =
            resolver
                .resolve_reference(context, name_position)
                .filter_map(|s| {
                    let signature = s.symbol.to_string();
                    if !dedup_set.insert(signature.clone()) {
                        // Skip duplicate signature.
                        return None;
                    }
                    let parameters: Vec<String> =
                        match s.symbol {
                            index::IndexSymbol::Method(method_symbol) => method_symbol
                                .parameters
                                .iter()
                                .map(|(name, data_type)| {
                                    format!("{} {}", data_type.to_string(), name.to_string())
                                })
                                .chain(method_symbol.return_type.as_ref().map(|return_type| {
                                    format!("Returns {}", return_type.to_string())
                                }))
                                .collect(),
                            index::IndexSymbol::Property(variable_symbol) => {
                                vec![variable_symbol.to_string()]
                            }
                            _ => vec![],
                        };

                    let active_parameter = if in_expression {
                        cursor
                            .node()
                            .children(&mut cursor.node().walk())
                            .filter(|n| {
                                n.kind() == ","
                                    || n.kind() == ")"
                                    || n.child(0).filter(|n| n.kind() == ",").is_some()
                            })
                            .take_while(|n| n.end_position() <= position)
                            .skip(1)
                            .count()
                    } else {
                        cursor
                            .node()
                            .children_by_field_name("argument", &mut cursor.node().walk())
                            .chain(cursor.node().child_by_field_name("result"))
                            .take_while(|n| n.end_position() < position)
                            .count()
                    };
                    if active_parameter < parameters.len() || in_expression {
                        Some(ParameterInfo {
                            signature,
                            parameters,
                            active_parameter,
                        })
                    } else {
                        None
                    }
                })
                .collect();
        Some(parameter_info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_info_with_call_statement() {
        let test_content = r#"
Object oTest is a cObject
    Procedure MyMethod String sArg1 Integer iArg2
    End_Procedure
End_Object

Send MyMethod of oTest "test" 1234
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let parameter_info = ParameterInfo::parameter_info(&doc, Point::new(6, 23));
        assert_eq!(
            format!("{:?}", parameter_info),
            "Some([ParameterInfo { signature: \"Procedure MyMethod String sArg1 Integer iArg2\", parameters: [\"String sArg1\", \"Integer iArg2\"], active_parameter: 0 }])"
        );
        let parameter_info = ParameterInfo::parameter_info(&doc, Point::new(6, 30));
        assert_eq!(
            format!("{:?}", parameter_info),
            "Some([ParameterInfo { signature: \"Procedure MyMethod String sArg1 Integer iArg2\", parameters: [\"String sArg1\", \"Integer iArg2\"], active_parameter: 1 }])"
        );
        let parameter_info = ParameterInfo::parameter_info(&doc, Point::new(6, 35));
        assert_eq!(format!("{:?}", parameter_info), "Some([])");
    }

    #[test]
    fn test_parameter_info_with_call_expression() {
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

        let parameter_info = ParameterInfo::parameter_info(&doc, Point::new(7, 11));
        assert_eq!(
            format!("{:?}", parameter_info),
            "Some([ParameterInfo { signature: \"Function MyMethod String sArg1 Integer iArg2 Returns Integer\", parameters: [\"String sArg1\", \"Integer iArg2\", \"Returns Integer\"], active_parameter: 0 }])"
        );
        let parameter_info = ParameterInfo::parameter_info(&doc, Point::new(7, 15));
        assert_eq!(
            format!("{:?}", parameter_info),
            "Some([ParameterInfo { signature: \"Function MyMethod String sArg1 Integer iArg2 Returns Integer\", parameters: [\"String sArg1\", \"Integer iArg2\", \"Returns Integer\"], active_parameter: 0 }])"
        );
        let parameter_info = ParameterInfo::parameter_info(&doc, Point::new(7, 22));
        assert_eq!(
            format!("{:?}", parameter_info),
            "Some([ParameterInfo { signature: \"Function MyMethod String sArg1 Integer iArg2 Returns Integer\", parameters: [\"String sArg1\", \"Integer iArg2\", \"Returns Integer\"], active_parameter: 0 }])"
        );
        let parameter_info = ParameterInfo::parameter_info(&doc, Point::new(7, 30));
        assert_eq!(
            format!("{:?}", parameter_info),
            "Some([ParameterInfo { signature: \"Function MyMethod String sArg1 Integer iArg2 Returns Integer\", parameters: [\"String sArg1\", \"Integer iArg2\", \"Returns Integer\"], active_parameter: 1 }])"
        );
        let parameter_info = ParameterInfo::parameter_info(&doc, Point::new(7, 35));
        assert_eq!(
            format!("{:?}", parameter_info),
            "Some([ParameterInfo { signature: \"Function MyMethod String sArg1 Integer iArg2 Returns Integer\", parameters: [\"String sArg1\", \"Integer iArg2\", \"Returns Integer\"], active_parameter: 2 }])"
        );
    }
}
