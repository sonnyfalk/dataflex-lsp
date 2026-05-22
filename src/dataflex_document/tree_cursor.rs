use std::ops::{Deref, DerefMut};
use tree_sitter::TreeCursor;

use super::*;

pub struct DataFlexTreeCursor<'a> {
    cursor: TreeCursor<'a>,
    doc: &'a DataFlexDocument,
}

impl<'a> DataFlexTreeCursor<'a> {
    pub fn new(cursor: TreeCursor<'a>, doc: &'a DataFlexDocument) -> Self {
        Self { cursor, doc }
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
        self.goto_enclosing_node_kind(&["object_definition", "class_definition"])
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

    pub fn is_identifier(&self) -> bool {
        self.node().kind() == "identifier"
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
}

impl<'a> Clone for DataFlexTreeCursor<'a> {
    fn clone(&self) -> Self {
        Self::new(self.cursor.clone(), self.doc)
    }
}

impl<'a> Deref for DataFlexTreeCursor<'a> {
    type Target = tree_sitter::TreeCursor<'a>;

    fn deref(&self) -> &Self::Target {
        &self.cursor
    }
}

impl<'a> DerefMut for DataFlexTreeCursor<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cursor
    }
}

impl DataFlexDocument {
    pub fn cursor(&self) -> Option<DataFlexTreeCursor<'_>> {
        self.root_node()
            .map(|root_node| DataFlexTreeCursor::new(root_node.walk(), self))
    }
}

pub trait TreeCursorExt {
    fn goto_first_leaf_node_for_point(&mut self, point: Point) -> bool;
    fn goto_leaf_node_before_point(&mut self, point: Point) -> bool;
    fn goto_descendant_for_point(&mut self, point: Point) -> bool;
    fn goto_leaf_node(&mut self) -> bool;
    fn goto_next_node(&mut self) -> bool;
    fn goto_previous_node(&mut self) -> bool;
    fn goto_next_leaf_node(&mut self) -> bool;
    fn goto_previous_leaf_node(&mut self) -> bool;
    fn goto_enclosing_node_kind(&mut self, kinds: &[&str]) -> bool;
}

impl TreeCursorExt for tree_sitter::TreeCursor<'_> {
    fn goto_first_leaf_node_for_point(&mut self, point: Point) -> bool {
        if !self.goto_first_child_for_point(point).is_some() {
            return false;
        }
        loop {
            if !self.goto_first_child_for_point(point).is_some() {
                break;
            }
        }
        true
    }

    fn goto_leaf_node_before_point(&mut self, point: Point) -> bool {
        self.goto_descendant_for_point(point);
        while self.goto_last_child() {}
        while self.node().start_position() > point {
            self.goto_previous_leaf_node();
        }
        if self.node().start_position() == point {
            let current = self.clone();
            if self.goto_previous_leaf_node() && self.node().end_position() < point {
                self.reset_to(&current);
            }
        }
        true
    }

    fn goto_descendant_for_point(&mut self, point: Point) -> bool {
        let mut current = self.clone();
        let mut did_descend = false;
        loop {
            if self.goto_first_child_for_point(point).is_some()
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

    fn goto_next_node(&mut self) -> bool {
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

    fn goto_next_leaf_node(&mut self) -> bool {
        if self.goto_next_node() {
            self.goto_leaf_node();
            return true;
        }
        false
    }

    fn goto_previous_node(&mut self) -> bool {
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

    fn goto_previous_leaf_node(&mut self) -> bool {
        if self.goto_previous_node() {
            while self.goto_last_child() {}
            return true;
        }
        false
    }

    fn goto_leaf_node(&mut self) -> bool {
        let mut did_descend = false;
        while self.goto_first_child() {
            did_descend = true;
        }
        did_descend
    }

    fn goto_enclosing_node_kind(&mut self, kinds: &[&str]) -> bool {
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
