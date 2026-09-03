use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use super::*;
use index::{IndexSymbolType, MethodKind, MethodSymbol, StructSymbol, SymbolName};

pub struct CodeCompletion {}

#[derive(Debug, Default)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub details: Option<String>,
    pub insert_text: Option<String>,
}

#[derive(Debug, Default)]
pub enum CompletionItemKind {
    #[default]
    Text,
    Class,
    LocalObject,
    TopLevelObject,
    OtherObject,
    Method,
    Property,
    LocalVariable,
    GlobalVariable,
    Function,
    StructMember,
    EnumMember,
    TableName,
    TableColumn,
    Command,
    File,
    Struct,
}

pub struct CompletionItemRanker<'a> {
    doc: &'a DataFlexDocument,
    index: &'a index::Index,
    position: Point,
    likely_enum_symbols: std::sync::OnceLock<HashMap<String, CompletionItemRankAdjustment>>,
    likely_commands: std::sync::OnceLock<HashMap<String, CompletionItemRankAdjustment>>,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum CompletionItemRank {
    Top = 0,
    NearTop = 1,
    UpperMid = 4,
    Mid = 5,
    NearBottom = 8,
    Bottom = 9,
}

#[derive(Debug, Clone, Copy, Default)]
#[allow(dead_code)]
enum CompletionItemRankAdjustment {
    #[default]
    None,
    Up,
    Down,
    Top,
    Bottom,
}

impl CodeCompletion {
    pub fn code_completion(
        doc: &DataFlexDocument,
        position: Point,
        auto_complete: bool,
        index: &index::Index,
    ) -> Option<Vec<CompletionItem>> {
        let context = DocumentContext::context(doc, position)?;
        if auto_complete && !Self::should_auto_complete_with_context(&context) {
            return None;
        }

        let completions = match context {
            DocumentContext::ClassReference => Some(Self::class_completions(index)),
            DocumentContext::MethodReference(kind) => {
                Some(Self::method_completions(doc, position, kind, index))
            }
            DocumentContext::Expression => Some(Self::expr_completions(doc, position, index)),
            DocumentContext::ParenExpression => {
                Some(Self::paren_expr_completions(doc, position, index))
            }
            DocumentContext::DotMemberExpression => {
                Some(Self::dot_completions(doc, position, index))
            }
            DocumentContext::CommandReference => Some(Self::command_completions(index)),
            DocumentContext::FileDependency => Some(Self::file_completions(index)),
            DocumentContext::MethodDeclaration(kind) => {
                Some(Self::override_completions(doc, position, kind, index))
            }
            DocumentContext::TypeReference => Some(Self::type_completions(index)),
        };

        completions
    }

    fn should_auto_complete_with_context(context: &DocumentContext) -> bool {
        match context {
            DocumentContext::ClassReference => true,
            DocumentContext::MethodReference(_) => true,
            DocumentContext::DotMemberExpression => true,
            DocumentContext::FileDependency => true,
            DocumentContext::Expression => false,
            DocumentContext::ParenExpression => false,
            DocumentContext::CommandReference => false,
            DocumentContext::MethodDeclaration(_) => false,
            DocumentContext::TypeReference => false,
        }
    }

    fn class_completions(index: &index::Index) -> Vec<CompletionItem> {
        index
            .all_known_classes()
            .drain(..)
            .map(|class_name| CompletionItem {
                label: class_name.to_string(),
                kind: CompletionItemKind::Class,
                ..Default::default()
            })
            .collect()
    }

    fn method_completions(
        doc: &DataFlexDocument,
        position: Point,
        kind: index::MethodKind,
        index: &index::Index,
    ) -> Vec<CompletionItem> {
        let completions: Vec<CompletionItem> =
            match kind {
                MethodKind::Msg => index
                    .all_known_methods(kind)
                    .drain(..)
                    .map(|method_name| CompletionItem {
                        label: method_name.to_string(),
                        kind: CompletionItemKind::Method,
                        ..Default::default()
                    })
                    .collect(),
                MethodKind::Get | MethodKind::Set => index
                    .all_known_methods(kind)
                    .drain(..)
                    .map(|method_name| CompletionItem {
                        label: method_name.to_string(),
                        kind: CompletionItemKind::Method,
                        ..Default::default()
                    })
                    .chain(index.all_known_properties().drain(..).map(|property_name| {
                        CompletionItem {
                            label: property_name.to_string(),
                            kind: CompletionItemKind::Property,
                            ..Default::default()
                        }
                    }))
                    .collect(),
            };

        if let Some(mut cursor) = doc.cursor()
            && cursor.goto_leaf_node_at_or_before_point(position)
            && let Some(filter_text) = cursor
                .is_identifier()
                .then(|| doc.line_map.text_for_node(&cursor.node()))
                .filter(|text| text.contains('.'))
            && let Some(filter_text) = filter_text.rfind('.').map(|indx| &filter_text[..=indx])
        {
            // Code completion with embedded dot, e.g. Send Private.MyMethod.
            // Filter out prefix before the last dot, since code completion context is after the dot.
            completions
                .into_iter()
                .filter_map(|mut cc| {
                    if cc.label.len() >= filter_text.len()
                        && cc.label[..filter_text.len()].eq_ignore_ascii_case(filter_text)
                    {
                        cc.label = cc.label[filter_text.len()..].into();
                        Some(cc)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            completions
        }
    }

    fn override_completions(
        doc: &DataFlexDocument,
        position: Point,
        kind: index::MethodKind,
        index: &index::Index,
    ) -> Vec<CompletionItem> {
        if let Some(mut cursor) = doc.cursor()
            && cursor.goto_descendant_for_point(position)
            && cursor.goto_enclosing_object_or_class()
        {
            let superclass = cursor
                .node()
                .child(0)
                .and_then(|n| n.child_by_field_name("superclass"))
                .map(|n| doc.line_map.text_for_node(&n))
                .and_then(|superclass_name| index.find_class(&superclass_name.into()))
                .and_then(|symbol_ref| index.resolve_symbol(symbol_ref));

            // TODO: Filter out already overridden methods.
            superclass
                .into_iter()
                .flat_map(|superclass| index.inherited_class_members(superclass, kind))
                .map(|m| {
                    let mut details = String::new();
                    if let Some(method_symbol) = MethodSymbol::from_index_symbol(m.symbol) {
                        for (name, data_type) in &method_symbol.parameters {
                            _ = write!(details, " {} {}", data_type, name);
                        }
                        if let Some(return_type) = &method_symbol.return_type {
                            _ = write!(details, " Returns {}", return_type);
                        }
                    }
                    CompletionItem {
                        label: m.symbol.name().to_string(),
                        kind: CompletionItemKind::Method,
                        details: Some(details.clone()),
                        insert_text: Some(format!("{}{}\n    ", m.symbol.name(), details)),
                        ..Default::default()
                    }
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn expr_completions(
        doc: &DataFlexDocument,
        position: Point,
        index: &index::Index,
    ) -> Vec<CompletionItem> {
        Self::local_variable_completions(doc, position, index)
            .chain(
                index
                    .all_known_global_variables()
                    .drain(..)
                    .map(|variable_name| CompletionItem {
                        label: variable_name.to_string(),
                        kind: CompletionItemKind::GlobalVariable,
                        ..Default::default()
                    }),
            )
            .chain(Self::object_completions(doc, index))
            .chain(
                index
                    .all_known_alias_symbols()
                    .drain(..)
                    .map(|alias_name| CompletionItem {
                        label: alias_name.to_string(),
                        kind: CompletionItemKind::EnumMember,
                        ..Default::default()
                    }),
            )
            .chain(
                index
                    .all_known_dataflex_tables()
                    .drain(..)
                    .map(|table_name| CompletionItem {
                        label: table_name.to_string(),
                        kind: CompletionItemKind::TableName,
                        ..Default::default()
                    }),
            )
            .collect()
    }

    fn paren_expr_completions(
        doc: &DataFlexDocument,
        position: Point,
        index: &index::Index,
    ) -> Vec<CompletionItem> {
        Self::local_variable_completions(doc, position, index)
            .chain(Self::system_functions(index))
            .chain(
                index
                    .all_known_global_variables()
                    .drain(..)
                    .map(|variable_name| CompletionItem {
                        label: variable_name.to_string(),
                        kind: CompletionItemKind::GlobalVariable,
                        ..Default::default()
                    }),
            )
            .chain(Self::object_completions(doc, index))
            .chain(
                index
                    .all_known_alias_symbols()
                    .drain(..)
                    .map(|alias_name| CompletionItem {
                        label: alias_name.to_string(),
                        kind: CompletionItemKind::EnumMember,
                        ..Default::default()
                    }),
            )
            .chain(
                index
                    .all_known_methods(MethodKind::Get)
                    .drain(..)
                    .map(|method_name| CompletionItem {
                        label: method_name.to_string(),
                        kind: CompletionItemKind::Method,
                        ..Default::default()
                    }),
            )
            .chain(
                index
                    .all_known_properties()
                    .drain(..)
                    .map(|property_name| CompletionItem {
                        label: property_name.to_string(),
                        kind: CompletionItemKind::Property,
                        ..Default::default()
                    }),
            )
            .chain(
                index
                    .all_known_classes()
                    .drain(..)
                    .map(|class_name| CompletionItem {
                        label: class_name.to_string(),
                        kind: CompletionItemKind::Class,
                        ..Default::default()
                    }),
            )
            .chain(
                index
                    .all_known_dataflex_tables()
                    .drain(..)
                    .map(|table_name| CompletionItem {
                        label: table_name.to_string(),
                        kind: CompletionItemKind::TableName,
                        ..Default::default()
                    }),
            )
            .collect()
    }

    fn dot_completions(
        doc: &DataFlexDocument,
        position: Point,
        index: &index::Index,
    ) -> Vec<CompletionItem> {
        let Some(mut cursor) = doc.cursor().and_then(|mut cursor| {
            cursor
                .goto_leaf_node_at_or_before_point(position)
                .then_some(cursor)
        }) else {
            return vec![];
        };

        while !cursor.is_dot() {
            cursor.goto_previous_leaf_node();
        }

        let position = cursor.node().end_position();
        let reference_resolver = ReferenceResolver::new(doc, index);

        let root_name = if cursor.goto_enclosing_member_access()
            && cursor.goto_previous_sibling()
            && cursor.is_identifier()
        {
            Some(SymbolName::from(doc.line_map.text_for_node(&cursor.node())))
        } else {
            None
        };

        let qualified_symbol = if let Some(name) = root_name.as_ref() {
            reference_resolver
                .resolve_type_of_variable(cursor.node().start_position(), name)
                .and_then(|data_type| index.find_struct(data_type.name()))
                .and_then(|struct_ref| index.resolve_symbol(struct_ref))
        } else {
            reference_resolver
                .resolve_reference(DocumentContext::DotMemberExpression, position)
                .next()
        };

        if let Some(qualified_symbol) = qualified_symbol {
            StructSymbol::from_index_symbol(qualified_symbol.symbol)
                .map(|struct_symbol| {
                    struct_symbol
                        .members
                        .iter()
                        .map(|member| CompletionItem {
                            label: member.name().to_string(),
                            kind: CompletionItemKind::StructMember,
                            ..Default::default()
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else if let Some(table) = root_name
            .as_ref()
            .and_then(|name| index.find_dataflex_table(name).map(|t| t.table))
        {
            table
                .columns
                .iter()
                .map(|column| CompletionItem {
                    label: column.to_string(),
                    kind: CompletionItemKind::TableColumn,
                    ..Default::default()
                })
                .collect()
        } else {
            vec![]
        }
    }

    fn command_completions(index: &index::Index) -> Vec<CompletionItem> {
        Self::system_commands(index)
            .chain(
                index
                    .all_known_structs()
                    .iter()
                    .chain(index.all_system_types())
                    .map(|name| CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Struct,
                        ..Default::default()
                    }),
            )
            .collect()
    }

    fn file_completions(index: &index::Index) -> Vec<CompletionItem> {
        index
            .all_known_files()
            .into_iter()
            .filter_map(|file_ref| {
                Some(CompletionItem {
                    label: file_ref.try_into().ok()?,
                    kind: CompletionItemKind::File,
                    ..Default::default()
                })
            })
            .collect()
    }

    fn type_completions(index: &index::Index) -> Vec<CompletionItem> {
        index
            .all_known_structs()
            .iter()
            .chain(index.all_system_types())
            .map(|name| CompletionItem {
                label: name.to_string(),
                kind: CompletionItemKind::Struct,
                ..Default::default()
            })
            .collect()
    }

    fn local_variable_completions<'a>(
        doc: &'a DataFlexDocument,
        position: Point,
        index: &'a index::Index,
    ) -> impl Iterator<Item = CompletionItem> {
        let reference_resolver = ReferenceResolver::new(doc, index);
        reference_resolver
            .local_variables(position)
            .map(|variable| CompletionItem {
                label: variable.symbol_path.name().to_string(),
                kind: CompletionItemKind::LocalVariable,
                ..Default::default()
            })
    }

    fn object_completions(
        doc: &DataFlexDocument,
        index: &index::Index,
    ) -> impl Iterator<Item = CompletionItem> {
        let local_file_ref = index::IndexFileRef::from(&doc.file_path);
        index.all_object_symbols().map(move |symbols| {
            let local_object = symbols
                .iter()
                .find(|symbol_ref| symbol_ref.file_ref == local_file_ref);
            let top_level_object = local_object
                .is_none()
                .then(|| {
                    symbols
                        .iter()
                        .find(|symbol_ref| symbol_ref.symbol_path.is_top_level())
                })
                .flatten();
            let kind = if local_object.is_some() {
                CompletionItemKind::LocalObject
            } else if top_level_object.is_some() {
                CompletionItemKind::TopLevelObject
            } else {
                CompletionItemKind::OtherObject
            };
            let label = symbols
                .first()
                .map(|symbol_ref| symbol_ref.symbol_path.name().to_string())
                .unwrap_or_default();
            CompletionItem {
                label,
                kind,
                ..Default::default()
            }
        })
    }

    fn system_functions(index: &index::Index) -> impl Iterator<Item = CompletionItem> {
        index.all_system_functions().map(|f| CompletionItem {
            label: f.to_string(),
            kind: CompletionItemKind::Function,
            ..Default::default()
        })
    }

    fn system_commands(index: &index::Index) -> impl Iterator<Item = CompletionItem> {
        index.all_commands().map(move |command| CompletionItem {
            label: command.to_string(),
            kind: CompletionItemKind::Command,
            ..Default::default()
        })
    }
}

impl<'a> CompletionItemRanker<'a> {
    pub fn new(doc: &'a DataFlexDocument, position: Point, index: &'a index::Index) -> Self {
        Self {
            doc,
            index,
            position,
            likely_enum_symbols: std::sync::OnceLock::new(),
            likely_commands: std::sync::OnceLock::new(),
        }
    }

    pub fn rank(&self, completion_item: &CompletionItem) -> CompletionItemRank {
        let mut rank = match completion_item.kind {
            CompletionItemKind::LocalVariable => CompletionItemRank::NearTop,
            CompletionItemKind::LocalObject => CompletionItemRank::NearTop,
            CompletionItemKind::TableName => CompletionItemRank::UpperMid,
            CompletionItemKind::TopLevelObject => CompletionItemRank::UpperMid,
            CompletionItemKind::Method => CompletionItemRank::UpperMid,
            CompletionItemKind::Property => CompletionItemRank::UpperMid,
            CompletionItemKind::EnumMember => CompletionItemRank::Mid
                .adjusted(self.enum_symbol_adjustment(&completion_item.label)),
            CompletionItemKind::Text => CompletionItemRank::Mid,
            CompletionItemKind::Class => CompletionItemRank::Mid,
            CompletionItemKind::Function => CompletionItemRank::Mid,
            CompletionItemKind::StructMember => CompletionItemRank::Mid,
            CompletionItemKind::TableColumn => CompletionItemRank::Mid,
            CompletionItemKind::Command => {
                CompletionItemRank::Mid.adjusted(self.command_adjustment(&completion_item.label))
            }
            CompletionItemKind::File => CompletionItemRank::Mid,
            CompletionItemKind::Struct => CompletionItemRank::Mid,
            CompletionItemKind::OtherObject => CompletionItemRank::NearBottom,
            CompletionItemKind::GlobalVariable => CompletionItemRank::NearBottom,
        };

        if completion_item
            .label
            .chars()
            .next()
            .is_some_and(|c| !c.is_ascii_alphabetic())
        {
            rank = rank.adjusted(CompletionItemRankAdjustment::Down);
        }

        rank
    }

    fn enum_symbol_adjustment(&self, name: &str) -> CompletionItemRankAdjustment {
        let likely_enum_symbols = self
            .likely_enum_symbols
            .get_or_init(|| self.likely_enum_symbols());
        if let Some(adjustment) = likely_enum_symbols.get(name) {
            *adjustment
        } else {
            CompletionItemRankAdjustment::None
        }
    }

    fn command_adjustment(&self, name: &str) -> CompletionItemRankAdjustment {
        let likely_commands = self.likely_commands.get_or_init(|| self.likely_commands());
        if let Some(adjustment) = likely_commands.get(name) {
            *adjustment
        } else {
            CompletionItemRankAdjustment::None
        }
    }

    fn likely_enum_symbols(&self) -> HashMap<String, CompletionItemRankAdjustment> {
        if let Some(mut cursor) = self.doc.cursor()
            && cursor.goto_leaf_node_preceding_point(self.position)
            && cursor.is_keyword(|kw| matches!(kw, "to"))
            && cursor.goto_enclosing_method_call()
            && cursor.is_set_statement()
            && let Some(method_name_node) = cursor.node().child_by_field_name("name")
        {
            // This is a set statement, see if there are any associated EnumList meta tags.
            let reference_resolver = ReferenceResolver::new(self.doc, self.index);
            let symbols = reference_resolver.resolve_reference(
                DocumentContext::MethodReference(MethodKind::Set),
                method_name_node.start_position(),
            );

            symbols
                .flat_map(|method| self.index.associated_meta_tags("EnumList".into(), method))
                .flat_map(|tag| tag.value_list())
                .map(|enum_symbol| (String::from(enum_symbol), CompletionItemRankAdjustment::Top))
                .collect()
        } else {
            HashMap::new()
        }
    }

    fn likely_commands(&self) -> HashMap<String, CompletionItemRankAdjustment> {
        let mut result: HashSet<String> = ["Move", "Get", "Set", "Send", "WebGet", "WebSet"]
            .into_iter()
            .map(String::from)
            .collect();

        if let Some(mut cursor) = self.doc.cursor() {
            if cursor.goto_descendant_for_point(self.position)
                && cursor.goto_enclosing_if_statement()
                && cursor
                    .node()
                    .child_by_field_name("condition")
                    .is_some_and(|condition_node| {
                        condition_node.end_position() < self.position
                            && condition_node
                                .next_sibling()
                                .is_none_or(|n| n.end_position() >= self.position)
                    })
            {
                // This is in an if-statement at the action command position, e.g. `if expr |`.
                result.insert("Begin".into());
            } else if cursor.goto_leaf_node_preceding_point(self.position)
                && cursor.is_keyword(|kw| matches!(kw, "else"))
            {
                // This is an else-statement at the action command position, e.g. `else |`.
                result.insert("Begin".into());
                result.insert("If".into());
            } else {
                result.insert("If".into());
                result.insert("For".into());
                result.insert("While".into());
            }

            if cursor.goto_enclosing_method_definition() {
                if cursor.is_function_definition() {
                    result.insert("Function_Return".into());
                } else {
                    result.insert("Procedure_Return".into());
                }
            }
        }

        result
            .into_iter()
            .map(|name| (name, CompletionItemRankAdjustment::Up))
            .collect()
    }
}

impl CompletionItemRank {
    fn adjusted(self, adjustment: CompletionItemRankAdjustment) -> Self {
        match adjustment {
            CompletionItemRankAdjustment::None => self,
            CompletionItemRankAdjustment::Up => match self {
                CompletionItemRank::Top => self,
                CompletionItemRank::NearTop => CompletionItemRank::Top,
                CompletionItemRank::UpperMid => CompletionItemRank::NearTop,
                CompletionItemRank::Mid => CompletionItemRank::UpperMid,
                CompletionItemRank::NearBottom => CompletionItemRank::Mid,
                CompletionItemRank::Bottom => CompletionItemRank::NearBottom,
            },
            CompletionItemRankAdjustment::Down => match self {
                CompletionItemRank::Top => CompletionItemRank::NearTop,
                CompletionItemRank::NearTop => CompletionItemRank::UpperMid,
                CompletionItemRank::UpperMid => CompletionItemRank::Mid,
                CompletionItemRank::Mid => CompletionItemRank::NearBottom,
                CompletionItemRank::NearBottom => CompletionItemRank::Bottom,
                CompletionItemRank::Bottom => self,
            },
            CompletionItemRankAdjustment::Top => CompletionItemRank::Top,
            CompletionItemRankAdjustment::Bottom => CompletionItemRank::Bottom,
        }
    }
}

impl std::fmt::Display for CompletionItemRank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rank = *self as u8;
        write!(f, "{rank}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dot_completions() {
        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    Move "test" to myStruct.
End_Procedure
                "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let index = index.get();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, &index);
        let completions =
            CodeCompletion::code_completion(&doc, Point::new(7, 28), false, &index).unwrap();
        assert_eq!(completions.len(), 1);

        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    Move "test" to myStruct.s
End_Procedure
                "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let index = index.get();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, &index);
        let completions =
            CodeCompletion::code_completion(&doc, Point::new(7, 29), false, &index).unwrap();
        assert_eq!(completions.len(), 1);

        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    Move "test" to myStruct.s
End_Procedure
                "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let index = index.get();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, &index);
        let completions =
            CodeCompletion::code_completion(&doc, Point::new(7, 29), false, &index).unwrap();
        assert_eq!(completions.len(), 1);

        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Procedure test
    tMyStruct myStruct
    Move "test" to myStruct.sName.
End_Procedure
                "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let index = index.get();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, &index);
        let completions =
            CodeCompletion::code_completion(&doc, Point::new(7, 34), false, &index).unwrap();
        assert_eq!(completions.len(), 0);

        let test_content = r#"
Struct tMyStruct
    String sName
End_Struct

Struct tMyOtherStruct
    tMyStruct myStruct
End_Struct

Procedure test
    tMyOtherStruct myOtherStruct
    Move "test" to myOtherStruct.myStruct.
End_Procedure
        "#;
        let index = index::IndexRef::make_test_index_ref();
        index::Indexer::index_test_content(test_content, "test.pkg".into(), &index);
        let index = index.get();
        let doc = DataFlexDocument::new("test.pkg".into(), test_content, &index);
        let completions =
            CodeCompletion::code_completion(&doc, Point::new(11, 42), false, &index).unwrap();
        assert_eq!(completions.len(), 1);
    }
}
