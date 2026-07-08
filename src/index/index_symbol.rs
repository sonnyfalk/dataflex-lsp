use super::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum IndexSymbol {
    Class(ClassSymbol),
    Object(ClassSymbol),
    Struct(StructSymbol),
    Method(MethodSymbol),
    Property(VariableSymbol),
    Variable(VariableSymbol),
    Alias(AliasSymbol),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassSymbol {
    pub location: SourceLocation,
    pub range: SourceRange,
    pub symbol_path: SymbolPath,
    pub superclass: SymbolName,
    pub mixins: Vec<SymbolName>,
    pub members: Vec<IndexSymbol>,
    pub metadata: Vec<MetadataTagSet>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StructSymbol {
    pub location: SourceLocation,
    pub range: SourceRange,
    pub symbol_path: SymbolPath,
    pub members: Vec<IndexSymbol>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MethodSymbol {
    pub location: SourceLocation,
    pub range: SourceRange,
    pub symbol_path: SymbolPath,
    pub kind: MethodKind,
    pub parameters: Vec<(SymbolName, DataFlexDataType)>,
    pub return_type: Option<DataFlexDataType>,
    pub metadata: Vec<MetadataTagSet>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VariableSymbol {
    pub location: SourceLocation,
    pub range: SourceRange,
    pub symbol_path: SymbolPath,
    pub data_type: DataFlexDataType,
    pub metadata: Vec<MetadataTagSet>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AliasSymbol {
    pub location: SourceLocation,
    pub range: SourceRange,
    pub symbol_path: SymbolPath,
    pub alias: ValueReference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRange {
    pub start: SourceLocation,
    pub end: SourceLocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MethodKind {
    Msg,
    Get,
    Set,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum DataFlexDataType {
    Simple(SymbolName),
    Array(SymbolName, usize),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ValueReference {
    Symbol(SymbolName),
    Value(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SymbolName(String);

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SymbolPath(Vec<SymbolName>);

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataTagSet {
    pub tags: Vec<MetadataTag>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataTag {
    pub name: SymbolName,
    pub value: String,
}

#[derive(Clone, Copy)]
pub struct QualifiedIndexSymbol<'a> {
    pub file: &'a IndexFile,
    pub symbol: &'a IndexSymbol,
}

#[derive(Debug)]
pub struct IndexSymbolRef {
    pub file_ref: IndexFileRef,
    pub symbol_path: SymbolPath,
}

impl IndexSymbol {
    pub fn name(&self) -> &SymbolName {
        match self {
            Self::Class(class_symbol) => class_symbol.symbol_path.name(),
            Self::Object(class_symbol) => class_symbol.symbol_path.name(),
            Self::Struct(struct_symbol) => struct_symbol.symbol_path.name(),
            Self::Method(method_symbol) => method_symbol.symbol_path.name(),
            Self::Property(variable_symbol) => variable_symbol.symbol_path.name(),
            Self::Variable(variable_symbol) => variable_symbol.symbol_path.name(),
            Self::Alias(alias_symbol) => alias_symbol.symbol_path.name(),
        }
    }

    pub fn symbol_path(&self) -> &SymbolPath {
        match self {
            Self::Class(class_symbol) => &class_symbol.symbol_path,
            Self::Object(class_symbol) => &class_symbol.symbol_path,
            Self::Struct(struct_symbol) => &struct_symbol.symbol_path,
            Self::Method(method_symbol) => &method_symbol.symbol_path,
            Self::Property(variable_symbol) => &variable_symbol.symbol_path,
            Self::Variable(variable_symbol) => &variable_symbol.symbol_path,
            Self::Alias(alias_symbol) => &alias_symbol.symbol_path,
        }
    }

    pub fn location(&self) -> SourceLocation {
        match self {
            Self::Class(class_symbol) => class_symbol.location,
            Self::Object(class_symbol) => class_symbol.location,
            Self::Struct(struct_symbol) => struct_symbol.location,
            Self::Method(method_symbol) => method_symbol.location,
            Self::Property(variable_symbol) => variable_symbol.location,
            Self::Variable(variable_symbol) => variable_symbol.location,
            Self::Alias(alias_symbol) => alias_symbol.location,
        }
    }

    pub fn range(&self) -> SourceRange {
        match self {
            Self::Class(class_symbol) => class_symbol.range,
            Self::Object(class_symbol) => class_symbol.range,
            Self::Struct(struct_symbol) => struct_symbol.range,
            Self::Method(method_symbol) => method_symbol.range,
            Self::Property(variable_symbol) => variable_symbol.range,
            Self::Variable(variable_symbol) => variable_symbol.range,
            Self::Alias(alias_symbol) => alias_symbol.range,
        }
    }

    pub fn metadata_tags(&self) -> impl Iterator<Item = &MetadataTag> {
        let metadata = match self {
            Self::Class(class_symbol) => Some(&class_symbol.metadata),
            Self::Object(class_symbol) => Some(&class_symbol.metadata),
            Self::Method(method_symbol) => Some(&method_symbol.metadata),
            Self::Property(variable_symbol) => Some(&variable_symbol.metadata),
            Self::Variable(variable_symbol) => Some(&variable_symbol.metadata),
            Self::Struct(_) => None,
            Self::Alias(_) => None,
        };
        metadata
            .into_iter()
            .flat_map(|metadata| metadata.iter().flat_map(|tag_set| tag_set.tags.iter()))
    }

    pub fn is_matching(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Class(class_symbol), Self::Class(other_class_symbol)) => {
                class_symbol.symbol_path == other_class_symbol.symbol_path
            }
            (Self::Object(class_symbol), Self::Object(other_class_symbol)) => {
                class_symbol.symbol_path == other_class_symbol.symbol_path
            }
            (Self::Struct(struct_symbol), Self::Struct(other_struct_symbol)) => {
                struct_symbol.symbol_path == other_struct_symbol.symbol_path
            }
            (Self::Method(method_symbol), Self::Method(other_method_symbol)) => {
                method_symbol.symbol_path == other_method_symbol.symbol_path
            }
            (Self::Property(variable_symbol), Self::Property(other_variable_symbol)) => {
                variable_symbol.symbol_path == other_variable_symbol.symbol_path
            }
            (Self::Variable(variable_symbol), Self::Variable(other_variable_symbol)) => {
                variable_symbol.symbol_path == other_variable_symbol.symbol_path
            }
            (Self::Alias(alias_symbol), Self::Alias(other_alias_symbol)) => {
                alias_symbol.symbol_path == other_alias_symbol.symbol_path
            }
            (Self::Class(_), _) => false,
            (Self::Object(_), _) => false,
            (Self::Struct(_), _) => false,
            (Self::Method(_), _) => false,
            (Self::Property(_), _) => false,
            (Self::Variable(_), _) => false,
            (Self::Alias(_), _) => false,
        }
    }

    pub fn child(&self, name: &SymbolName) -> Option<&Self> {
        match self {
            Self::Class(class_symbol) => class_symbol.members.iter().find(|s| s.name() == name),
            Self::Object(class_symbol) => class_symbol.members.iter().find(|s| s.name() == name),
            Self::Struct(struct_symbol) => struct_symbol.members.iter().find(|s| s.name() == name),
            Self::Method(_) => None,
            Self::Property(_) => None,
            Self::Variable(_) => None,
            Self::Alias(_) => None,
        }
    }

    pub fn children(&self) -> impl DoubleEndedIterator<Item = &IndexSymbol> + use<'_> {
        match self {
            Self::Class(class_symbol) => class_symbol.members.iter(),
            Self::Object(class_symbol) => class_symbol.members.iter(),
            Self::Struct(struct_symbol) => struct_symbol.members.iter(),
            Self::Method(_) => Default::default(),
            Self::Property(_) => Default::default(),
            Self::Variable(_) => Default::default(),
            Self::Alias(_) => Default::default(),
        }
    }

    pub fn resolve(&self, mut sym_path_it: core::slice::Iter<SymbolName>) -> Option<&Self> {
        if let Some(name) = sym_path_it.next() {
            self.child(name).and_then(|s| s.resolve(sym_path_it))
        } else {
            Some(self)
        }
    }
}

impl From<tree_sitter::Point> for SourceLocation {
    fn from(value: tree_sitter::Point) -> Self {
        Self {
            line: value.row,
            column: value.column,
        }
    }
}

impl SourceRange {
    pub fn new(start: SourceLocation, end: SourceLocation) -> Self {
        Self { start, end }
    }

    pub fn with_location(location: SourceLocation) -> Self {
        Self {
            start: location,
            end: location,
        }
    }
}

impl From<tree_sitter::Range> for SourceRange {
    fn from(value: tree_sitter::Range) -> Self {
        Self {
            start: value.start_point.into(),
            end: value.end_point.into(),
        }
    }
}

impl SymbolName {
    pub fn starts_with(&self, pat: &str) -> bool {
        self.0.starts_with(pat)
    }
}

impl PartialEq for SymbolName {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for SymbolName {}

impl std::hash::Hash for SymbolName {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.to_ascii_lowercase().hash(state);
    }
}

impl From<String> for SymbolName {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for SymbolName {
    fn from(value: &str) -> Self {
        Self::from(String::from(value))
    }
}

impl std::fmt::Display for SymbolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl DataFlexDataType {
    pub fn name(&self) -> &SymbolName {
        match self {
            Self::Simple(type_name) => type_name,
            Self::Array(type_name, _) => type_name,
        }
    }
}

impl std::fmt::Display for DataFlexDataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simple(type_name) => write!(f, "{type_name}"),
            Self::Array(type_name, dimension_count) => {
                write!(f, "{type_name}{}", "[]".repeat(*dimension_count))
            }
        }
    }
}

impl std::fmt::Debug for DataFlexDataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DataFlexDataType(\"{}\")", self)
    }
}

impl SymbolPath {
    pub fn with_name<T: Into<SymbolName>>(name: T) -> Self {
        Self(vec![name.into()])
    }

    pub fn with_parent_and_name<T: Into<SymbolName>>(parent: &SymbolPath, name: T) -> Self {
        let mut path = Vec::with_capacity(parent.0.len() + 1);
        path.extend_from_slice(&parent.0);
        path.push(name.into());
        Self(path)
    }

    pub fn name(&self) -> &SymbolName {
        self.0.last().unwrap()
    }

    pub fn parent_name(&self) -> Option<&SymbolName> {
        self.parent_slice().last()
    }

    pub fn parent_path(&self) -> Option<SymbolPath> {
        let parent_slice = self.parent_slice();
        if !parent_slice.is_empty() {
            Some(SymbolPath(parent_slice.to_vec()))
        } else {
            None
        }
    }

    pub fn is_top_level(&self) -> bool {
        self.0.len() == 1
    }

    pub fn as_slice(&self) -> &[SymbolName] {
        self.0.as_slice()
    }

    pub fn parent_slice(&self) -> &[SymbolName] {
        self.0
            .as_slice()
            .split_last()
            .map(|(_, parent)| parent)
            .unwrap_or_default()
    }
}

impl std::fmt::Debug for SymbolPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted_path = self
            .0
            .iter()
            .map(SymbolName::to_string)
            .collect::<Vec<String>>()
            .join(".");
        write!(f, "SymbolPath(\"{formatted_path}\")")
    }
}

impl From<Vec<SymbolName>> for SymbolPath {
    fn from(value: Vec<SymbolName>) -> Self {
        SymbolPath(value)
    }
}

impl<'a> QualifiedIndexSymbol<'a> {
    pub fn parent_symbol(&self) -> Option<QualifiedIndexSymbol<'a>> {
        self.symbol
            .symbol_path()
            .parent_path()
            .and_then(|parent_path| self.file.resolve(&parent_path))
            .map(|parent_symbol| QualifiedIndexSymbol {
                file: self.file,
                symbol: parent_symbol,
            })
    }

    pub fn children(&self) -> impl DoubleEndedIterator<Item = QualifiedIndexSymbol<'a>> + use<'a> {
        self.symbol.children().map(|s| QualifiedIndexSymbol {
            file: self.file,
            symbol: s,
        })
    }
}

impl std::fmt::Debug for QualifiedIndexSymbol<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "QualifiedIndexSymbol {{ file.path: {:?}, symbol: {:?} }}",
            self.file.path, self.symbol
        )
    }
}

pub trait IndexSymbolType {
    fn from_index_symbol(index_symbol: &IndexSymbol) -> Option<&Self>;
    fn from_index_symbol_mut(index_symbol: &mut IndexSymbol) -> Option<&mut Self>;
}

impl IndexSymbolType for ClassSymbol {
    fn from_index_symbol(index_symbol: &IndexSymbol) -> Option<&Self> {
        if let IndexSymbol::Class(class_symbol) = index_symbol {
            Some(class_symbol)
        } else if let IndexSymbol::Object(class_symbol) = index_symbol {
            Some(class_symbol)
        } else {
            None
        }
    }

    fn from_index_symbol_mut(index_symbol: &mut IndexSymbol) -> Option<&mut Self> {
        if let IndexSymbol::Class(class_symbol) = index_symbol {
            Some(class_symbol)
        } else if let IndexSymbol::Object(class_symbol) = index_symbol {
            Some(class_symbol)
        } else {
            None
        }
    }
}

impl IndexSymbolType for StructSymbol {
    fn from_index_symbol(index_symbol: &IndexSymbol) -> Option<&Self> {
        if let IndexSymbol::Struct(struct_symbol) = index_symbol {
            Some(struct_symbol)
        } else {
            None
        }
    }

    fn from_index_symbol_mut(index_symbol: &mut IndexSymbol) -> Option<&mut Self> {
        if let IndexSymbol::Struct(struct_symbol) = index_symbol {
            Some(struct_symbol)
        } else {
            None
        }
    }
}

impl IndexSymbolType for MethodSymbol {
    fn from_index_symbol(index_symbol: &IndexSymbol) -> Option<&Self> {
        if let IndexSymbol::Method(method_symbol) = index_symbol {
            Some(method_symbol)
        } else {
            None
        }
    }

    fn from_index_symbol_mut(index_symbol: &mut IndexSymbol) -> Option<&mut Self> {
        if let IndexSymbol::Method(method_symbol) = index_symbol {
            Some(method_symbol)
        } else {
            None
        }
    }
}

impl IndexSymbolType for VariableSymbol {
    fn from_index_symbol(index_symbol: &IndexSymbol) -> Option<&Self> {
        if let IndexSymbol::Variable(variable_symbol) = index_symbol {
            Some(variable_symbol)
        } else {
            None
        }
    }

    fn from_index_symbol_mut(index_symbol: &mut IndexSymbol) -> Option<&mut Self> {
        if let IndexSymbol::Variable(variable_symbol) = index_symbol {
            Some(variable_symbol)
        } else {
            None
        }
    }
}

impl IndexSymbolRef {
    pub fn new(file_ref: IndexFileRef, symbol_path: SymbolPath) -> Self {
        Self {
            file_ref,
            symbol_path,
        }
    }
}

impl std::fmt::Display for IndexSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Class(class_symbol) => write!(
                f,
                "Class {} is a {}",
                class_symbol.symbol_path.name(),
                class_symbol.superclass
            ),
            Self::Object(class_symbol) => write!(
                f,
                "Object {} is a {}",
                class_symbol.symbol_path.name(),
                class_symbol.superclass
            ),
            Self::Struct(struct_symbol) => {
                writeln!(f, "Struct {}", struct_symbol.symbol_path.name())?;
                for member in &struct_symbol.members {
                    writeln!(f, "   {}", member)?;
                }
                writeln!(f, "End_Struct")
            }
            Self::Method(method_symbol) => {
                write!(
                    f,
                    "{} {}",
                    match method_symbol.kind {
                        MethodKind::Msg => "Procedure",
                        MethodKind::Get => "Function",
                        MethodKind::Set => "Procedure Set",
                    },
                    method_symbol.symbol_path.name()
                )?;
                for (name, data_type) in &method_symbol.parameters {
                    write!(f, " {} {}", data_type, name)?;
                }
                if let Some(return_type) = &method_symbol.return_type {
                    write!(f, " Returns {}", return_type)?;
                }
                Ok(())
            }
            Self::Property(variable_symbol) => {
                write!(f, "Property {}", variable_symbol,)
            }
            Self::Variable(variable_symbol) => write!(f, "{}", variable_symbol),
            Self::Alias(alias_symbol) => write!(
                f,
                "{} = {}",
                alias_symbol.symbol_path.name(),
                alias_symbol.alias
            ),
        }
    }
}

impl std::fmt::Display for VariableSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.data_type, self.symbol_path.name())
    }
}

impl std::fmt::Display for ValueReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValueReference::Symbol(name) => write!(f, "{name}"),
            ValueReference::Value(value) => write!(f, "{value}"),
        }
    }
}
