use std::path::PathBuf;

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
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct IndexRef {
    index: std::sync::Arc<tokio::sync::RwLock<Index>>,
}

pub struct Indexer {
    index: IndexRef,
}

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
        Self { workspace }
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

    #[allow(dead_code)]
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

    async fn index_workspace(_index: &IndexRef) {
        log::info!("Indexing workspace");
    }

    async fn watch_and_index_changed_files(_index: &IndexRef) {
        log::info!("Watching workspace files");
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
