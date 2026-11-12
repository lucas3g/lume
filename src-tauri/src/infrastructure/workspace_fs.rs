//! Explorador de arquivos sobre o sistema de arquivos local.

use crate::application::{ports::Workspace, AppError, AppResult};
use crate::domain::workspace::{FileEntry, FileView};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Acima disso o arquivo é mostrado só com suas informações.
const MAX_PREVIEW_BYTES: u64 = 400 * 1024;
const MAX_ENTRIES: usize = 1000;
/// Itens internos que só poluem a árvore.
const HIDDEN: [&str; 2] = [".git", ".DS_Store"];

pub struct FsWorkspace;

/// Caminho absoluto de `relative` dentro de `root`; recusa o que escapar da pasta.
pub(crate) fn resolve(root: &str, relative: &str) -> AppResult<PathBuf> {
    let outside = || AppError("Este caminho fica fora da pasta da sessão.".into());
    let base = fs::canonicalize(root).map_err(|_| AppError("A pasta da sessão não existe mais.".into()))?;
    let target = fs::canonicalize(base.join(relative)).map_err(|_| AppError("Arquivo não encontrado.".into()))?;
    if target.starts_with(&base) {
        Ok(target)
    } else {
        Err(outside())
    }
}

fn relative_path(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_owned()
    } else {
        format!("{}/{name}", dir.trim_end_matches('/'))
    }
}

impl Workspace for FsWorkspace {
    fn entries(&self, root: &str, dir: &str) -> AppResult<Vec<FileEntry>> {
        let folder = resolve(root, dir)?;
        let listing = fs::read_dir(&folder).map_err(|_| AppError("Não foi possível abrir a pasta.".into()))?;
        let mut entries: Vec<FileEntry> = listing
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if HIDDEN.contains(&name.as_str()) {
                    return None;
                }
                let is_dir = e.file_type().ok()?.is_dir();
                Some(FileEntry { path: relative_path(dir, &name), name, is_dir })
            })
            .collect();
        // Pastas primeiro, depois por nome sem diferenciar maiúsculas.
        entries.sort_by_key(|e| (!e.is_dir, e.name.to_lowercase()));
        entries.truncate(MAX_ENTRIES);
        Ok(entries)
    }

    fn read(&self, root: &str, path: &str) -> AppResult<FileView> {
        let file = resolve(root, path)?;
        let size = fs::metadata(&file).map_err(|_| AppError("Arquivo não encontrado.".into()))?.len();
        let text = if size <= MAX_PREVIEW_BYTES { read_text(&file) } else { None };
        Ok(FileView { path: path.to_owned(), size, text })
    }
}

fn read_text(file: &Path) -> Option<String> {
    String::from_utf8(fs::read(file).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_reads_and_stays_inside_the_folder() {
        let base = std::env::temp_dir().join(format!("relay-ws-{}", uuid::Uuid::new_v4()));
        let root = base.join("projeto");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("README.md"), "# Projeto").unwrap();
        fs::write(root.join("logo.png"), [0x89, 0xff, 0x00]).unwrap();
        fs::write(base.join("segredo.txt"), "fora").unwrap();
        let root = root.to_string_lossy().into_owned();

        let top = FsWorkspace.entries(&root, "").unwrap();
        let names: Vec<&str> = top.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["src", "logo.png", "README.md"], "pastas primeiro, .git oculto");
        assert_eq!(FsWorkspace.entries(&root, "src").unwrap()[0].path, "src/main.rs");

        let code = FsWorkspace.read(&root, "src/main.rs").unwrap();
        assert_eq!((code.text.as_deref(), code.size), (Some("fn main() {}\n"), 13));
        assert_eq!(FsWorkspace.read(&root, "logo.png").unwrap().text, None, "binário não tem prévia");

        assert!(FsWorkspace.read(&root, "../segredo.txt").is_err(), "não lê fora da pasta");
        assert!(FsWorkspace.entries(&root, "..").is_err());
        fs::remove_dir_all(base).unwrap();
    }
}
