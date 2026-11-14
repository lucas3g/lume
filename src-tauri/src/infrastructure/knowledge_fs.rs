//! Arquivos de conhecimento dos projetos copiados para disco, um projeto por pasta,
//! para o assistente ler sob demanda.

use crate::application::{ports::KnowledgeFolder, AppError, AppResult};
use std::{fs, path::PathBuf};

pub struct FsKnowledge {
    /// `<dados do app>/knowledge`.
    root: PathBuf,
}

impl FsKnowledge {
    pub fn new(root: PathBuf) -> Self {
        FsKnowledge { root }
    }

    fn dir(&self, project_id: &str) -> AppResult<PathBuf> {
        if project_id.is_empty() || !project_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(AppError("Projeto inválido.".into()));
        }
        Ok(self.root.join(project_id))
    }
}

fn io(e: std::io::Error) -> AppError {
    AppError(format!("Não foi possível gravar o conhecimento do projeto: {e}"))
}

/// Nome simples, sem pasta: o que vem de fora nunca escreve fora da pasta do projeto.
fn plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\'])
}

impl KnowledgeFolder for FsKnowledge {
    fn sync(&self, project_id: &str, files: &[(String, String)]) -> AppResult<String> {
        let dir = self.dir(project_id)?;
        if let Some((name, _)) = files.iter().find(|(name, _)| !plain(name)) {
            return Err(AppError(format!("{name}: nome de arquivo inválido.")));
        }
        fs::create_dir_all(&dir).map_err(io)?;
        // Primeiro sai o que não é mais do projeto, comparando o nome exato: num disco que não
        // distingue maiúsculas, "regras.md" sai antes de "Regras.md" ser gravado com o nome novo.
        for entry in fs::read_dir(&dir).map_err(io)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !files.iter().any(|(n, _)| *n == name) {
                let path = entry.path();
                let _ = if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
            }
        }
        for (name, content) in files {
            let path = dir.join(name);
            // Só regrava o que mudou, e por troca de arquivo: quem estiver lendo nunca vê metade.
            if fs::read_to_string(&path).ok().as_deref() != Some(content.as_str()) {
                let tmp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
                fs::write(&tmp, content).and_then(|_| fs::rename(&tmp, &path)).map_err(|e| {
                    let _ = fs::remove_file(&tmp);
                    io(e)
                })?;
            }
        }
        Ok(dir.to_string_lossy().into_owned())
    }

    fn remove(&self, project_id: &str) -> AppResult<()> {
        let dir = self.dir(project_id)?;
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(io)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_exactly_the_project_files_on_disk() {
        let root = std::env::temp_dir().join(format!("lume-knowledge-{}", uuid::Uuid::new_v4()));
        let knowledge = FsKnowledge::new(root.clone());
        let files = [("regras.md".to_owned(), "Use REST.".to_owned()), ("precos.csv".to_owned(), "a;1".to_owned())];
        let dir = knowledge.sync("p1", &files).unwrap();
        assert_eq!(PathBuf::from(&dir), root.join("p1"));
        assert_eq!(fs::read_to_string(root.join("p1/regras.md")).unwrap(), "Use REST.");

        knowledge.sync("p1", &[("regras.md".to_owned(), "Use GraphQL.".to_owned())]).unwrap();
        assert_eq!(fs::read_to_string(root.join("p1/regras.md")).unwrap(), "Use GraphQL.");
        assert!(!root.join("p1/precos.csv").exists(), "o arquivo removido do projeto sai da pasta");

        knowledge.sync("p1", &[("Regras.md".to_owned(), "Use GraphQL.".to_owned())]).unwrap();
        let names: Vec<String> = fs::read_dir(root.join("p1")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Regras.md"], "só muda a caixa: o arquivo fica com o nome novo, sem sobras");

        assert!(knowledge.sync("p1", &[("../fora.md".to_owned(), "x".to_owned())]).is_err());
        assert!(knowledge.sync("../p1", &[]).is_err());
        knowledge.remove("p1").unwrap();
        assert!(!root.join("p1").exists());
        knowledge.remove("p1").unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
