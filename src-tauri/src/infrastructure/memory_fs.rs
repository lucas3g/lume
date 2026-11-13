//! Memória por projeto: os arquivos Markdown que o Claude Code guarda em
//! `~/.claude/projects/<pasta codificada>/memory`.

use crate::application::{ports::MemoryStore, AppError, AppResult};
use crate::domain::project::{claude_project_dir_name, valid_memory_name, MemoryFile};
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MEMORY_DIR: &str = "memory";

pub struct FsMemory {
    /// `~/.claude/projects`.
    projects_dir: PathBuf,
}

impl FsMemory {
    pub fn new(projects_dir: PathBuf) -> Self {
        FsMemory { projects_dir }
    }

    fn dir(&self, folder: &str) -> PathBuf {
        self.projects_dir.join(claude_project_dir_name(folder)).join(MEMORY_DIR)
    }

    fn file(&self, folder: &str, name: &str) -> AppResult<PathBuf> {
        if !valid_memory_name(name) {
            return Err(AppError("Use um nome simples terminado em .md.".into()));
        }
        Ok(self.dir(folder).join(name))
    }
}

fn io(e: std::io::Error) -> AppError {
    AppError(format!("Não foi possível acessar a memória do projeto: {e}"))
}

fn modified(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl MemoryStore for FsMemory {
    fn list(&self, folder: &str) -> AppResult<Vec<MemoryFile>> {
        let Ok(entries) = fs::read_dir(self.dir(folder)) else { return Ok(vec![]) };
        let mut files: Vec<MemoryFile> = entries
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                valid_memory_name(&name).then(|| MemoryFile {
                    size: e.metadata().map(|m| m.len()).unwrap_or(0),
                    modified: modified(&e.path()),
                    name,
                })
            })
            .collect();
        // MEMORY.md (o índice) primeiro; o resto por nome.
        files.sort_by(|a, b| (a.name != "MEMORY.md").cmp(&(b.name != "MEMORY.md")).then(a.name.cmp(&b.name)));
        Ok(files)
    }

    fn read(&self, folder: &str, name: &str) -> AppResult<String> {
        fs::read_to_string(self.file(folder, name)?).map_err(io)
    }

    fn write(&self, folder: &str, name: &str, content: &str) -> AppResult<()> {
        let path = self.file(folder, name)?;
        fs::create_dir_all(self.dir(folder)).map_err(io)?;
        fs::write(path, content).map_err(io)
    }

    fn remove(&self, folder: &str, name: &str) -> AppResult<()> {
        fs::remove_file(self.file(folder, name)?).map_err(io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_reads_writes_and_removes_memory_files() {
        let root = std::env::temp_dir().join(format!("lume-memory-{}", uuid::Uuid::new_v4()));
        let memory = FsMemory::new(root.clone());
        assert!(memory.list("/tmp/loja").unwrap().is_empty());
        memory.write("/tmp/loja", "notas.md", "oi").unwrap();
        memory.write("/tmp/loja", "MEMORY.md", "- [x](notas.md)").unwrap();
        assert!(root.join("-tmp-loja").join("memory").join("notas.md").exists());
        let names: Vec<String> = memory.list("/tmp/loja/").unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["MEMORY.md", "notas.md"]);
        assert_eq!(memory.read("/tmp/loja", "notas.md").unwrap(), "oi");
        assert!(memory.write("/tmp/loja", "../fora.md", "x").is_err());
        memory.remove("/tmp/loja", "notas.md").unwrap();
        assert_eq!(memory.list("/tmp/loja").unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
