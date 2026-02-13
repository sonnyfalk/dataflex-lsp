use super::*;
use index::{IndexSymbolIter, ReadableIndexRef};

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
        let Some(tree) = self.doc.tree.as_ref() else {
            return IndexSymbolIter::empty();
        };
        let Some(node) = tree
            .root_node()
            .descendant_for_point_range(position, position)
        else {
            return IndexSymbolIter::empty();
        };
        let name = index::SymbolName::from(
            self.doc
                .line_map
                .text_in_range(node.start_position(), node.end_position()),
        );

        match DocumentContext::context(self.doc, position) {
            Some(DocumentContext::ClassReference) => IndexSymbolIter::new(
                self.index
                    .find_class(&name)
                    .and_then(|s| self.index.symbol_snapshot(s))
                    .into_iter(),
            ),
            Some(DocumentContext::MethodReference(kind)) => {
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
            _ => IndexSymbolIter::empty(),
        }
    }
}
