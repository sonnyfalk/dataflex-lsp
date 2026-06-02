use super::*;

pub struct ScopeBalancer {}

#[derive(Debug)]
pub struct TextEdit {
    pub range: std::ops::Range<tree_sitter::Point>,
    pub text: String,
}

impl ScopeBalancer {
    const fn auto_close_scope_pairs() -> &'static [(&'static str, &'static str)] {
        &[
            ("Object", "End_Object"),
            ("Class", "End_Class"),
            ("Composite", "End_Composite"),
            ("Procedure", "End_Procedure"),
            ("Function", "End_Function"),
            ("Struct", "End_Struct"),
            ("Enum_List", "End_Enum_List"),
            ("For", "Loop"),
            ("While", "Loop"),
            ("Repeat", "Until"),
            ("Case Begin", "Case End"),
            ("Begin", "End"),
        ]
    }

    const fn scope_node_pairs() -> &'static [(&'static str, &'static str)] {
        &[
            ("object_header", "object_footer"),
            ("class_header", "class_footer"),
            ("composite_header", "composite_footer"),
            ("procedure_header", "procedure_footer"),
            ("function_header", "function_footer"),
            ("struct_header", "struct_footer"),
            ("enum_header", "enum_footer"),
            ("for_header", "for_footer"),
            ("while_header", "while_footer"),
            ("repeat_header", "repeat_footer"),
            ("case_header", "case_footer"),
            ("block_header", "block_footer"),
        ]
    }

    pub fn is_auto_close_scope_trigger(text: &str) -> bool {
        text == " " || Self::is_auto_close_newline_scope_trigger(text)
    }

    fn is_auto_close_newline_scope_trigger(text: &str) -> bool {
        text.contains('\n') && text.chars().all(char::is_whitespace)
    }

    pub fn auto_close_scope(
        doc: &DataFlexDocument,
        position: Point,
        trigger_text: &str,
    ) -> Option<TextEdit> {
        let mut cursor = doc.cursor()?;
        if !cursor.goto_leaf_node_at_or_before_point(position) || cursor.node().kind() != "keyword"
        {
            return None;
        }
        let current_token = doc.line_map.text_for_node(&cursor.node());
        let prev_token = cursor
            .node()
            .prev_sibling()
            .map(|n| doc.line_map.text_for_node(&n))
            .unwrap_or_default();
        let scope_pair = Self::auto_close_scope_pairs().iter().find(|scope_pair| {
            if scope_pair.0.contains(char::is_whitespace) {
                let mut start_tokens = scope_pair.0.split_whitespace();
                start_tokens
                    .next()
                    .is_some_and(|token| token.eq_ignore_ascii_case(&prev_token))
                    && start_tokens
                        .next()
                        .is_some_and(|token| token.eq_ignore_ascii_case(&current_token))
                    && start_tokens.next().is_none()
            } else {
                scope_pair.0.eq_ignore_ascii_case(&current_token)
            }
        })?;

        if let Some(scope_node) = cursor.node().parent()
            && Self::find_balanced_corresponding_scope_node(&scope_node).is_some()
        {
            return None;
        }
        if Self::find_next_unbalanced_close_scope(scope_pair.1, doc, position).is_some() {
            return None;
        }

        let leading_whitespace = doc
            .line_map
            .line_text_with_ending(position.row)
            .map(|text| {
                text.find(|c: char| !c.is_ascii_whitespace())
                    .map(|offset| String::from(&text[..offset]))
                    .unwrap_or(text.into())
            })
            .unwrap_or_default();

        let close_scope_position = if Self::is_auto_close_newline_scope_trigger(trigger_text) {
            Point::new(position.row + 2, 0)
        } else {
            Point::new(position.row + 1, 0)
        };
        Some(TextEdit {
            range: std::ops::Range {
                start: close_scope_position,
                end: close_scope_position,
            },
            text: format!("{}{}\n", leading_whitespace, scope_pair.1),
        })
    }

    pub fn open_and_close_scope_range_pair(
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<(std::ops::Range<Point>, std::ops::Range<Point>)> {
        let mut cursor = doc.cursor()?;
        if !cursor.goto_leaf_node_at_or_before_point(position) || !cursor.goto_parent() {
            return None;
        }
        let scope_node = cursor.node();

        if let Some(balanced_scope_node) = Self::find_balanced_corresponding_scope_node(&scope_node)
        {
            if scope_node.start_position() < balanced_scope_node.start_position() {
                Some((
                    Self::scope_node_range(&scope_node),
                    Self::scope_node_range(&balanced_scope_node),
                ))
            } else {
                Some((
                    Self::scope_node_range(&balanced_scope_node),
                    Self::scope_node_range(&scope_node),
                ))
            }
        } else {
            None
        }
    }

    fn find_balanced_corresponding_scope_node<'a>(
        scope_node: &tree_sitter::Node<'a>,
    ) -> Option<tree_sitter::Node<'a>> {
        let corresponding_scope_node = Self::scope_node_pairs().iter().find_map(|node_pair| {
            let node_kind = scope_node.kind();
            if node_pair.0 == node_kind {
                Some(node_pair.1)
            } else if node_pair.1 == node_kind {
                Some(node_pair.0)
            } else {
                None
            }
        })?;
        scope_node.parent().and_then(|parent_node| {
            parent_node
                .children(&mut parent_node.walk())
                .find(|n| n.kind() == corresponding_scope_node)
        })
    }

    fn find_next_unbalanced_close_scope(
        close_scope: &str,
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<Point> {
        let node = doc.root_node()?;
        let query = tree_sitter::Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            &format!(
                "(other_command_statement (identifier) @cmd (#match? @cmd \"(?i)^{close_scope}$\"))",
            ),
        )
        .expect("Error loading query");
        let mut query_cursor = tree_sitter::QueryCursor::new();
        query_cursor.set_point_range(
            Point::new(position.row + 1, 0)..Point::new(doc.line_map.line_count(), 0),
        );

        query_cursor
            .matches(&query, node, doc.line_map.text_provider())
            .next()
            .and_then(|query_match| {
                query_match
                    .nodes_for_capture_index(query.capture_index_for_name("cmd").unwrap())
                    .next()
                    .map(|n| n.start_position())
            })
    }

    fn scope_node_range(scope_node: &tree_sitter::Node) -> std::ops::Range<tree_sitter::Point> {
        let start = scope_node.start_position();
        let end = scope_node
            .children(&mut scope_node.walk())
            .take_while(|n| n.kind() == "keyword")
            .last()
            .map(|n| n.end_position())
            .unwrap_or(scope_node.end_position());
        start..end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_close_object() {
        let test_content = r#"
    Object 
            "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 10), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    End_Object\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_class() {
        let test_content = r#"
    Class 
            "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 10), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    End_Class\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_composite() {
        let test_content = r#"
    Composite 
            "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 14), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    End_Composite\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_procedure() {
        let test_content = r#"
    Procedure 
            "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 14), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    End_Procedure\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_function() {
        let test_content = r#"
    Function 
            "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 13), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    End_Function\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_struct() {
        let test_content = r#"
    Struct 
            "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 11), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    End_Struct\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_enum_list() {
        let test_content = r#"
    Enum_List 
            "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 14), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    End_Enum_List\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_for() {
        let test_content = r#"
    For  
    "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 8), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    Loop\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_while() {
        let test_content = r#"
    while 
    "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 10), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    Loop\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_repeat() {
        let test_content = r#"
    repeat 
    "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 11), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    Until\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_case_begin() {
        let test_content = r#"
    Case Begin 
    "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 14), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    Case End\\n\" })"
        );

        let test_content = r#"
    Case Begin
        
    "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 14), "        \r\n");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 3, column: 0 }..Point { row: 3, column: 0 }, text: \"    Case End\\n\" })"
        );
    }

    #[test]
    fn test_auto_close_begin() {
        let test_content = r#"
    If foo Begin 
    "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 17), " ");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 2, column: 0 }..Point { row: 2, column: 0 }, text: \"    End\\n\" })"
        );

        let test_content = r#"
    If foo Begin
        
    "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let edits = ScopeBalancer::auto_close_scope(&doc, Point::new(1, 16), "        \r\n");
        assert_eq!(
            format!("{:?}", edits),
            "Some(TextEdit { range: Point { row: 3, column: 0 }..Point { row: 3, column: 0 }, text: \"    End\\n\" })"
        );
    }

    #[test]
    fn test_open_and_close_scope_range_pair() {
        let test_content = r#"
Object oMyObject is a cObject
    Procedure MyMethod
        If foo Begin
        End
    End_Procedure
End_Object
    "#;
        let index = index::IndexRef::make_test_index_ref();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());

        let range_pair = ScopeBalancer::open_and_close_scope_range_pair(&doc, Point::new(1, 0));
        assert_eq!(
            format!("{:?}", range_pair),
            "Some((Point { row: 1, column: 0 }..Point { row: 1, column: 6 }, Point { row: 6, column: 0 }..Point { row: 6, column: 10 }))"
        );
        let range_pair = ScopeBalancer::open_and_close_scope_range_pair(&doc, Point::new(1, 6));
        assert_eq!(
            format!("{:?}", range_pair),
            "Some((Point { row: 1, column: 0 }..Point { row: 1, column: 6 }, Point { row: 6, column: 0 }..Point { row: 6, column: 10 }))"
        );
        let range_pair = ScopeBalancer::open_and_close_scope_range_pair(&doc, Point::new(6, 0));
        assert_eq!(
            format!("{:?}", range_pair),
            "Some((Point { row: 1, column: 0 }..Point { row: 1, column: 6 }, Point { row: 6, column: 0 }..Point { row: 6, column: 10 }))"
        );
        let range_pair = ScopeBalancer::open_and_close_scope_range_pair(&doc, Point::new(6, 10));
        assert_eq!(
            format!("{:?}", range_pair),
            "Some((Point { row: 1, column: 0 }..Point { row: 1, column: 6 }, Point { row: 6, column: 0 }..Point { row: 6, column: 10 }))"
        );

        let range_pair = ScopeBalancer::open_and_close_scope_range_pair(&doc, Point::new(2, 4));
        assert_eq!(
            format!("{:?}", range_pair),
            "Some((Point { row: 2, column: 4 }..Point { row: 2, column: 13 }, Point { row: 5, column: 4 }..Point { row: 5, column: 17 }))"
        );

        let range_pair = ScopeBalancer::open_and_close_scope_range_pair(&doc, Point::new(3, 15));
        assert_eq!(
            format!("{:?}", range_pair),
            "Some((Point { row: 3, column: 15 }..Point { row: 3, column: 20 }, Point { row: 4, column: 8 }..Point { row: 4, column: 11 }))"
        );
    }
}
