use tree_sitter::TreeCursor;

use super::*;
use index::MethodKind;

#[derive(Debug, Eq, PartialEq)]
pub enum DocumentContext {
    ClassReference,
    MethodReference(MethodKind),
}

impl DocumentContext {
    pub fn context(doc: &DataFlexDocument, position: Point) -> Option<Self> {
        let Some(root_node) = doc.root_node() else {
            return None;
        };
        let start_of_line = Point::new(position.row, 0);

        let mut cursor = root_node.walk();
        cursor.goto_first_leaf_node_for_point(start_of_line);

        let node = cursor.node();
        let kind = node.kind();
        let text = doc.line_map.text_for_node(&node);

        let context = match (kind, text.to_lowercase().as_str()) {
            ("keyword", "object") => Self::context_for_object(cursor, doc, position),
            ("keyword", "send") => Self::context_for_send(cursor, doc, position),
            ("keyword", "get") => Self::context_for_get(cursor, doc, position),
            ("keyword", "set") => Self::context_for_set(cursor, doc, position),
            _ => None,
        };

        context
    }

    fn context_for_object(
        cursor: TreeCursor,
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<Self> {
        let mut cursor = DataFlexTreeCursor::new(cursor, doc);

        if !cursor.goto_next_identifier_before_position(&position) {
            return None;
        }

        if !cursor.goto_next_keyword_before_position("is", &position) {
            return None;
        }

        if !cursor.goto_next_keyword_before_position("a", &position) {
            return None;
        }

        if cursor.goto_next_identifier_enclosing_position(&position) {
            return Some(Self::ClassReference);
        } else if cursor.goto_next_node() {
            if cursor.node().start_position() > position {
                return Some(Self::ClassReference);
            }
            return None;
        } else {
            return Some(Self::ClassReference);
        }
    }

    fn context_for_send(
        cursor: TreeCursor,
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<Self> {
        if position <= cursor.node().end_position() {
            return None;
        }

        let mut cursor = DataFlexTreeCursor::new(cursor, doc);

        if cursor.goto_next_identifier_enclosing_position(&position) {
            return Some(Self::MethodReference(MethodKind::Procedure));
        } else if cursor.goto_next_node() {
            if cursor.node().start_position() > position {
                return Some(Self::MethodReference(MethodKind::Procedure));
            }
            return None;
        } else {
            return Some(Self::MethodReference(MethodKind::Procedure));
        }
    }

    fn context_for_get(
        cursor: TreeCursor,
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<Self> {
        if position <= cursor.node().end_position() {
            return None;
        }

        let mut cursor = DataFlexTreeCursor::new(cursor, doc);

        if cursor.goto_next_identifier_enclosing_position(&position) {
            return Some(Self::MethodReference(MethodKind::Function));
        } else if cursor.goto_next_node() {
            if cursor.node().start_position() > position {
                return Some(Self::MethodReference(MethodKind::Function));
            }
            return None;
        } else {
            return Some(Self::MethodReference(MethodKind::Function));
        }
    }

    fn context_for_set(
        cursor: TreeCursor,
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<Self> {
        if position <= cursor.node().end_position() {
            return None;
        }

        let mut cursor = DataFlexTreeCursor::new(cursor, doc);

        if cursor.goto_next_identifier_enclosing_position(&position) {
            return Some(Self::MethodReference(MethodKind::Set));
        } else if cursor.goto_next_node() {
            if cursor.node().start_position() > position {
                return Some(Self::MethodReference(MethodKind::Set));
            }
            return None;
        } else {
            return Some(Self::MethodReference(MethodKind::Set));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_class_reference_context() {
        let doc = DataFlexDocument::new(
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "Object oTest is a cTest\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "Object oTest is a \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "Object oTest is a \nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "Object oTestButton is a cWebButton\nObject oTest is a \nEnd_Object",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 1, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));
    }

    #[test]
    fn test_method_reference_context() {
        let doc = DataFlexDocument::new("Send Foo\n", index::IndexRef::make_test_index_ref());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 5 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Procedure))
        );

        let doc = DataFlexDocument::new("Send Foo\n", index::IndexRef::make_test_index_ref());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Procedure))
        );

        let doc = DataFlexDocument::new("Get Foo\n", index::IndexRef::make_test_index_ref());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Function))
        );

        let doc = DataFlexDocument::new("Set Foo\n", index::IndexRef::make_test_index_ref());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Set))
        );

        let doc = DataFlexDocument::new("Send Foo 1\n", index::IndexRef::make_test_index_ref());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 9 });
        assert_eq!(context, None);

        let doc = DataFlexDocument::new("Send Foo 1\n", index::IndexRef::make_test_index_ref());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 4 });
        assert_eq!(context, None);
    }
}
