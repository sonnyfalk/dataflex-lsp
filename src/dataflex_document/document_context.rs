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
            ("keyword", "object") => Self::context_for_object_or_class(cursor, doc, position),
            ("keyword", "class") => Self::context_for_object_or_class(cursor, doc, position),
            ("keyword", "send") => Self::context_for_send(cursor, doc, position),
            ("keyword", "get") => Self::context_for_get(cursor, doc, position),
            ("keyword", "set") => Self::context_for_set(cursor, doc, position),
            _ => None,
        };

        context
    }

    fn context_for_object_or_class(
        cursor: TreeCursor,
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<Self> {
        if position <= cursor.node().end_position() {
            return None;
        }

        let mut scanner = ContextScanner::new(DataFlexTreeCursor::new(cursor, doc), position);
        if matches!(
            scanner.accept_identifier().ok()?,
            ContextScannerStatus::AtEnd
        ) {
            return None;
        }
        if matches!(
            scanner.accept_keyword("is").ok()?,
            ContextScannerStatus::AtEnd
        ) {
            return None;
        }
        if matches!(
            scanner.accept_keyword("a").ok()?,
            ContextScannerStatus::AtEnd
        ) {
            return None;
        }
        if matches!(
            scanner.accept_identifier().ok()?,
            ContextScannerStatus::AtEnd
        ) {
            return Some(Self::ClassReference);
        }

        None
    }

    fn context_for_send(
        cursor: TreeCursor,
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<Self> {
        if position <= cursor.node().end_position() {
            return None;
        }

        let mut scanner = ContextScanner::new(DataFlexTreeCursor::new(cursor, doc), position);
        if matches!(
            scanner.accept_identifier().ok()?,
            ContextScannerStatus::AtEnd
        ) {
            return Some(Self::MethodReference(MethodKind::Msg));
        }

        None
    }

    fn context_for_get(
        cursor: TreeCursor,
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<Self> {
        if position <= cursor.node().end_position() {
            return None;
        }

        let mut scanner = ContextScanner::new(DataFlexTreeCursor::new(cursor, doc), position);
        if matches!(
            scanner.accept_identifier().ok()?,
            ContextScannerStatus::AtEnd
        ) {
            return Some(Self::MethodReference(MethodKind::Get));
        }

        None
    }

    fn context_for_set(
        cursor: TreeCursor,
        doc: &DataFlexDocument,
        position: Point,
    ) -> Option<Self> {
        if position <= cursor.node().end_position() {
            return None;
        }

        let mut scanner = ContextScanner::new(DataFlexTreeCursor::new(cursor, doc), position);
        if matches!(
            scanner.accept_identifier().ok()?,
            ContextScannerStatus::AtEnd
        ) {
            return Some(Self::MethodReference(MethodKind::Set));
        }

        None
    }
}

struct ContextScanner<'a> {
    cursor: DataFlexTreeCursor<'a>,
    end: Point,
}

enum ContextScannerStatus {
    Success,
    AtEnd,
}

enum ContextScannerError {
    UnexpectedToken,
}

impl<'a> ContextScanner<'a> {
    fn new(cursor: DataFlexTreeCursor<'a>, end: Point) -> Self {
        Self { cursor, end }
    }

    fn accept_keyword(
        &mut self,
        keyword: &str,
    ) -> Result<ContextScannerStatus, ContextScannerError> {
        if !self.cursor.goto_next_node() || self.cursor.node().start_position() > self.end {
            return Ok(ContextScannerStatus::AtEnd);
        }
        if !self.cursor.is_keyword(keyword) {
            return Err(ContextScannerError::UnexpectedToken);
        }
        if self.cursor.node().end_position() >= self.end {
            return Ok(ContextScannerStatus::AtEnd);
        }
        Ok(ContextScannerStatus::Success)
    }

    fn accept_identifier(&mut self) -> Result<ContextScannerStatus, ContextScannerError> {
        if !self.cursor.goto_next_node() || self.cursor.node().start_position() > self.end {
            return Ok(ContextScannerStatus::AtEnd);
        }
        if !self.cursor.is_identifier() {
            return Err(ContextScannerError::UnexpectedToken);
        }
        if self.cursor.node().end_position() >= self.end {
            return Ok(ContextScannerStatus::AtEnd);
        }
        Ok(ContextScannerStatus::Success)
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

        let doc = DataFlexDocument::new(
            "Class cTest is a cBase\nEnd_Class\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));
    }

    #[test]
    fn test_method_reference_context() {
        let doc = DataFlexDocument::new("Send Foo\n", index::IndexRef::make_test_index_ref());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 5 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new("Send Foo\n", index::IndexRef::make_test_index_ref());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new("Get Foo\n", index::IndexRef::make_test_index_ref());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Get))
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
