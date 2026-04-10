use super::*;
use index::MethodKind;

#[derive(Debug, Eq, PartialEq)]
pub enum DocumentContext {
    ClassReference,
    MethodReference(MethodKind),
    CallReceiverReference,
    Expression,
}

struct ContextScanner<'a> {
    cursor: DataFlexTreeCursor<'a>,
    end: Point,
}

enum ContextScannerStatus {
    Continue,
    Stop,
    Yield(DocumentContext),
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
    (@rules $scanner:ident, identifier -> $action:expr) => {
        if matches!($scanner.accept_identifier().ok()?, ContextScannerStatus::Stop) {
            return Some($action);
        }
    };
    (@rules $scanner:ident, identifier, $($rest:tt)*) => {
        if matches!($scanner.accept_identifier().ok()?,  ContextScannerStatus::Stop) {
            return None;
        }
        context_scanner_match!(@rules $scanner, $($rest)*);
    };
    (@rules $scanner:ident, expr*, $($rest:tt)*) => {
        loop {
            match $scanner.accept_optional_expr().ok()? {
                ContextScannerStatus::Yield(context) => { return Some(context); }
                ContextScannerStatus::Stop => { break; }
                ContextScannerStatus::Continue => { continue; }
            };
        }
        context_scanner_match!(@rules $scanner, $($rest)*);
    };
    (@rules $scanner:ident, expr*) => {
        loop {
            match $scanner.accept_optional_expr().ok()? {
                ContextScannerStatus::Yield(context) => { return Some(context); }
                ContextScannerStatus::Stop => { break; }
                ContextScannerStatus::Continue => { continue; }
            };
        }
    };
    (@rules $scanner:ident, ($keyword:pat, $($inner_rest:tt)+)?, $($rest:tt)*) => {
        match $scanner.accept_optional_keyword_if(|kw| matches!(kw, $keyword)).ok() {
            Some(ContextScannerStatus::Stop) => { return None; }
            Some(_) => {
                context_scanner_match!(@rules $scanner, $($inner_rest)*);
            }
            None => {}
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
                context_scanner_match!(scanner, identifier, "is", "a" | "an", identifier -> Self::ClassReference)
            }
            ("keyword", "class") => {
                context_scanner_match!(scanner, identifier, "is", "a" | "an", identifier -> Self::ClassReference)
            }
            ("keyword", "send") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Msg), ("to" | "of", identifier -> Self::CallReceiverReference)?, expr*)
            }
            ("keyword", "get") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Get), ("of", identifier -> Self::CallReceiverReference)?, expr*)
            }
            ("keyword", "set") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Set), ("of", identifier -> Self::CallReceiverReference)?, "to", expr*)
            }
            _ => None,
        };

        context
    }

    pub fn can_reference_variables(&self) -> bool {
        match self {
            Self::CallReceiverReference => true,
            Self::Expression => true,
            Self::ClassReference => false,
            Self::MethodReference(_) => false,
        }
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

    fn accept_optional_expr(&mut self) -> Result<ContextScannerStatus, ContextScannerError> {
        let current = self.cursor.clone();
        if !self.cursor.goto_next_node() || self.cursor.node().start_position() > self.end {
            return Ok(ContextScannerStatus::Yield(DocumentContext::Expression));
        }
        if self.cursor.is_identifier() && self.cursor.node().end_position() >= self.end {
            return Ok(ContextScannerStatus::Yield(DocumentContext::Expression));
        }

        if self.cursor.node().end_position() >= self.end {
            self.cursor.reset_to(&current);
            return Ok(ContextScannerStatus::Stop);
        }

        // FIXME: Reject anything that's not valid expression.

        Ok(ContextScannerStatus::Continue)
    }

    fn accept_optional_keyword_if<P: Fn(&str) -> bool>(
        &mut self,
        pred: P,
    ) -> Result<ContextScannerStatus, ContextScannerError> {
        let current = self.cursor.clone();
        let result = self.accept_keyword_if(pred);
        if result.is_err() {
            self.cursor.reset_to(&current);
        }
        result
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_class_reference_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Object oTest is a cTest\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Object oTest is a \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Object oTest is a \nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Object oTestButton is a cWebButton\nObject oTest is a \nEnd_Object",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 1, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Class cTest is a cBase\nEnd_Class\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));
    }

    #[test]
    fn test_method_reference_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 5 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Get Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Get))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Set))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo 1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 9 });
        assert_eq!(context, None);

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo 1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 4 });
        assert_eq!(context, None);
    }

    #[test]
    fn test_call_receiver_reference_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo to oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::CallReceiverReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo of oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::CallReceiverReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo of oMyObj arg1 arg2\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::CallReceiverReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Get Foo of oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::CallReceiverReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo of oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::CallReceiverReference));
    }

    #[test]
    fn test_expr_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo to oMyObj arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 21 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo to oMyObj arg1 arg2 arg3 \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 26 });
        assert_eq!(context, Some(DocumentContext::Expression));
        let context = DocumentContext::context(&doc, Point { row: 0, column: 31 });
        assert_eq!(context, Some(DocumentContext::Expression));
        let context = DocumentContext::context(&doc, Point { row: 0, column: 34 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo arg1 arg2 arg3 \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 11 });
        assert_eq!(context, Some(DocumentContext::Expression));
        let context = DocumentContext::context(&doc, Point { row: 0, column: 16 });
        assert_eq!(context, Some(DocumentContext::Expression));
        let context = DocumentContext::context(&doc, Point { row: 0, column: 21 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Get Foo of oMyObj arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 20 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Get Foo arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 10 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Get Foo of oMyObj arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 20 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo of oMyObj to arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 23 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo to arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 13 });
        assert_eq!(context, Some(DocumentContext::Expression));
    }
}
