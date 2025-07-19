use super::*;

#[derive(Debug)]
#[allow(dead_code)]
pub enum IndexSymbol {
    Class(ClassSymbol),
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ClassSymbol {
    pub location: Point,
    pub name: SymbolName,
    pub methods: Vec<MethodSymbol>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MethodSymbol {
    pub location: Point,
    pub name: SymbolName,
    pub kind: MethodKind,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum MethodKind {
    Procedure,
    Function,
    Set,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SymbolName(String);

#[derive(Debug)]
pub struct IndexSymbolSnapshot<'a, IndexSymbolType> {
    pub path: &'a PathBuf,
    pub symbol: &'a IndexSymbolType,
}

pub type ClassSymbolSnapshot<'a> = IndexSymbolSnapshot<'a, ClassSymbol>;

#[derive(Debug)]
pub struct IndexSymbolRef {
    pub file_ref: IndexFileRef,
    pub symbol_path: Vec<SymbolName>,
}

impl IndexSymbol {
    pub fn name(&self) -> &SymbolName {
        match self {
            Self::Class(class_symbol) => &class_symbol.name,
        }
    }

    pub fn is_matching(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Class(class_symbol), Self::Class(other_class_symbol)) => {
                class_symbol.name == other_class_symbol.name
            }
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

pub trait IndexSymbolType {
    fn from_index_symbol(index_symbol: &IndexSymbol) -> Option<&Self>;
    fn from_index_symbol_mut(index_symbol: &mut IndexSymbol) -> Option<&mut Self>;
}

impl IndexSymbolType for ClassSymbol {
    fn from_index_symbol(index_symbol: &IndexSymbol) -> Option<&Self> {
        match index_symbol {
            IndexSymbol::Class(class_symbol) => Some(class_symbol),
        }
    }

    fn from_index_symbol_mut(index_symbol: &mut IndexSymbol) -> Option<&mut Self> {
        match index_symbol {
            IndexSymbol::Class(class_symbol) => Some(class_symbol),
        }
    }
}

impl IndexSymbolRef {
    pub fn new(file_ref: IndexFileRef, symbol_path: Vec<SymbolName>) -> Self {
        Self {
            file_ref,
            symbol_path,
        }
    }
}
