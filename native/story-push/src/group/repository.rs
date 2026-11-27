//! Repository port over story files, so the saga can be proved against an in-memory fake.

use crate::{lenient_story::Js, list};
use std::{fs, path::PathBuf};

pub(crate) trait StoryRepository {
    /// The story's current file bytes, kept by the saga for compensation.
    fn snapshot(&self, id: &str) -> Result<Vec<u8>, String>;
    fn save(&mut self, story: &Js) -> Result<(), String>;
    /// Puts back the exact bytes `snapshot` returned.
    fn restore(&mut self, id: &str, bytes: &[u8]) -> Result<(), String>;
}

pub(crate) struct FileStoryRepository {
    pub(crate) dir: PathBuf,
}

impl FileStoryRepository {
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if !crate::valid_id(id) {
            return Err(format!("Invalid story id: {id}"));
        }
        Ok(self.dir.join(format!("{id}.md")))
    }
}

impl StoryRepository for FileStoryRepository {
    fn snapshot(&self, id: &str) -> Result<Vec<u8>, String> {
        let path = self.path(id)?;
        fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
    }
    fn save(&mut self, story: &Js) -> Result<(), String> {
        list::save(&self.dir, story)
    }
    fn restore(&mut self, id: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.path(id)?;
        let tmp = path.with_extension("md.restore");
        fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
        fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
    }
}
