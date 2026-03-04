use super::*;
use index::MethodKind;

pub struct CodeCompletion {}

#[derive(Debug)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
}

#[derive(Debug)]
pub enum CompletionItemKind {
    Class,
    Object,
    Method,
    Property,
}

impl CodeCompletion {
    pub fn code_completion(doc: &DataFlexDocument, position: Point) -> Option<Vec<CompletionItem>> {
        let Some(context) = DocumentContext::context(doc, position) else {
            return None;
        };

        let completions = match context {
            DocumentContext::ClassReference => Some(Self::class_completions(doc)),
            DocumentContext::MethodReference(kind) => Some(Self::method_completions(doc, kind)),
            DocumentContext::CallReceiverReference => Some(Self::call_receiver_completions(doc)),
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
            MethodKind::Msg => doc
                .index
                .get()
                .all_known_methods(kind)
                .drain(..)
                .map(|method_name| CompletionItem {
                    label: method_name.to_string(),
                    kind: CompletionItemKind::Method,
                })
                .collect(),
            MethodKind::Get | MethodKind::Set => doc
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

    fn call_receiver_completions(doc: &DataFlexDocument) -> Vec<CompletionItem> {
        doc.index
            .get()
            .all_known_objects()
            .drain(..)
            .map(|object_name| CompletionItem {
                label: object_name.to_string(),
                kind: CompletionItemKind::Object,
            })
            .collect()
    }
}
