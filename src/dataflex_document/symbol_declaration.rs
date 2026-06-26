use crate::index::{Index, IndexSymbolSnapshot};

pub struct SymbolDeclaration {
    pub declaration: String,
    pub description: Option<String>,
}

impl SymbolDeclaration {
    pub fn new(symbol_snapshot: &IndexSymbolSnapshot<'_>, index: &Index) -> Self {
        let description: String = index
            .associated_meta_tags("Description".into(), symbol_snapshot)
            .map(|tag| tag.value.trim_matches('"'))
            .collect::<Vec<&str>>()
            .join("\n");
        Self {
            declaration: symbol_snapshot.symbol.to_string(),
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
        }
    }
}

impl std::fmt::Display for SymbolDeclaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "```dataflex")?;
        writeln!(f, "{}", self.declaration)?;
        writeln!(f, "```")?;
        if let Some(description) = &self.description {
            writeln!(f, "")?;
            writeln!(f, "---")?;
            writeln!(f, "{description}")?;
        }
        Ok(())
    }
}
