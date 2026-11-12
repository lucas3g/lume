//! Pasta de trabalho de uma sessão de agente: arquivos e alterações.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileEntry {
    pub name: String,
    /// Caminho relativo à pasta da sessão, com `/` como separador.
    pub path: String,
    pub is_dir: bool,
}

/// Um arquivo aberto no explorador. Sem `text` quando não é texto ou é grande demais.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileView {
    pub path: String,
    pub size: u64,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChangedFile {
    pub path: String,
    /// `M` modificado, `A` adicionado, `D` removido, `R` renomeado, `?` novo e não rastreado.
    pub status: String,
    pub added: u32,
    pub removed: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct WorkspaceStatus {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub added: u32,
    pub removed: u32,
    pub files: Vec<ChangedFile>,
}

impl WorkspaceStatus {
    pub fn with_files(branch: Option<String>, files: Vec<ChangedFile>) -> WorkspaceStatus {
        WorkspaceStatus {
            is_repo: true,
            branch,
            added: files.iter().map(|f| f.added).sum(),
            removed: files.iter().map(|f| f.removed).sum(),
            files,
        }
    }
}

/// Percentual do contexto do modelo já ocupado pela conversa.
pub fn context_percent(used_tokens: u64, window_tokens: u64) -> Option<i64> {
    (window_tokens > 0).then(|| ((used_tokens * 100) / window_tokens).min(100) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_and_context_percent() {
        let file = |added, removed| ChangedFile { path: "a".into(), status: "M".into(), added, removed };
        let status = WorkspaceStatus::with_files(Some("main".into()), vec![file(10, 2), file(5, 0)]);
        assert_eq!((status.added, status.removed, status.is_repo), (15, 2, true));
        assert_eq!(context_percent(370_000, 1_000_000), Some(37));
        assert_eq!(context_percent(2_000_000, 1_000_000), Some(100));
        assert_eq!(context_percent(10, 0), None);
    }
}
