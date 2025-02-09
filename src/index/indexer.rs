use super::*;

pub struct Indexer {
    index: IndexRef,
}

impl Indexer {
    pub fn new(workspace: WorkspaceInfo) -> Self {
        Self {
            index: IndexRef::new(Index::new(workspace)),
        }
    }

    pub fn get_index(&self) -> &IndexRef {
        &self.index
    }

    pub fn start_indexing(&self) {
        let index = self.index.clone();
        tokio::spawn(async move {
            Self::index_workspace(&index).await;
            log::trace!("Finished indexing workspace:\n{:#?}", index.get().await);
            Self::watch_and_index_changed_files(&index).await;
        });
    }

    async fn index_workspace(index: &IndexRef) {
        log::trace!("Indexing workspace");
        let root_folder = index.get().await.workspace.get_root_folder().clone();
        Self::index_directory(root_folder, index).await;
    }

    async fn index_directory(path: PathBuf, index: &IndexRef) {
        let Some(path_entries) = path.read_dir().ok() else {
            return;
        };
        for path in path_entries.filter_map(|p| Some(p.ok()?.path())) {
            if path.is_dir() {
                Box::pin(Self::index_directory(path, index)).await;
            } else if Self::should_index_file(&path) {
                Self::index_file(path, index).await;
            }
        }
    }

    async fn index_file(path: PathBuf, index: &IndexRef) {
        if !path.is_file() || !path.exists() {
            return;
        }
        let Some(content) = tokio::fs::read(&path).await.ok() else {
            return;
        };
        Self::index_file_content(&content, &path, index).await;
    }

    async fn index_file_content(content: &[u8], path: &PathBuf, index: &IndexRef) {
        log::trace!("Indexing file content for {:?}", path);
        let mut parser = Self::make_parser();

        let Some(tree) = parser.parse(content, None) else {
            return;
        };

        Self::index_parse_tree(&tree, content, path, index).await;
    }

    async fn index_parse_tree(
        tree: &tree_sitter::Tree,
        content: &[u8],
        path: &PathBuf,
        index: &IndexRef,
    ) {
        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            return;
        };
        log::trace!("Indexing file parse tree for {:?}", path);

        let query = tree_sitter::Query::new(
            &tree_sitter_dataflex::LANGUAGE.into(),
            tree_sitter_dataflex::TAGS_QUERY,
        )
        .expect("Error loading TAGS_QUERY");

        let pattern_index_element_map: Vec<Option<TagsQueryIndexElement>> = (0..query
            .pattern_count())
            .map(|pattern_index| {
                query
                    .property_settings(pattern_index)
                    .iter()
                    .find_map(|p| match p.key.as_ref() {
                        "index.element" => TagsQueryIndexElement::from_str(p.value.as_ref()?).ok(),
                        _ => None,
                    })
            })
            .collect();
        let name_capture_index = query.capture_index_for_name("name").unwrap();
        let mut query_cursor = tree_sitter::QueryCursor::new();
        let matches = query_cursor.matches(&query, tree.root_node(), content);

        let index_file = matches.fold(IndexFile::new(), |mut index_file, query_match| {
            match pattern_index_element_map[query_match.pattern_index] {
                Some(TagsQueryIndexElement::FileDependency) => {
                    if let Some(file_dependency) = query_match
                        .nodes_for_capture_index(name_capture_index)
                        .next()
                        .map(|node| node.utf8_text(content).ok())
                        .flatten()
                    {
                        index_file.dependencies.push(file_dependency.to_string());
                    }
                }
                _ => {}
            };
            index_file
        });

        index
            .get_mut()
            .await
            .files
            .insert(file_name.to_string(), index_file);
    }

    async fn watch_and_index_changed_files(_index: &IndexRef) {
        log::trace!("Watching workspace files");
    }

    fn make_parser() -> tree_sitter::Parser {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_dataflex::LANGUAGE.into())
            .expect("Error loading DataFlex grammar");
        parser
    }

    fn should_index_file(path: &PathBuf) -> bool {
        match path.extension().and_then(OsStr::to_str) {
            Some("pkg" | "vw" | "wo" | "sl" | "dd") => true,
            _ => false,
        }
    }
}

#[derive(EnumString)]
#[strum(serialize_all = "snake_case")]
enum TagsQueryIndexElement {
    FileDependency,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_index_file_dependency() {
        let index_ref = IndexRef::make_test_index_ref();
        Indexer::index_file_content(
            "Use cWebView.pkg\n".as_bytes(),
            &PathBuf::from_str("test.vw").unwrap(),
            &index_ref,
        )
        .await;

        assert_eq!(
            index_ref.get().await.files["test.vw"].dependencies,
            ["cWebView.pkg"]
        );
    }
}
