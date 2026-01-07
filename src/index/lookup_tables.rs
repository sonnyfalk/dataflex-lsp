use super::*;

#[derive(Debug)]
pub struct LookupTables {
    class_lookup_table: HashMap<SymbolName, IndexSymbolRef>,
    method_lookup_tables: [MultiMap<SymbolName, IndexSymbolRef>; 3],
    property_lookup_table: MultiMap<SymbolName, IndexSymbolRef>,
}

impl LookupTables {
    pub fn new() -> Self {
        Self {
            class_lookup_table: HashMap::new(),
            method_lookup_tables: [MultiMap::new(), MultiMap::new(), MultiMap::new()],
            property_lookup_table: MultiMap::new(),
        }
    }

    pub fn class_lookup_table(&self) -> &HashMap<SymbolName, IndexSymbolRef> {
        &self.class_lookup_table
    }

    pub fn class_lookup_table_mut(&mut self) -> &mut HashMap<SymbolName, IndexSymbolRef> {
        &mut self.class_lookup_table
    }

    pub fn method_lookup_table(&self, kind: MethodKind) -> &MultiMap<SymbolName, IndexSymbolRef> {
        match kind {
            MethodKind::Procedure => &self.method_lookup_tables[MethodKind::Procedure as usize],
            MethodKind::Function => &self.method_lookup_tables[MethodKind::Function as usize],
            MethodKind::Set => &self.method_lookup_tables[MethodKind::Set as usize],
        }
    }

    pub fn method_lookup_table_mut(
        &mut self,
        kind: MethodKind,
    ) -> &mut MultiMap<SymbolName, IndexSymbolRef> {
        match kind {
            MethodKind::Procedure => &mut self.method_lookup_tables[MethodKind::Procedure as usize],
            MethodKind::Function => &mut self.method_lookup_tables[MethodKind::Function as usize],
            MethodKind::Set => &mut self.method_lookup_tables[MethodKind::Set as usize],
        }
    }

    pub fn property_lookup_table(&self) -> &MultiMap<SymbolName, IndexSymbolRef> {
        &self.property_lookup_table
    }

    pub fn property_lookup_table_mut(&mut self) -> &mut MultiMap<SymbolName, IndexSymbolRef> {
        &mut self.property_lookup_table
    }

    pub fn is_known_method(&self, name: &SymbolName) -> bool {
        self.method_lookup_tables
            .iter()
            .find_map(|t| t.get(name))
            .is_some()
    }
}
