use std::ops::{Deref, DerefMut};
use tree_sitter::{Node, TreeCursor};

use super::*;

pub struct DataFlexTreeCursor<'a> {
    cursor: TreeCursor<'a>,
    doc: &'a DataFlexDocument,
}

impl<'a> DataFlexTreeCursor<'a> {
    pub fn new(cursor: TreeCursor<'a>, doc: &'a DataFlexDocument) -> Self {
        Self { cursor, doc }
    }

    pub fn goto_next_identifier_before_position(&mut self, position: &Point) -> bool {
        if self
            .cursor
            .goto_next_node_if(|n| n.kind() == "identifier" && n.end_position() < *position)
        {
            true
        } else {
            false
        }
    }

    pub fn goto_next_keyword_before_position(&mut self, keyword: &str, position: &Point) -> bool {
        if self.cursor.goto_next_node_if(|n| {
            n.kind() == "keyword"
                && n.end_position() < *position
                && self
                    .doc
                    .line_map
                    .text_for_node(n)
                    .eq_ignore_ascii_case(keyword)
        }) {
            true
        } else {
            false
        }
    }

    pub fn goto_next_identifier_enclosing_position(&mut self, position: &Point) -> bool {
        if self.cursor.goto_next_node_if(|n| {
            n.kind() == "identifier"
                && n.start_position() <= *position
                && n.end_position() >= *position
        }) {
            true
        } else {
            false
        }
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

pub trait TreeCursorExt {
    fn goto_first_leaf_node_for_point(&mut self, point: Point) -> bool;
    fn goto_next_node(&mut self) -> bool;
    fn goto_next_node_if<P: FnMut(&Node) -> bool>(&mut self, pred: P) -> bool;
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

    fn goto_next_node_if<P: FnMut(&Node) -> bool>(&mut self, mut pred: P) -> bool {
        let current = self.clone();
        if self.goto_next_node() && pred(&self.node()) {
            true
        } else {
            self.reset_to(&current);
            false
        }
    }
}
