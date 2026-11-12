//! Projetos: instruções e arquivos de conhecimento compartilhados por um grupo de conversas.

use serde::Serialize;

/// Limite do conhecimento do projeto enviado em cada turno, em caracteres.
pub const MAX_KNOWLEDGE_CHARS: usize = 150_000;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub instructions: String,
    /// Pasta do projeto no computador; as sessões de agente do projeto trabalham nela.
    pub folder: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectFile {
    pub id: String,
    pub name: String,
    /// Tamanho do conteúdo, em caracteres.
    pub size: usize,
}

/// Caminho de pasta sem barra final, para comparar pastas de forma estável.
pub fn normalize_folder(folder: &str) -> String {
    let trimmed = folder.trim();
    let without_slash = trimmed.trim_end_matches(['/', '\\']);
    if without_slash.is_empty() { trimmed.to_owned() } else { without_slash.to_owned() }
}

/// Nome sugerido para o projeto de uma pasta: o último trecho do caminho.
pub fn folder_name(folder: &str) -> String {
    let normalized = normalize_folder(folder);
    normalized.rsplit(['/', '\\']).next().filter(|n| !n.is_empty()).unwrap_or(&normalized).to_owned()
}

/// Texto anexado ao prompt de sistema: instruções do projeto e seus arquivos.
/// Arquivos que não cabem no limite ficam de fora, inteiros.
pub fn project_prompt(project: &Project, files: &[(String, String)]) -> Option<String> {
    let mut out = String::new();
    if !project.instructions.trim().is_empty() {
        out.push_str(&format!("Instruções do projeto \"{}\":\n{}\n", project.name, project.instructions.trim()));
    }
    let mut used = 0;
    for (name, content) in files {
        let size = content.chars().count();
        if used + size > MAX_KNOWLEDGE_CHARS {
            continue;
        }
        used += size;
        out.push_str(&format!("\nArquivo de conhecimento do projeto — {name}:\n{content}\n"));
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(instructions: &str) -> Project {
        Project { id: "p".into(), name: "Portal".into(), instructions: instructions.into(), folder: None }
    }

    #[test]
    fn joins_instructions_and_files() {
        let prompt = project_prompt(&project("Responda em português."), &[("regras.md".into(), "Use REST.".into())]).unwrap();
        assert!(prompt.contains("Instruções do projeto \"Portal\":\nResponda em português."));
        assert!(prompt.contains("regras.md:\nUse REST."));
        assert_eq!(project_prompt(&project("  "), &[]), None);
    }

    #[test]
    fn names_project_after_its_folder() {
        assert_eq!(normalize_folder(" /Users/ana/Dev/loja/ "), "/Users/ana/Dev/loja");
        assert_eq!(folder_name("/Users/ana/Dev/loja/"), "loja");
        assert_eq!(folder_name("C:\\Dev\\portal"), "portal");
        assert_eq!(folder_name("/"), "/");
    }

    #[test]
    fn skips_files_beyond_the_limit() {
        let big = "x".repeat(MAX_KNOWLEDGE_CHARS);
        let files = [("grande.txt".into(), big), ("extra.txt".into(), "y".into())];
        let prompt = project_prompt(&project(""), &files).unwrap();
        assert!(prompt.contains("grande.txt") && !prompt.contains("extra.txt"));
    }
}
