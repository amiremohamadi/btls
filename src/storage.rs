use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct Storage {
    documents: BTreeMap<PathBuf, String>,
}

impl Storage {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn read(&mut self, path: &Path) -> String {
        self.documents
            .get(path)
            .map(|x| x.to_owned())
            .unwrap_or_else(|| std::fs::read_to_string(path).unwrap_or_default())
    }

    pub fn load(&mut self, path: &Path, data: &str) {
        self.documents.insert(path.to_path_buf(), data.to_string());
    }
}
