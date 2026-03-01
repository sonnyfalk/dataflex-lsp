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
        self.goto_enclosing_node_kind(&["send_statement", "get_statement", "set_statement"])
    }

    pub fn goto_enclosing_object_or_class(&mut self) -> bool {
        self.goto_enclosing_node_kind(&["object_definition", "class_definition"])
    }

    pub fn is_object_definition(&self) -> bool {
        self.node().kind() == "object_definition"
    }

    pub fn is_keyword(&self, keyword: &str) -> bool {
        self.node().kind() == "keyword"
            && self.doc.line_map.text_for_node(&self.node()).to_lowercase() == keyword
    }

    pub fn is_identifier(&self) -> bool {
        self.node().kind() == "identifier"
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
    fn goto_next_node(&mut self) -> bool;
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

    fn goto_enclosing_node_kind(&mut self, kinds: &[&str]) -> bool {
        let current = self.clone();
        while self.goto_parent() {
            if kinds.contains(&self.node().kind()) {
                return true;
            }
        }

        self.reset_to(&current);
        false
    }
}
