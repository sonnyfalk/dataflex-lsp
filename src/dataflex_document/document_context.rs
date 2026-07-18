use super::*;
use index::MethodKind;

#[derive(Debug, Eq, PartialEq)]
pub enum DocumentContext {
    ClassReference,
    MethodReference(MethodKind),
    Expression,
    ParenExpression,
    DotMemberExpression,
    CommandReference,
    FileDependency,
    MethodDeclaration(MethodKind),
    TypeReference,
}

struct ContextScanner<'a> {
    cursor: DataFlexTreeCursor<'a>,
    end: Point,
}

#[derive(Debug)]
enum ContextScannerStatus {
    Continue,
    Stop,
    Yield(DocumentContext),
}

#[derive(Debug)]
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
    (@rules $scanner:ident, identifier) => {
        if matches!($scanner.accept_identifier().ok()?,  ContextScannerStatus::Stop) {
            return None;
        }
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
    (@rules $scanner:ident, expr, $($rest:tt)*) => {
        match $scanner.accept_expr().ok()? {
            ContextScannerStatus::Yield(context) => { return Some(context); }
            ContextScannerStatus::Stop => { return None; }
            ContextScannerStatus::Continue => {}
        };
        context_scanner_match!(@rules $scanner, $($rest)*);
    };
    (@rules $scanner:ident, expr) => {
        match $scanner.accept_expr().ok()? {
            ContextScannerStatus::Yield(context) => { return Some(context); }
            ContextScannerStatus::Stop => { return None; }
            ContextScannerStatus::Continue => {}
        };
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
    (@rules $scanner:ident, ($keyword:pat)?, $($rest:tt)*) => {
        match $scanner.accept_optional_keyword_if(|kw| matches!(kw, $keyword)).ok() {
            Some(ContextScannerStatus::Stop) => { return None; }
            _ => {}
        }
        context_scanner_match!(@rules $scanner, $($rest)*);
    };
    (@rules $scanner:ident, typedecl, $($rest:tt)*) => {
        if matches!($scanner.accept_typedecl().ok()?, ContextScannerStatus::Stop) {
            if $scanner.cursor.node().child_by_field_name("name").filter(|n| !n.is_missing()).is_some_and(|n| n.end_position() < $scanner.end) {
                return None;
            }
            return Some(DocumentContext::TypeReference);
        }
        context_scanner_match!(@rules $scanner, $($rest)*);
    };
    (@rules $scanner:ident, (typedecl, $($inner_rest:tt)+)*, $($rest:tt)*) => {
        while let Ok(status) = $scanner.accept_optional_typedecl() {
            if matches!(status, ContextScannerStatus::Stop) {
                if $scanner.cursor.node().child_by_field_name("name").filter(|n| !n.is_missing()).is_some_and(|n| n.end_position() < $scanner.end) {
                    return None;
                }
                return Some(DocumentContext::TypeReference);
            }
            context_scanner_match!(@rules $scanner, $($inner_rest)*);
        }
        context_scanner_match!(@rules $scanner, $($rest)*);
    };
    (@rules $scanner:ident, (typedecl, $($inner_rest:tt)+)*) => {
        while let Ok(status) = $scanner.accept_optional_typedecl() {
            if matches!(status, ContextScannerStatus::Stop) {
                if $scanner.cursor.node().child_by_field_name("name").filter(|n| !n.is_missing()).is_some_and(|n| n.end_position() < $scanner.end) {
                    return None;
                }
                return Some(DocumentContext::TypeReference);
            }
            context_scanner_match!(@rules $scanner, $($inner_rest)*);
        }
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
        if let Some(context) = Self::dot_member_context(doc, position) {
            return Some(context);
        }
        if let Some(context) = Self::paren_expr_context(doc, position) {
            return Some(context);
        }

        let mut cursor = doc.cursor()?;
        cursor.goto_start_of_command_for_line(position.row);

        let scanner = ContextScanner::new(cursor, position);
        Self::context_with_scanner(scanner, doc)
    }

    pub fn can_reference_variables(&self) -> bool {
        match self {
            Self::Expression => true,
            Self::ParenExpression => true,
            Self::ClassReference => false,
            Self::MethodReference(_) => false,
            Self::DotMemberExpression => false,
            Self::CommandReference => false,
            Self::FileDependency => false,
            Self::MethodDeclaration(_) => false,
            Self::TypeReference => false,
        }
    }

    pub fn can_reference_tables(&self) -> bool {
        match self {
            Self::ClassReference => false,
            Self::MethodReference(_) => false,
            Self::Expression => true,
            Self::ParenExpression => true,
            Self::DotMemberExpression => true,
            Self::CommandReference => false,
            Self::FileDependency => false,
            Self::MethodDeclaration(_) => false,
            Self::TypeReference => false,
        }
    }

    pub fn is_file_reference(&self) -> bool {
        match self {
            Self::FileDependency => true,
            Self::ClassReference => false,
            Self::MethodReference(_) => false,
            Self::Expression => false,
            Self::ParenExpression => false,
            Self::DotMemberExpression => false,
            Self::CommandReference => false,
            Self::MethodDeclaration(_) => false,
            Self::TypeReference => false,
        }
    }

    fn context_with_scanner(mut scanner: ContextScanner, doc: &DataFlexDocument) -> Option<Self> {
        if scanner.cursor.node().end_position() >= scanner.end {
            return Some(Self::CommandReference);
        }

        let node = scanner.cursor.node();
        let kind = node.kind();
        let text = doc.line_map.text_for_node(&node).to_lowercase();

        let context = match (kind, text.as_str()) {
            ("keyword", "object") => {
                context_scanner_match!(scanner, identifier, "is", "a" | "an", identifier -> Self::ClassReference)
            }
            ("keyword", "class") => {
                context_scanner_match!(scanner, identifier, "is", "a" | "an", identifier -> Self::ClassReference)
            }
            ("keyword", "composite") => {
                context_scanner_match!(scanner, identifier, "is", "a" | "an", identifier -> Self::ClassReference)
            }
            ("keyword", "import_class_protocol") => {
                context_scanner_match!(scanner, identifier -> Self::ClassReference)
            }
            ("keyword", "send") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Msg), expr*)
            }
            ("keyword", "get") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Get), expr*)
            }
            ("keyword", "set") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Set), expr*)
            }
            ("keyword", "webget") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Get), expr*)
            }
            ("keyword", "webset") => {
                context_scanner_match!(scanner, identifier -> Self::MethodReference(MethodKind::Set), expr*)
            }
            ("keyword", "move") => {
                context_scanner_match!(scanner, expr, "to", expr)
            }
            ("keyword", "use") => {
                context_scanner_match!(scanner, identifier -> Self::FileDependency)
            }
            ("keyword", "function") => {
                context_scanner_match!(scanner, identifier -> Self::MethodDeclaration(MethodKind::Get), ("global")?, (typedecl, ("byref")?, identifier)*, "returns", typedecl,)
            }
            ("keyword", "procedure") => {
                let method_kind = if scanner
                    .accept_optional_keyword_if(|kw| matches!(kw, "set"))
                    .is_ok_and(|s| matches!(s, ContextScannerStatus::Continue))
                {
                    MethodKind::Set
                } else {
                    MethodKind::Msg
                };
                context_scanner_match!(scanner, identifier -> Self::MethodDeclaration(method_kind), ("global")?, (typedecl, ("byref")?, identifier)*)
            }
            ("keyword", "property") => {
                context_scanner_match!(scanner, typedecl, identifier, expr)
            }
            ("keyword", "if") => {
                let context =
                    context_scanner_match!(scanner, expr, identifier -> Self::CommandReference);
                if context.is_none() && scanner.cursor.node().end_position() < scanner.end {
                    Self::context_with_scanner(scanner, doc)
                } else {
                    context
                }
            }
            ("keyword", "else") => {
                let context = context_scanner_match!(scanner, identifier -> Self::CommandReference);
                if context.is_none() && scanner.cursor.node().end_position() < scanner.end {
                    Self::context_with_scanner(scanner, doc)
                } else {
                    context
                }
            }
            ("keyword", "forward" | "delegate") => {
                scanner.cursor.goto_next_leaf_node();
                Self::context_with_scanner(scanner, doc)
            }
            ("keyword", "broadcast" | "broadcast_focus") => {
                _ = scanner
                    .accept_optional_keyword_if(|kw| matches!(kw, "recursive" | "recursive_up"));
                _ = scanner.accept_optional_keyword_if(|kw| matches!(kw, "no_stop"));
                scanner.cursor.goto_next_leaf_node();
                Self::context_with_scanner(scanner, doc)
            }
            ("keyword", "deferred_view") => {
                _ = context_scanner_match!(scanner, identifier, "for",);
                scanner.cursor.goto_next_leaf_node();
                Self::context_with_scanner(scanner, doc)
            }
            // Default fallback to recognize expression context as appropriate for all other commands.
            _ => context_scanner_match!(scanner, expr*),
        };

        context
    }

    fn dot_member_context(doc: &DataFlexDocument, position: Point) -> Option<Self> {
        let mut cursor = doc.cursor()?;
        if cursor.goto_leaf_node_at_or_before_point(position)
            && cursor.goto_enclosing_member_access()
        {
            Some(Self::DotMemberExpression)
        } else {
            None
        }
    }

    fn paren_expr_context(doc: &DataFlexDocument, position: Point) -> Option<Self> {
        let mut cursor = doc.cursor()?;
        if cursor.goto_descendant_for_point(position) && cursor.goto_enclosing_paren_expression() {
            Some(Self::ParenExpression)
        } else {
            None
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
        if !self.cursor.goto_next_leaf_node()
            || self.cursor.node().is_missing()
            || self.cursor.node().is_error()
            || self.cursor.node().start_position() > self.end
        {
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
        if !self.cursor.goto_next_leaf_node()
            || self.cursor.node().is_missing()
            || self.cursor.node().is_error()
            || self.cursor.node().start_position() > self.end
        {
            return Ok(ContextScannerStatus::Stop);
        }
        if !self.cursor.is_identifier()
            && !self.cursor.is_any_keyword()
            && !self.cursor.is_file_path()
        {
            return Err(ContextScannerError::UnexpectedToken);
        }
        if self.cursor.node().end_position() >= self.end {
            return Ok(ContextScannerStatus::Stop);
        }
        Ok(ContextScannerStatus::Continue)
    }

    fn accept_typedecl(&mut self) -> Result<ContextScannerStatus, ContextScannerError> {
        if !self.cursor.goto_next_node() || self.cursor.node().start_position() > self.end {
            return Ok(ContextScannerStatus::Stop);
        }
        if !self.cursor.goto_descendant_typedecl() {
            return Err(ContextScannerError::UnexpectedToken);
        }
        if self.cursor.node().end_position() >= self.end
            || self.cursor.node().start_position() == self.cursor.node().end_position()
        {
            return Ok(ContextScannerStatus::Stop);
        }
        Ok(ContextScannerStatus::Continue)
    }

    fn accept_optional_typedecl(&mut self) -> Result<ContextScannerStatus, ContextScannerError> {
        let current = self.cursor.clone();
        let result = self.accept_typedecl();
        if result.is_err() {
            self.cursor.reset_to(&current);
        }
        result
    }

    fn accept_expr(&mut self) -> Result<ContextScannerStatus, ContextScannerError> {
        log::trace!("accept_optional_expr() after {:?}", self.cursor.node());
        if !self.cursor.goto_next_node() {
            log::trace!(
                "accept_optional_expr() - Ok(Yield) with {:?}",
                self.cursor.node()
            );
            return Ok(ContextScannerStatus::Yield(DocumentContext::Expression));
        }

        if (self.cursor.is_paren_expression() || self.cursor.is_postfix_expression())
            && self.cursor.node().end_position() < self.end
        {
            log::trace!(
                "accept_optional_expr() - Ok(Continue) with {:?}",
                self.cursor.node()
            );
            return Ok(ContextScannerStatus::Continue);
        }

        self.cursor.goto_leaf_node();
        if self.cursor.node().is_missing()
            || self.cursor.node().is_error()
            || self.cursor.node().start_position() > self.end
        {
            log::trace!(
                "accept_optional_expr() - Ok(Yield) with {:?}",
                self.cursor.node()
            );
            return Ok(ContextScannerStatus::Yield(DocumentContext::Expression));
        }

        if self.cursor.is_identifier() && self.cursor.node().end_position() >= self.end {
            log::trace!(
                "accept_optional_expr() - Ok(Yield) with {:?}",
                self.cursor.node()
            );
            return Ok(ContextScannerStatus::Yield(DocumentContext::Expression));
        }

        if self.cursor.node().end_position() >= self.end {
            log::trace!(
                "accept_optional_expr() - Ok(Stop) with {:?}",
                self.cursor.node()
            );
            return Ok(ContextScannerStatus::Stop);
        }

        // FIXME: Reject anything that's not valid expression.

        log::trace!(
            "accept_optional_expr() - Ok(Continue) with {:?}",
            self.cursor.node()
        );
        Ok(ContextScannerStatus::Continue)
    }

    fn accept_optional_expr(&mut self) -> Result<ContextScannerStatus, ContextScannerError> {
        let current = self.cursor.clone();
        let result = self.accept_expr();
        if result.is_err()
            || result
                .as_ref()
                .is_ok_and(|s| matches!(s, ContextScannerStatus::Stop))
        {
            self.cursor.reset_to(&current);
        }
        result
    }

    fn accept_optional_keyword_if<P: Fn(&str) -> bool>(
        &mut self,
        pred: P,
    ) -> Result<ContextScannerStatus, ContextScannerError> {
        let current = self.cursor.clone();
        let result = self.accept_keyword_if(pred);
        if result.is_err()
            || self.cursor.node().is_missing()
            || self.cursor.node().is_error()
            || self.cursor.node().start_position() > self.end
        {
            self.cursor.reset_to(&current);
            Err(ContextScannerError::UnexpectedToken)
        } else {
            result
        }
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
            "Deferred_View Activate_oMyView for ;\nObject oMyView is a cView\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 1, column: 23 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Deferred_View Activate_oMyView for ;\nObject oMyView is a \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 1, column: 20 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Class cTest is a cBase\nEnd_Class\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Class cTest is a cBase\nImport_Class_Protocol \nEnd_Class\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 1, column: 22 });
        assert_eq!(context, Some(DocumentContext::ClassReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Class cTest is a cBase\nImport_Class_Protocol cMyMixin\nEnd_Class\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 1, column: 23 });
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
            "WebGet Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 9 });
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
            "WebSet Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 9 });
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
        assert_eq!(context, Some(DocumentContext::CommandReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Forward Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Delegate Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 15 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Broadcast Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 16 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Broadcast Recursive Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 26 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Broadcast Recursive_Up Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 29 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Broadcast No_Stop Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 24 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Broadcast Recursive No_Stop Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 34 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Broadcast_Focus Send Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 22 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Private.Foo\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 8 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );
    }

    #[test]
    fn test_call_receiver_reference_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo to oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo of oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Send Foo of oMyObj arg1 arg2\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Get Foo of oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo of oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "WebGet Foo of oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 17 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "WebSet Foo of oMyObj\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 17 });
        assert_eq!(context, Some(DocumentContext::Expression));
    }

    #[test]
    fn test_call_expr_context() {
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
            "Get Foo of (oMyObj) arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 22 });
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

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo arg1 to arg2\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 10 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "WebGet Foo of oMyObj arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 23 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "WebGet Foo arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 13 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "WebGet Foo of oMyObj arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 23 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "WebGet Foo of (oMyObj) arg1\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 25 });
        assert_eq!(context, Some(DocumentContext::Expression));
    }

    #[test]
    fn test_incomplete_call_expr() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set \nSend OtherMessage\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 4 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Set))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 8 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo \nSet OtherMessage\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 8 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo arg1 \nSet OtherMessage\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 13 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo of \nSet OtherMessage\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 11 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo arg1 to \nSet OtherMessage\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 16 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo of oMyObj \nSet OtherMessage\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo of oMyObj to \nSet OtherMessage\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 21 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo of oMyObj arg1 \nSet OtherMessage\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 23 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Set Foo of oMyObj arg1 to \nSet OtherMessage\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 26 });
        assert_eq!(context, Some(DocumentContext::Expression));
    }

    #[test]
    fn test_move_expr_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move iMyInt to iMyOtherInt\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 8 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let context = DocumentContext::context(&doc, Point { row: 0, column: 13 });
        assert_eq!(context, None);

        let context = DocumentContext::context(&doc, Point { row: 0, column: 20 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move iMyInt to \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 15 });
        assert_eq!(context, Some(DocumentContext::Expression));
    }

    #[test]
    fn test_paren_expr_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move (iMyInt) to iMyOtherInt\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 10 });
        assert_eq!(context, Some(DocumentContext::ParenExpression));
    }

    #[test]
    fn test_dot_member_expr_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move shippingAddress.iZipCode to iZipCode\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(context, Some(DocumentContext::Expression));
        let context = DocumentContext::context(&doc, Point { row: 0, column: 24 });
        assert_eq!(context, Some(DocumentContext::DotMemberExpression));
        let context = DocumentContext::context(&doc, Point { row: 0, column: 21 });
        assert_eq!(context, Some(DocumentContext::DotMemberExpression));
        let context = DocumentContext::context(&doc, Point { row: 0, column: 36 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move shippingAddress.iZipCode\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 24 });
        assert_eq!(context, Some(DocumentContext::DotMemberExpression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move shippingAddress.\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 21 });
        assert_eq!(context, Some(DocumentContext::DotMemberExpression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move iZipCode to shippingAddress.iZipCode\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 8 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move iZipCode to shippingAddress.iZipCode\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 24 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move iZipCode to shippingAddress.iZipCode\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 37 });
        assert_eq!(context, Some(DocumentContext::DotMemberExpression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move iZipCode to shippingAddress\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 26 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move iZipCode to shippingAddress.\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 33 });
        assert_eq!(context, Some(DocumentContext::DotMemberExpression));
        let context = DocumentContext::context(&doc, Point { row: 0, column: 32 });
        assert_eq!(context, Some(DocumentContext::Expression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move iZipCode to shippingAddress.i\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 34 });
        assert_eq!(context, Some(DocumentContext::DotMemberExpression));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move iZipCode to order.shippingAddress.\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 39 });
        assert_eq!(context, Some(DocumentContext::DotMemberExpression));
    }

    #[test]
    fn test_command_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Move\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 4 });
        assert_eq!(context, Some(DocumentContext::CommandReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 0 });
        assert_eq!(context, Some(DocumentContext::CommandReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "If bOk \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 7 });
        assert_eq!(context, Some(DocumentContext::CommandReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "If bOk Move 1 to iVar\nElse \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 1, column: 5 });
        assert_eq!(context, Some(DocumentContext::CommandReference));
        let context = DocumentContext::context(&doc, Point { row: 0, column: 11 });
        assert_eq!(context, Some(DocumentContext::CommandReference));
    }

    #[test]
    fn test_file_dependency_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Use \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 4 });
        assert_eq!(context, Some(DocumentContext::FileDependency));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Use SomeFile.pkg\nUse OtherFile.pkg\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 8 });
        assert_eq!(context, Some(DocumentContext::FileDependency));
    }

    #[test]
    fn test_method_decl_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Procedure \nEnd_Procedure\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 10 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodDeclaration(MethodKind::Msg))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Procedure Set \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 14 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodDeclaration(MethodKind::Set))
        );

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Function \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 9 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodDeclaration(MethodKind::Get))
        );
    }

    #[test]
    fn test_parameter_type_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Procedure SayHello \nEnd_Procedure\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 19 });
        assert_eq!(context, Some(DocumentContext::TypeReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Procedure SayHello I\nEnd_Procedure\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 20 });
        assert_eq!(context, Some(DocumentContext::TypeReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Procedure SayHello Integer \nEnd_Procedure\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 27 });
        assert_eq!(context, None);

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Procedure SayHello Integer i\nEnd_Procedure\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 28 });
        assert_eq!(context, None);

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Procedure SayHello Integer iArg1 \nEnd_Procedure\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 33 });
        assert_eq!(context, Some(DocumentContext::TypeReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Procedure SayHello Integer iArg1 I\nEnd_Procedure\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 34 });
        assert_eq!(context, Some(DocumentContext::TypeReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Function SayHello \nEnd_Function\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(DocumentContext::TypeReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Function SayHello Integer iArg1 \nEnd_Function\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 32 });
        assert_eq!(context, Some(DocumentContext::TypeReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Function SayHello Integer ByRef iArg1 \nEnd_Function\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 38 });
        assert_eq!(context, Some(DocumentContext::TypeReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Function SayHello Integer iArg1 Returns \nEnd_Function\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 40 });
        assert_eq!(context, Some(DocumentContext::TypeReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Function SayHello Global \nEnd_Function\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 25 });
        assert_eq!(context, Some(DocumentContext::TypeReference));
    }

    #[test]
    fn test_property_context() {
        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Property \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 9 });
        assert_eq!(context, Some(DocumentContext::TypeReference));

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Property String \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 16 });
        assert_eq!(context, None);

        let doc = DataFlexDocument::new(
            "test.pkg".into(),
            "Property String psName \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = DocumentContext::context(&doc, Point { row: 0, column: 23 });
        assert_eq!(context, Some(DocumentContext::Expression));
    }

    #[test]
    fn test_context_with_inline_comment() {
        let test_content = "Move 1234 /* 123 test comment */ to iMyVar\n";
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let context = DocumentContext::context(&doc, Point { row: 0, column: 38 });
        assert_eq!(context, Some(DocumentContext::Expression));
    }

    #[test]
    fn test_context_with_line_continuation() {
        let test_content = "Send ;\n foo\n";
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let context = DocumentContext::context(&doc, Point { row: 1, column: 3 });
        assert_eq!(
            context,
            Some(DocumentContext::MethodReference(MethodKind::Msg))
        );
    }
}
