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
    LocalVariable,
    GlobalVariable,
    Function,
}

impl CodeCompletion {
    pub fn code_completion(doc: &DataFlexDocument, position: Point) -> Option<Vec<CompletionItem>> {
        let Some(context) = DocumentContext::context(doc, position) else {
            return None;
        };

        let completions = match context {
            DocumentContext::ClassReference => Some(Self::class_completions(doc)),
            DocumentContext::MethodReference(kind) => Some(Self::method_completions(doc, kind)),
            DocumentContext::CallReceiverReference => Some(Self::expr_completions(doc, position)),
            DocumentContext::Expression => Some(Self::expr_completions(doc, position)),
            DocumentContext::ParenExpression => Some(Self::paren_expr_completions(doc, position)),
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

    fn expr_completions(doc: &DataFlexDocument, position: Point) -> Vec<CompletionItem> {
        Self::local_variable_completions(doc, position)
            .chain(
                doc.index
                    .get()
                    .all_known_global_variables()
                    .drain(..)
                    .map(|variable_name| CompletionItem {
                        label: variable_name.to_string(),
                        kind: CompletionItemKind::GlobalVariable,
                    }),
            )
            .chain(
                doc.index
                    .get()
                    .all_known_objects()
                    .drain(..)
                    .map(|object_name| CompletionItem {
                        label: object_name.to_string(),
                        kind: CompletionItemKind::Object,
                    }),
            )
            .collect()
    }

    fn paren_expr_completions(doc: &DataFlexDocument, position: Point) -> Vec<CompletionItem> {
        Self::local_variable_completions(doc, position)
            .chain(Self::system_functions(doc))
            .chain(
                doc.index
                    .get()
                    .all_known_global_variables()
                    .drain(..)
                    .map(|variable_name| CompletionItem {
                        label: variable_name.to_string(),
                        kind: CompletionItemKind::GlobalVariable,
                    }),
            )
            .chain(
                doc.index
                    .get()
                    .all_known_objects()
                    .drain(..)
                    .map(|object_name| CompletionItem {
                        label: object_name.to_string(),
                        kind: CompletionItemKind::Object,
                    }),
            )
            .chain(
                doc.index
                    .get()
                    .all_known_methods(MethodKind::Get)
                    .drain(..)
                    .map(|method_name| CompletionItem {
                        label: method_name.to_string(),
                        kind: CompletionItemKind::Method,
                    }),
            )
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
            .collect()
    }

    fn local_variable_completions(
        doc: &DataFlexDocument,
        position: Point,
    ) -> impl Iterator<Item = CompletionItem> {
        doc.local_variables(position)
            .map(|variable| CompletionItem {
                label: variable.symbol_path.name().to_string(),
                kind: CompletionItemKind::LocalVariable,
            })
    }

    fn system_functions(doc: &DataFlexDocument) -> impl Iterator<Item = CompletionItem> {
        doc.index
            .get()
            .all_system_functions()
            .into_iter()
            .map(|f| CompletionItem {
                label: f.to_string(),
                kind: CompletionItemKind::Function,
            })
    }
}
