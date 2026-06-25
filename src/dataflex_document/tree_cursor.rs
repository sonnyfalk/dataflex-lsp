use tree_sitter::{Node, TreeCursor};

use super::*;

pub struct DataFlexTreeCursor<'a> {
    inner: TreeCursor<'a>,
    doc: &'a DataFlexDocument,
    skip_extraneous_nodes: bool,
}

impl<'a> DataFlexTreeCursor<'a> {
    pub fn new(inner: TreeCursor<'a>, doc: &'a DataFlexDocument) -> Self {
        Self {
            inner,
            doc,
            skip_extraneous_nodes: true,
        }
    }

    pub fn set_skip_extraneous_nodes(&mut self, skip_extraneous_nodes: bool) {
        self.skip_extraneous_nodes = skip_extraneous_nodes;
    }

    pub fn goto_start_of_command_for_line(&mut self, line: usize) -> bool {
        let mut cursor = self.clone();
        cursor.set_skip_extraneous_nodes(false);

        let mut start_of_line = Point::new(line, 0);
        while cursor.goto_leaf_node_preceding_point(start_of_line)
            && cursor.is_in_line_continuation()
        {
            let line = cursor.node().start_position().row;
            if start_of_line.row == line {
                break;
            }
            start_of_line.row = line;
        }
        self.goto_leaf_node_at_or_after_point(start_of_line)
    }

    pub fn goto_enclosing_method_call(&mut self) -> bool {
        self.goto_enclosing_node_kind(&[
            "send_statement",
            "get_statement",
            "set_statement",
            "web_get_statement",
            "web_set_statement",
        ])
    }

    pub fn goto_enclosing_object_or_class(&mut self) -> bool {
        self.goto_enclosing_node_kind(&[
            "object_definition",
            "class_definition",
            "composite_definition",
        ])
    }

    pub fn goto_enclosing_method_definition(&mut self) -> bool {
        self.goto_enclosing_node_kind(&["procedure_definition", "function_definition"])
    }

    pub fn goto_enclosing_paren_expression(&mut self) -> bool {
        self.goto_enclosing_node_kind(&["paren_expression"])
    }

    pub fn goto_enclosing_postfix_expression(&mut self) -> bool {
        self.goto_enclosing_node_kind(&["postfix_expression"])
    }

    pub fn goto_enclosing_member_access(&mut self) -> bool {
        self.goto_enclosing_node_kind(&["member_access"])
    }

    pub fn goto_enclosing_call_expression(&mut self) -> bool {
        self.goto_enclosing_node_kind(&["call_expression"])
    }

    pub fn is_object_definition(&self) -> bool {
        self.node().kind() == "object_definition"
    }

    pub fn is_class_definition(&self) -> bool {
        self.node().kind() == "class_definition"
    }

    pub fn is_composite_definition(&self) -> bool {
        self.node().kind() == "composite_definition"
    }

    pub fn is_object_or_class_definition(&self) -> bool {
        self.is_object_definition() || self.is_class_definition() || self.is_composite_definition()
    }

    pub fn is_identifier(&self) -> bool {
        self.node().kind() == "identifier"
    }

    pub fn is_file_path(&self) -> bool {
        self.node().kind() == "file_path"
    }

    pub fn is_dot(&self) -> bool {
        self.node().kind() == "."
    }

    pub fn is_paren_expression(&self) -> bool {
        self.node().kind() == "paren_expression"
    }

    pub fn is_postfix_expression(&self) -> bool {
        self.node().kind() == "postfix_expression"
    }

    pub fn is_call_modifier(&self) -> bool {
        self.node().kind() == "call_modifier"
    }

    pub fn is_method_call_with_dynamic_receiver(&self) -> bool {
        if matches!(
            self.node().kind(),
            "send_statement" | "get_statement" | "set_statement"
        ) {
            let mut clone = self.clone();
            clone.goto_first_child()
                && clone.is_call_modifier()
                && clone.goto_leaf_node()
                && clone.is_keyword(|kw| matches!(kw, "delegate" | "broadcast" | "broadcast_focus"))
        } else {
            false
        }
    }

    pub fn is_keyword<P: Fn(&str) -> bool>(&self, pred: P) -> bool {
        if self.node().kind() == "keyword" {
            let mut keyword = self.doc.line_map.text_for_node(&self.node());
            keyword.make_ascii_lowercase();
            pred(&keyword)
        } else {
            false
        }
    }

    pub fn is_any_keyword(&self) -> bool {
        self.node().kind() == "keyword"
    }

    pub fn is_in_line_continuation(&self) -> bool {
        self.node().kind() == "line_continuation"
            || self
                .node()
                .parent()
                .map(|n| n.kind() == "line_continuation")
                .unwrap_or(false)
    }
}

impl<'a> DataFlexTreeCursor<'a> {
    pub fn goto_leaf_node_at_or_after_point(&mut self, point: Point) -> bool {
        if !self.goto_first_child_for_point(point) {
            return false;
        }
        loop {
            if !self.goto_first_child_for_point(point) {
                break;
            }
        }
        true
    }

    pub fn goto_leaf_node_at_or_before_point(&mut self, point: Point) -> bool {
        let current = self.clone();
        self.goto_descendant_for_point(point);
        while self.goto_last_child() {}
        while self.node().start_position() > point && self.goto_previous_leaf_node() {}
        if self.node().start_position() == point {
            let current = self.clone();
            if self.goto_previous_leaf_node() && self.node().end_position() < point {
                self.reset_to(&current);
            }
        }
        if self.node().start_position() <= point {
            true
        } else {
            self.reset_to(&current);
            false
        }
    }

    pub fn goto_leaf_node_preceding_point(&mut self, point: Point) -> bool {
        let current = self.clone();
        self.goto_descendant_for_point(point);
        while self.goto_last_child() {}
        while self.node().end_position() >= point && self.goto_previous_leaf_node() {}

        if self.node().end_position() < point {
            true
        } else {
            self.reset_to(&current);
            false
        }
    }

    pub fn goto_descendant_for_point(&mut self, point: Point) -> bool {
        let mut current = self.clone();
        let mut did_descend = false;
        loop {
            if self.goto_first_child_for_point(point)
                && self.node().start_position() <= point
                && self.node().end_position() >= point
            {
                did_descend = true;
                current = self.clone();
            } else {
                self.reset_to(&current);
                break;
            }
        }
        did_descend
    }

    pub fn goto_descendant_node(&mut self, node: &Node) -> bool {
        let current = self.clone();
        while self.node() != *node {
            if !self.goto_first_child_for_point(node.start_position()) {
                self.reset_to(&current);
                return false;
            }
        }
        true
    }

    pub fn goto_next_node(&mut self) -> bool {
        if self.goto_next_sibling() {
            return true;
        }

        let current = self.clone();
        while self.goto_parent() {
            if self.goto_next_sibling() {
                return true;
            }
        }

        self.reset_to(&current);
        false
    }

    pub fn goto_next_leaf_node(&mut self) -> bool {
        if self.goto_next_node() {
            self.goto_leaf_node();
            return true;
        }
        false
    }

    pub fn goto_previous_node(&mut self) -> bool {
        if self.goto_previous_sibling() {
            return true;
        }

        let current = self.clone();
        while self.goto_parent() {
            if self.goto_previous_sibling() {
                return true;
            }
        }

        self.reset_to(&current);
        false
    }

    pub fn goto_previous_leaf_node(&mut self) -> bool {
        if self.goto_previous_node() {
            while self.goto_last_child() {}
            return true;
        }
        false
    }

    pub fn goto_leaf_node(&mut self) -> bool {
        let mut did_descend = false;
        while self.goto_first_child() {
            did_descend = true;
        }
        did_descend
    }

    pub fn goto_enclosing_node_kind(&mut self, kinds: &[&str]) -> bool {
        let current = self.clone();
        loop {
            if kinds.contains(&self.node().kind()) {
                return true;
            }
            if !self.goto_parent() {
                break;
            }
        }

        self.reset_to(&current);
        false
    }
}

impl<'a> DataFlexTreeCursor<'a> {
    pub fn node(&self) -> Node<'a> {
        self.inner.node()
    }

    pub fn goto_first_child(&mut self) -> bool {
        if self.skip_extraneous_nodes {
            let mut cursor = self.inner.clone();
            cursor
                .goto_first_child()
                .then(|| skip_extraneous_nodes_forward(cursor).map(|cursor| self.inner = cursor))
                .flatten()
                .is_some()
        } else {
            self.inner.goto_first_child()
        }
    }

    pub fn goto_last_child(&mut self) -> bool {
        if self.skip_extraneous_nodes {
            let mut cursor = self.inner.clone();
            cursor
                .goto_last_child()
                .then(|| skip_extraneous_nodes_backward(cursor).map(|cursor| self.inner = cursor))
                .flatten()
                .is_some()
        } else {
            self.inner.goto_last_child()
        }
    }

    pub fn goto_parent(&mut self) -> bool {
        self.inner.goto_parent()
    }

    pub fn goto_next_sibling(&mut self) -> bool {
        if self.skip_extraneous_nodes {
            let mut cursor = self.inner.clone();
            cursor
                .goto_next_sibling()
                .then(|| skip_extraneous_nodes_forward(cursor).map(|cursor| self.inner = cursor))
                .flatten()
                .is_some()
        } else {
            self.inner.goto_next_sibling()
        }
    }

    pub fn goto_previous_sibling(&mut self) -> bool {
        if self.skip_extraneous_nodes {
            let mut cursor = self.inner.clone();
            cursor
                .goto_previous_sibling()
                .then(|| skip_extraneous_nodes_backward(cursor).map(|cursor| self.inner = cursor))
                .flatten()
                .is_some()
        } else {
            self.inner.goto_previous_sibling()
        }
    }

    pub fn goto_first_child_for_point(&mut self, point: Point) -> bool {
        if self.skip_extraneous_nodes {
            let mut cursor = self.inner.clone();
            cursor
                .goto_first_child_for_point(point)
                .is_some()
                .then(|| skip_extraneous_nodes_forward(cursor).map(|cursor| self.inner = cursor))
                .flatten()
                .is_some()
        } else {
            self.inner.goto_first_child_for_point(point).is_some()
        }
    }

    pub fn reset_to(&mut self, cursor: &Self) {
        self.inner.reset_to(&cursor.inner);
    }
}

impl<'a> Clone for DataFlexTreeCursor<'a> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            doc: self.doc,
            skip_extraneous_nodes: self.skip_extraneous_nodes,
        }
    }
}

fn skip_extraneous_nodes_forward(mut cursor: TreeCursor) -> Option<TreeCursor> {
    while is_skippable_extraneous_node(&cursor.node()) && cursor.goto_next_sibling() {}
    if is_skippable_extraneous_node(&cursor.node()) {
        None
    } else {
        Some(cursor)
    }
}

fn skip_extraneous_nodes_backward(mut cursor: TreeCursor) -> Option<TreeCursor> {
    while is_skippable_extraneous_node(&cursor.node()) && cursor.goto_previous_sibling() {}
    if is_skippable_extraneous_node(&cursor.node()) {
        None
    } else {
        Some(cursor)
    }
}

fn is_skippable_extraneous_node(node: &Node) -> bool {
    node.is_extra() && !node.is_error() && !node.is_missing()
}

impl DataFlexDocument {
    pub fn cursor(&self) -> Option<DataFlexTreeCursor<'_>> {
        self.root_node()
            .map(|root_node| DataFlexTreeCursor::new(root_node.walk(), self))
    }
}

impl TryFrom<DataFlexTreeCursor<'_>> for index::SymbolPath {
    type Error = ();

    fn try_from(mut cursor: DataFlexTreeCursor) -> Result<Self, Self::Error> {
        if cursor.is_object_definition() {
            let mut path = vec![
                cursor
                    .node()
                    .child(0)
                    .and_then(|n| n.child_by_field_name("name"))
                    .map(|n| index::SymbolName::from(cursor.doc.line_map.text_for_node(&n)))
                    .ok_or(())?,
            ];
            while cursor.goto_parent() && cursor.is_object_or_class_definition() {
                let parent = cursor
                    .node()
                    .child(0)
                    .and_then(|n| n.child_by_field_name("name"))
                    .map(|n| index::SymbolName::from(cursor.doc.line_map.text_for_node(&n)))
                    .ok_or(())?;
                path.insert(0, parent);
            }
            Ok(path.into())
        } else {
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goto_start_of_command_for_line() {
        let index = index::IndexRef::make_test_index_ref();
        let test_content = "// Test\nSend foo\n";
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let mut cursor = doc.cursor().unwrap();
        cursor.goto_start_of_command_for_line(1);
        assert_eq!(
            format!("{:?}", cursor.node()),
            "{Node keyword (1, 0) - (1, 4)}"
        );

        let test_content = "Send ;\n foo\n";
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let mut cursor = doc.cursor().unwrap();
        cursor.goto_start_of_command_for_line(1);
        assert_eq!(
            format!("{:?}", cursor.node()),
            "{Node keyword (0, 0) - (0, 4)}"
        );

        let test_content = "Send ;\nfoo\n";
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let mut cursor = doc.cursor().unwrap();
        cursor.goto_start_of_command_for_line(1);
        assert_eq!(
            format!("{:?}", cursor.node()),
            "{Node keyword (0, 0) - (0, 4)}"
        );

        let test_content = "Send ; // Trailing comment\n foo\n";
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let mut cursor = doc.cursor().unwrap();
        cursor.goto_start_of_command_for_line(1);
        assert_eq!(
            format!("{:?}", cursor.node()),
            "{Node keyword (0, 0) - (0, 4)}"
        );
    }

    #[test]
    fn test_goto_leaf_node_at_or_before_point() {
        let index = index::IndexRef::make_test_index_ref();
        let test_content = "// Test\nSend foo\n";
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, index.clone());
        let mut cursor = doc.cursor().unwrap();
        cursor.goto_leaf_node_at_or_before_point(Point::new(0, 7));
        assert_eq!(
            format!("{:?}", cursor.node()),
            "{Node source_file (0, 0) - (2, 0)}"
        );
    }
}
