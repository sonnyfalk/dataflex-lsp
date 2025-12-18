use super::*;

#[derive(Debug)]
#[allow(dead_code)]
pub enum IndexSymbol {
    Class(ClassSymbol),
    Method(MethodSymbol),
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ClassSymbol {
    pub location: Point,
    pub name: SymbolName,
    pub methods: Vec<IndexSymbol>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MethodSymbol {
    pub location: Point,
    pub symbol_path: SymbolPath,
    pub kind: MethodKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum MethodKind {
    Procedure,
    Function,
    Set,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SymbolName(String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SymbolPath(Vec<SymbolName>);

#[derive(Debug)]
pub struct IndexSymbolSnapshot<'a, IndexSymbolType> {
    pub path: &'a PathBuf,
    pub symbol: &'a IndexSymbolType,
}

pub type ClassSymbolSnapshot<'a> = IndexSymbolSnapshot<'a, ClassSymbol>;

#[derive(Debug)]
pub struct IndexSymbolRef {
    pub file_ref: IndexFileRef,
    pub symbol_path: SymbolPath,
}

impl IndexSymbol {
    pub fn name(&self) -> &SymbolName {
        match self {
            Self::Class(class_symbol) => &class_symbol.name,
            Self::Method(method_symbol) => method_symbol.symbol_path.name(),
        }
    }

    pub fn is_matching(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Class(class_symbol), Self::Class(other_class_symbol)) => {
                class_symbol.name == other_class_symbol.name
            }
            (Self::Method(method_symbol), Self::Method(other_method_symbol)) => {
                method_symbol.symbol_path == other_method_symbol.symbol_path
            }
            _ => false,
        }
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

impl ToString for SymbolName {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}

impl SymbolPath {
    pub fn new(path: Vec<SymbolName>) -> Self {
        assert!(!path.is_empty());
        Self(path)
    }

    pub fn name(&self) -> &SymbolName {
        self.0.last().unwrap()
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
        } else {
            None
        }
    }

    fn from_index_symbol_mut(index_symbol: &mut IndexSymbol) -> Option<&mut Self> {
        if let IndexSymbol::Class(class_symbol) = index_symbol {
            Some(class_symbol)
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
