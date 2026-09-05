use crate::index::{Index, QualifiedIndexSymbol, SystemFunction};

pub struct SymbolDeclaration {
    pub declaration: String,
    pub description: Option<String>,
}

impl SymbolDeclaration {
    pub fn with_symbol(qualified_symbol: QualifiedIndexSymbol<'_>, index: &Index) -> Self {
        let description: String = index
            .associated_meta_tags("Description".into(), qualified_symbol)
            .map(|tag| tag.value.trim_matches('"'))
            .collect::<Vec<&str>>()
            .join("\n");
        Self {
            declaration: qualified_symbol.symbol.to_string(),
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
        }
    }

    pub fn with_system_function(function: &SystemFunction) -> Self {
        Self {
            declaration: function.signature.clone(),
            description: Some(function.description.clone()),
        }
    }
}

impl std::fmt::Display for SymbolDeclaration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "```dataflex")?;
        writeln!(f, "{}", self.declaration)?;
        writeln!(f, "```")?;
        if let Some(description) = &self.description {
            writeln!(f)?;
            writeln!(f, "---")?;
            writeln!(f, "{description}")?;
        }
        Ok(())
    }
}
