use super::*;
use index::{
    ClassSymbol, IndexSymbolIter, IndexSymbolType, MethodKind, ReadableIndexRef, SymbolName,
};

pub struct ReferenceResolver<'a> {
    doc: &'a DataFlexDocument,
    index: ReadableIndexRef<'a>,
}

impl<'a> ReferenceResolver<'a> {
    pub fn new(doc: &'a DataFlexDocument) -> Self {
        Self {
            doc,
            index: doc.index.get(),
        }
    }

    pub fn resolve_reference(&self, position: Point) -> IndexSymbolIter<'_> {
        match DocumentContext::context(self.doc, position) {
            Some(DocumentContext::ClassReference) => self.resolve_class_reference(position),
            Some(DocumentContext::MethodReference(kind)) => {
                self.resolve_method_reference(position, kind)
            }
            _ => IndexSymbolIter::empty(),
        }
    }

    fn resolve_class_reference(&self, position: Point) -> IndexSymbolIter<'_> {
        let Some(name) = self.doc.symbol_at_position(position) else {
            return IndexSymbolIter::empty();
        };

        IndexSymbolIter::new(
            self.index
                .find_class(&name)
                .and_then(|s| self.index.symbol_snapshot(s))
                .into_iter(),
        )
    }

    fn resolve_method_reference(&self, position: Point, kind: MethodKind) -> IndexSymbolIter<'_> {
        let Some(name) = self.doc.symbol_at_position(position) else {
            return IndexSymbolIter::empty();
        };

        if let Some(class) = self.resolve_call_receiver(position) {
            let members: Vec<&index::IndexSymbolRef> =
                self.index.find_members(&name, kind).collect();
            let member = self
                .index
                .class_hierarchy(class)
                .find_map(|class| {
                    members.iter().find(|member| {
                        member
                            .symbol_path
                            .parent_name()
                            .is_some_and(|name| *name == class.name)
                    })
                })
                .cloned();
            IndexSymbolIter::new(
                member
                    .into_iter()
                    .filter_map(|member_ref| self.index.symbol_snapshot(&member_ref)),
            )
        } else {
            let members = self.index.find_members(&name, kind);
            IndexSymbolIter::new(
                members.filter_map(|member_ref| self.index.symbol_snapshot(member_ref)),
            )
        }
    }

    fn resolve_call_receiver(&self, position: Point) -> Option<&ClassSymbol> {
        let mut cursor = self.doc.cursor()?;
        cursor
            .goto_first_leaf_node_for_point(position)
            .then(|| cursor.goto_enclosing_method_call());

        let receiver = cursor
            .node()
            .child_by_field_name("receiver")
            .map(|n| self.doc.line_map.text_for_node(&n))
            .unwrap_or(String::from("self"));

        if receiver.eq_ignore_ascii_case("self") {
            cursor
                .goto_enclosing_object_or_class()
                .then(|| {
                    if cursor.is_object_definition() {
                        cursor
                            .node()
                            .child(0)
                            .and_then(|n| n.child_by_field_name("superclass"))
                            .and_then(|n| {
                                self.index.find_class(&SymbolName::from(
                                    self.doc.line_map.text_for_node(&n),
                                ))
                            })
                            .and_then(|symbol_ref| self.index.symbol_snapshot(symbol_ref))
                            .and_then(|symbol_snapshot| {
                                ClassSymbol::from_index_symbol(symbol_snapshot.symbol)
                            })
                    } else {
                        cursor
                            .node()
                            .child(0)
                            .and_then(|n| n.child_by_field_name("name"))
                            .and_then(|n| {
                                self.index.find_class(&SymbolName::from(
                                    self.doc.line_map.text_for_node(&n),
                                ))
                            })
                            .and_then(|symbol_ref| self.index.symbol_snapshot(symbol_ref))
                            .and_then(|symbol_snapshot| {
                                ClassSymbol::from_index_symbol(symbol_snapshot.symbol)
                            })
                    }
                })
                .flatten()
        } else {
            // FIXME: Handle non-self receiver.
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index;
    use std::{path::PathBuf, str::FromStr};

    #[test]
    fn test_resolve_class_reference() {
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(
            r#"
Class cMyClass is a cBaseClass
End_Class
            "#,
            PathBuf::from_str("test.pkg").unwrap(),
            &index,
        );
        let doc = DataFlexDocument::new(
            r#"
Use test.pkg
Object oMyObject is a cMyClass
End_Object
            "#,
            index.clone(),
        );

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol = reference_resolver.resolve_class_reference(Point::new(2, 25));
        assert_eq!(format!("{:?}", symbol.next()), "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: Class(ClassSymbol { location: Point { row: 1, column: 6 }, name: SymbolName(\"cMyClass\"), superclass: SymbolName(\"cBaseClass\"), members: [] }) })");
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }

    #[test]
    fn test_resolve_method_reference() {
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(
            r#"
Class cMyClass is a cBaseClass
    Procedure testIt
    End_Procedure
End_Class

Class cMyOtherClass is a cBaseClass
    Procedure testIt
    End_Procedure
End_Class
            "#,
            PathBuf::from_str("test.pkg").unwrap(),
            &index,
        );
        let doc = DataFlexDocument::new(
            r#"
Use test.pkg
Object oMyObject is a cMyClass
    Procedure foo
        Send testIt
    End_Procedure
End_Object
            "#,
            index.clone(),
        );

        let reference_resolver = ReferenceResolver::new(&doc);
        let mut symbol =
            reference_resolver.resolve_method_reference(Point::new(4, 16), MethodKind::Procedure);
        assert_eq!(format!("{:?}", symbol.next()), "Some(IndexSymbolSnapshot { path: \"test.pkg\", symbol: Method(MethodSymbol { location: Point { row: 2, column: 14 }, symbol_path: SymbolPath([SymbolName(\"cMyClass\"), SymbolName(\"testIt\")]), kind: Procedure }) })");
        assert_eq!(format!("{:?}", symbol.next()), "None");
    }
}
