use std::{collections::HashMap, ffi::OsStr, path::PathBuf};

#[allow(dead_code)]
pub struct WorkspaceInfo {
    root_folder: PathBuf,
    projects: Vec<ProjectInfo>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ProjectInfo {
    main_file: PathBuf,
}

#[allow(dead_code)]
pub struct Index {
    workspace: WorkspaceInfo,
    files: HashMap<String, IndexFile>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct IndexRef {
    index: std::sync::Arc<tokio::sync::RwLock<Index>>,
}

pub struct Indexer {
    index: IndexRef,
}

pub struct IndexFile {}

impl WorkspaceInfo {
    pub fn new() -> Self {
        Self {
            root_folder: PathBuf::new(),
            projects: vec![],
        }
    }

    pub fn load_from_path(path: &PathBuf) -> Self {
        if path.is_dir() {
            if let Some(file) = Self::find_first_sws(path) {
                return Self::load_from_path(&file);
            }
        }

        if let Some(ini_file) = ini::Ini::load_from_file(path).ok() {
            let root_folder = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let projects: Vec<ProjectInfo> = ini_file
                .section(Some("Projects"))
                .map(|projects| {
                    projects
                        .iter()
                        .map(|(_, v)| ProjectInfo {
                            main_file: root_folder.join("AppSrc").join(v),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Self {
                root_folder,
                projects,
            }
        } else {
            Self {
                root_folder: path.clone(),
                projects: vec![],
            }
        }
    }

    fn find_first_sws(path: &PathBuf) -> Option<PathBuf> {
        path.read_dir().ok()?.find_map(|f| {
            let file_path = f.ok()?.path();
            if file_path.extension()?.to_str() == Some("sws") {
                Some(file_path)
            } else {
                None
            }
        })
    }
}

impl Index {
    pub fn new(workspace: WorkspaceInfo) -> Self {
        Self {
            workspace,
            files: HashMap::new(),
        }
    }
}

impl IndexRef {
    pub fn new(index: Index) -> Self {
        Self {
            index: std::sync::Arc::new(tokio::sync::RwLock::new(index)),
        }
    }

    #[allow(dead_code)]
    pub async fn get(&self) -> tokio::sync::RwLockReadGuard<Index> {
        self.index.read().await
    }

    pub async fn get_mut(&self) -> tokio::sync::RwLockWriteGuard<Index> {
        self.index.write().await
    }
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
            Self::watch_and_index_changed_files(&index).await;
        });
    }

    async fn index_workspace(index: &IndexRef) {
        log::info!("Indexing workspace");
        let root_folder = index.get().await.workspace.root_folder.clone();
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
        log::info!("Indexing file content for {:?}", path);
        let mut parser = Self::make_parser();

        let Some(tree) = parser.parse(content, None) else {
            return;
        };

        Self::index_parse_tree(&tree, path, index).await;
    }

    async fn index_parse_tree(_tree: &tree_sitter::Tree, path: &PathBuf, index: &IndexRef) {
        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            return;
        };
        log::info!("Indexing file parse tree for {:?}", path);
        let index_file = IndexFile {};
        index
            .get_mut()
            .await
            .files
            .insert(file_name.to_string(), index_file);
    }

    async fn watch_and_index_changed_files(_index: &IndexRef) {
        log::info!("Watching workspace files");
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

#[cfg(test)]
impl Index {
    pub fn make_test_index() -> Self {
        Self::new(WorkspaceInfo::new())
    }
}

#[cfg(test)]
impl IndexRef {
    pub fn make_test_index_ref() -> Self {
        Self::new(Index::make_test_index())
    }
}
