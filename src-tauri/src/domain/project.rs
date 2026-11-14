//! Projetos: instruções e arquivos de conhecimento compartilhados por um grupo de conversas.

use serde::Serialize;

/// Limite do conhecimento guardado por projeto, em caracteres.
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

/// Arquivo de memória do Claude Code em uma pasta de projeto.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemoryFile {
    pub name: String,
    pub size: u64,
    pub modified: i64,
}

/// Nome da pasta em que o Claude Code guarda os dados de um diretório de trabalho
/// (memória e transcrições): o caminho com tudo que não é letra ou dígito trocado por "-".
pub fn claude_project_dir_name(folder: &str) -> String {
    normalize_folder(folder).chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// Nome de arquivo de memória aceitável: um `.md` simples, sem caminho.
pub fn valid_memory_name(name: &str) -> bool {
    !name.is_empty()
        && name.ends_with(".md")
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
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

/// Instruções do projeto, quando há.
pub fn project_rules(project: &Project) -> Option<String> {
    let rules = project.instructions.trim();
    (!rules.is_empty()).then(|| format!("Instruções do projeto \"{}\":\n{rules}\n", project.name))
}

/// Nome de arquivo seguro para gravar o conhecimento em disco: sem pastas nem caracteres
/// que o sistema recusa; nomes repetidos ganham um número.
pub fn knowledge_file_names(names: &[String]) -> Vec<String> {
    let mut used: Vec<String> = vec![];
    for name in names {
        let base: String = name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or_default()
            .chars()
            .map(|c| if c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
            .collect();
        let base = base.trim().trim_start_matches('.').to_owned();
        let base = if base.is_empty() { "arquivo.txt".to_owned() } else { base };
        let (stem, ext) = match base.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
            _ => (base.clone(), String::new()),
        };
        let mut candidate = base.clone();
        let mut n = 2;
        // O disco do macOS e do Windows não distingue maiúsculas (inclusive acentuadas).
        while used.iter().any(|u| u.to_lowercase() == candidate.to_lowercase()) {
            candidate = format!("{stem} ({n}){ext}");
            n += 1;
        }
        used.push(candidate);
    }
    used
}

/// Conteúdo dos arquivos de conhecimento no próprio prompt, até o limite; os que não cabem
/// ficam de fora, inteiros. Usado quando os arquivos não puderam ser gravados em disco.
pub fn project_knowledge_inline(files: &[(String, String)]) -> Option<String> {
    let mut out = String::new();
    let mut used = 0;
    for (name, content) in files {
        let size = content.chars().count();
        if used + size > MAX_KNOWLEDGE_CHARS {
            continue;
        }
        used += size;
        out.push_str(&format!("Arquivo de conhecimento do projeto — {name}:\n{content}\n"));
    }
    (!out.is_empty()).then_some(out)
}

/// Aviso com a pasta e a lista dos arquivos de conhecimento: o conteúdo não vai no
/// prompt, o assistente lê só o que precisar. `files` é (nome em disco, caracteres).
pub fn project_knowledge(dir: &str, files: &[(String, usize)]) -> Option<String> {
    if files.is_empty() {
        return None;
    }
    let list: String = files.iter().map(|(name, size)| format!("- {name} ({size} caracteres)\n")).collect();
    Some(format!(
        "Arquivos de conhecimento do projeto, na pasta {dir}:\n{list}\
         Eles não estão neste prompt: quando a pergunta depender deles, leia só os relevantes \
         (ou procure neles por um trecho) antes de responder.\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(instructions: &str) -> Project {
        Project { id: "p".into(), name: "Portal".into(), instructions: instructions.into(), folder: None }
    }

    #[test]
    fn joins_instructions_and_files() {
        let rules = project_rules(&project("Responda em português.")).unwrap();
        assert!(rules.contains("Instruções do projeto \"Portal\":\nResponda em português."));
        let knowledge = project_knowledge("/dados/knowledge/p", &[("regras.md".into(), 9)]).unwrap();
        assert!(knowledge.contains("na pasta /dados/knowledge/p:\n- regras.md (9 caracteres)"));
        assert_eq!((project_rules(&project("  ")), project_knowledge("/x", &[])), (None, None));
    }

    #[test]
    fn names_project_after_its_folder() {
        assert_eq!(normalize_folder(" /Users/ana/Dev/loja/ "), "/Users/ana/Dev/loja");
        assert_eq!(folder_name("/Users/ana/Dev/loja/"), "loja");
        assert_eq!(folder_name("C:\\Dev\\portal"), "portal");
        assert_eq!(folder_name("/"), "/");
    }

    #[test]
    fn claude_project_dir_replaces_everything_but_letters_and_digits() {
        assert_eq!(claude_project_dir_name("/Users/ana/Dev/app.v2/"), "-Users-ana-Dev-app-v2");
        assert_eq!(claude_project_dir_name("C:\\Dev\\portal"), "C--Dev-portal");
        assert!(valid_memory_name("MEMORY.md") && valid_memory_name("clean-arch_v2.md"));
        assert!(!valid_memory_name("../x.md") && !valid_memory_name("notas.txt") && !valid_memory_name(".oculto.md"));
    }

    #[test]
    fn knowledge_names_are_plain_and_unique() {
        let names = ["regras.md", "../segredo.md", "regras.md", "REGRAS.md", "a:b?.txt", ".env", "", "ação.md", "AÇÃO.md"].map(String::from);
        assert_eq!(
            knowledge_file_names(&names),
            ["regras.md", "segredo.md", "regras (2).md", "REGRAS (3).md", "a_b_.txt", "env", "arquivo.txt", "ação.md", "AÇÃO (2).md"]
        );
    }

    #[test]
    fn inline_knowledge_skips_files_beyond_the_limit() {
        let files = [("grande.txt".into(), "x".repeat(MAX_KNOWLEDGE_CHARS)), ("extra.txt".into(), "y".into())];
        let prompt = project_knowledge_inline(&files).unwrap();
        assert!(prompt.contains("grande.txt") && !prompt.contains("extra.txt"));
        assert_eq!(project_knowledge_inline(&[]), None);
    }
}
