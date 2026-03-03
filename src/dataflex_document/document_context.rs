use super::*;
use index::MethodKind;

#[derive(Debug, Eq, PartialEq)]
pub enum DocumentContext {
    ClassReference,
    MethodReference(MethodKind),
}

struct ContextScanner<'a> {
    cursor: DataFlexTreeCursor<'a>,
    end: Point,
}

enum ContextScannerStatus {
    Continue,
    Stop,
}

enum ContextScannerError {
    UnexpectedToken,
}

macro_rules! context_scanner_match {
    ($scanner:ident, $($rest:tt)*) => {{
        || -> Option<DocumentContext> {
            if $scanner.end <= $scanner.cursor.node().end_position() {
                return None;
            }
            context_scanner_match!(@rules $scanner, $($rest)*);
            None
        }()
    }};

    (@rules $scanner:ident, identifier -> $action:expr, $($rest:tt)*) => {
        if matches!($scanner.accept_identifier().ok()?, ContextScannerStatus::Stop) {
            return Some($action);
        }
        context_scanner_match!(@rules $scanner, $($rest)*);
    };
    (@rules $scanner:ident, identifier, $($rest:tt)*) => {
        if matches!($scanner.accept_identifier().ok()?,  ContextScannerStatus::Stop) {
            return None;
        }
        context_scanner_match!(@rules $scanner, $($rest)*);
    };
    (@rules $scanner:ident, $keyword:pat, $($rest:tt)*) => {
        if matches!($scanner.accept_keyword_if(|kw| matches!(kw, $keyword)).ok()?, ContextScannerStatus::Stop) {
            return None;
        }
        context_scanner_match!(@rules $scanner, $($rest)*);
    };
    (@rules $scanner:ident,) => {};
}

impl DocumentContext {
    pub fn context(doc: &DataFlexDocument, position: Point) -> Option<Self> {
        let mut cursor = doc.cursor()?;
        let start_of_line = Point::new(position.row, 0);
        cursor.goto_first_leaf_node_for_point(start_of_line);

        let node = cursor.node();
        let kind = node.kind();
        let text = doc.line_map.text_for_node(&node);

        let mut scanner = ContextScanner::new(cursor, position);
        let context = match (kind, text.to_lowercase().as_str()) {
            ("keyword", "object") => {
                context_scanner_match!(scanner, identifier, "is", "a" | "an", identifier -> Self::ClassReference,)
            }
            ("keyword", "class") => {
                context_scanner_match!(scanner, identifier, "is", "a" | "an", identifier -> Self::ClassReference,)
            }
            ("keyword", "send") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Msg),)
            }
            ("keyword", "get") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Get),)
            }
            ("keyword", "set") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Set),)
            }
            _ => None,
        };

        context
    }
}

impl<'a> ContextScanner<'a> {
    fn new(cursor: DataFlexTreeCursor<'a>, end: Point) -> Self {
        Self { cursor, end }
    }

    fn accept_keyword_if<P: Fn(&str) -> bool>(
        &mut self,
        pred: P,
    ) -> Result<ContextScannerStatus, ContextScannerError> {
        if !self.cursor.goto_next_node() || self.cursor.node().start_position() > self.end {
            return Ok(ContextScannerStatus::Stop);
        }
        if !self.cursor.is_keyword(pred) {
            return Err(ContextScannerError::UnexpectedToken);
        }
        if self.cursor.node().end_position() >= self.end {
            return Ok(ContextScannerStatus::Stop);
        }
        Ok(ContextScannerStatus::Continue)
    }

    fn accept_identifier(&mut self) -> Result<ContextScannerStatus, ContextScannerError> {
        if !self.cursor.goto_next_node() || self.cursor.node().start_position() > self.end {
            return Ok(ContextScannerStatus::Stop);
        }
        if !self.cursor.is_identifier() {
            return Err(ContextScannerError::UnexpectedToken);
        }
        if self.cursor.node().end_position() >= self.end {
            return Ok(ContextScannerStatus::Stop);
        }
        Ok(ContextScannerStatus::Continue)
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
