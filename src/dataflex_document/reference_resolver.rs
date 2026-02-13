use super::*;
use index::{IndexSymbolIter, MethodKind, ReadableIndexRef};

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

        let methods = self.index.find_methods(&name, kind);
        if kind == index::MethodKind::Function || kind == index::MethodKind::Set {
            let properties = self.index.find_properties(&name);
            IndexSymbolIter::new(
                methods
                    .chain(properties)
                    .filter_map(|symbol_ref| self.index.symbol_snapshot(symbol_ref)),
            )
        } else {
            IndexSymbolIter::new(
                methods.filter_map(|symbol_ref| self.index.symbol_snapshot(symbol_ref)),
            )
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
