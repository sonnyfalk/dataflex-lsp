use std::ops::{Deref, DerefMut};

use index::MethodKind;

use super::*;
use tree_sitter::{Node, TreeCursor};

pub struct CodeCompletion {}

#[derive(Debug)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
}

#[derive(Debug)]
pub enum CompletionItemKind {
    Class,
    Method,
    Property,
}

#[derive(Debug, Eq, PartialEq)]
pub enum CodeCompletionContext {
    ClassReference,
    MethodReference(MethodKind),
}

impl CodeCompletion {
    pub fn code_completion(doc: &DataFlexDocument, position: Point) -> Option<Vec<CompletionItem>> {
        let Some(context) = CodeCompletionContext::context(doc, position) else {
            return None;
        };

        let completions = match context {
            CodeCompletionContext::ClassReference => Some(Self::class_completions(doc)),
            CodeCompletionContext::MethodReference(kind) => {
                Some(Self::method_completions(doc, kind))
            }
        };

        completions
    }

    fn class_completions(doc: &DataFlexDocument) -> Vec<CompletionItem> {
        doc.index
            .get()
            .all_known_classes()
            .drain(..)
            .map(|class_name| CompletionItem {
                label: class_name.to_string(),
                kind: CompletionItemKind::Class,
            })
            .collect()
    }

    fn method_completions(doc: &DataFlexDocument, kind: index::MethodKind) -> Vec<CompletionItem> {
        match kind {
            index::MethodKind::Procedure => doc
                .index
                .get()
                .all_known_methods(kind)
                .drain(..)
                .map(|method_name| CompletionItem {
                    label: method_name.to_string(),
                    kind: CompletionItemKind::Method,
                })
                .collect(),
            index::MethodKind::Function | index::MethodKind::Set => doc
                .index
                .get()
                .all_known_methods(kind)
                .drain(..)
                .map(|method_name| CompletionItem {
                    label: method_name.to_string(),
                    kind: CompletionItemKind::Method,
                })
                .chain(
                    doc.index
                        .get()
                        .all_known_properties()
                        .drain(..)
                        .map(|property_name| CompletionItem {
                            label: property_name.to_string(),
                            kind: CompletionItemKind::Property,
                        }),
                )
                .collect(),
        }
    }
}

impl CodeCompletionContext {
    pub fn context(doc: &DataFlexDocument, position: Point) -> Option<Self> {
        let Some(root_node) = doc.tree.as_ref().map(Tree::root_node) else {
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

struct DataFlexTreeCursor<'a> {
    cursor: TreeCursor<'a>,
    doc: &'a DataFlexDocument,
}

impl<'a> DataFlexTreeCursor<'a> {
    fn new(cursor: TreeCursor<'a>, doc: &'a DataFlexDocument) -> Self {
        Self { cursor, doc }
    }

    fn goto_next_identifier_before_position(&mut self, position: &Point) -> bool {
        if self
            .cursor
            .goto_next_node_if(|n| n.kind() == "identifier" && n.end_position() < *position)
        {
            true
        } else {
            false
        }
    }

    fn goto_next_keyword_before_position(&mut self, keyword: &str, position: &Point) -> bool {
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

    fn goto_next_identifier_enclosing_position(&mut self, position: &Point) -> bool {
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

trait TreeCursorExt {
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_class_reference_context() {
        let doc = DataFlexDocument::new(
            "Object oTest is a cTest\nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(CodeCompletionContext::ClassReference));

        let doc = DataFlexDocument::new(
            "Object oTest is a cTest\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(CodeCompletionContext::ClassReference));

        let doc = DataFlexDocument::new(
            "Object oTest is a \n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(CodeCompletionContext::ClassReference));

        let doc = DataFlexDocument::new(
            "Object oTest is a \nEnd_Object\n",
            index::IndexRef::make_test_index_ref(),
        );
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 18 });
        assert_eq!(context, Some(CodeCompletionContext::ClassReference));

        let doc = DataFlexDocument::new(
            "Object oTestButton is a cWebButton\nObject oTest is a \nEnd_Object",
            index::IndexRef::make_test_index_ref(),
        );
        let context = CodeCompletionContext::context(&doc, Point { row: 1, column: 18 });
        assert_eq!(context, Some(CodeCompletionContext::ClassReference));
    }

    #[test]
    fn test_method_reference_context() {
        let doc = DataFlexDocument::new("Send Foo\n", index::IndexRef::make_test_index_ref());
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 5 });
        assert_eq!(
            context,
            Some(CodeCompletionContext::MethodReference(
                MethodKind::Procedure
            ))
        );

        let doc = DataFlexDocument::new("Send Foo\n", index::IndexRef::make_test_index_ref());
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(CodeCompletionContext::MethodReference(
                MethodKind::Procedure
            ))
        );

        let doc = DataFlexDocument::new("Get Foo\n", index::IndexRef::make_test_index_ref());
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(CodeCompletionContext::MethodReference(MethodKind::Function))
        );

        let doc = DataFlexDocument::new("Set Foo\n", index::IndexRef::make_test_index_ref());
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 6 });
        assert_eq!(
            context,
            Some(CodeCompletionContext::MethodReference(MethodKind::Set))
        );

        let doc = DataFlexDocument::new("Send Foo 1\n", index::IndexRef::make_test_index_ref());
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 9 });
        assert_eq!(context, None);

        let doc = DataFlexDocument::new("Send Foo 1\n", index::IndexRef::make_test_index_ref());
        let context = CodeCompletionContext::context(&doc, Point { row: 0, column: 4 });
        assert_eq!(context, None);
    }
}
