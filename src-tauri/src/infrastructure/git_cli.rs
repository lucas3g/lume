//! Controle de versão pela linha de comando do git.

use super::workspace_fs::resolve;
use crate::application::{ports::VersionControl, AppError, AppResult};
use crate::domain::workspace::{ChangedFile, WorkspaceStatus};
use std::{collections::HashMap, fs, process::Command};

const MAX_FILES: usize = 500;
/// Arquivos novos maiores que isso entram na lista sem contagem de linhas.
const MAX_COUNTED_BYTES: u64 = 1024 * 1024;
const MAX_NEW_FILE_DIFF_LINES: usize = 400;

pub struct GitCli;

/// Saída padrão do git, se o comando teve sucesso.
fn git(root: &str, args: &[&str]) -> Option<String> {
    let output = Command::new("git").arg("-C").arg(root).args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_or_error(root: &str, args: &[&str]) -> AppResult<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| AppError(format!("Não foi possível executar o git: {e}")))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = if detail.trim().is_empty() { String::from_utf8_lossy(&output.stdout) } else { detail };
    Err(AppError(format!("O git recusou a operação: {}", detail.trim())))
}

/// `git status --porcelain=v1 -z`: pares (situação, caminho).
fn parse_status(output: &str) -> Vec<(String, String)> {
    let mut tokens = output.split('\0').filter(|t| !t.is_empty());
    let mut files = vec![];
    while let Some(entry) = tokens.next() {
        let Some((code, path)) = entry.split_at_checked(3) else { continue };
        let code = code.trim_end();
        let status = if code == "??" {
            "?".to_owned()
        } else {
            code.chars().find(|c| *c != ' ').map(String::from).unwrap_or_default()
        };
        // Renomeações e cópias trazem o caminho antigo no item seguinte.
        if status == "R" || status == "C" {
            tokens.next();
        }
        files.push((status, path.to_owned()));
    }
    files
}

/// `git diff --numstat -z`: caminho → (linhas adicionadas, removidas).
fn parse_numstat(output: &str) -> HashMap<String, (u32, u32)> {
    let mut tokens = output.split('\0');
    let mut counts = HashMap::new();
    while let Some(entry) = tokens.next() {
        let mut fields = entry.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (fields.next(), fields.next(), fields.next()) else { continue };
        // Em renomeações o caminho vem vazio, seguido de "antigo" e "novo".
        let path = if path.is_empty() {
            tokens.next();
            tokens.next().unwrap_or_default()
        } else {
            path
        };
        counts.insert(path.to_owned(), (added.parse().unwrap_or(0), removed.parse().unwrap_or(0)));
    }
    counts
}

fn has_commits(root: &str) -> bool {
    git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_some()
}

fn new_file_text(root: &str, path: &str) -> Option<String> {
    let file = resolve(root, path).ok()?;
    if fs::metadata(&file).ok()?.len() > MAX_COUNTED_BYTES {
        return None;
    }
    String::from_utf8(fs::read(file).ok()?).ok()
}

impl VersionControl for GitCli {
    fn status(&self, root: &str) -> WorkspaceStatus {
        let Some(listing) = git(root, &["status", "--porcelain=v1", "-z", "-uall"]) else {
            return WorkspaceStatus::default();
        };
        let branch = git(root, &["branch", "--show-current"]).map(|b| b.trim().to_owned()).filter(|b| !b.is_empty());
        let counts = if has_commits(root) {
            git(root, &["diff", "HEAD", "--numstat", "-z"]).map(|o| parse_numstat(&o)).unwrap_or_default()
        } else {
            HashMap::new()
        };
        let files = parse_status(&listing)
            .into_iter()
            .take(MAX_FILES)
            .map(|(status, path)| {
                let (added, removed) = counts.get(&path).copied().unwrap_or_else(|| {
                    // Arquivo novo: todas as suas linhas contam como adicionadas.
                    (new_file_text(root, &path).map(|t| t.lines().count() as u32).unwrap_or(0), 0)
                });
                ChangedFile { path, status, added, removed }
            })
            .collect();
        WorkspaceStatus::with_files(branch, files)
    }

    fn diff(&self, root: &str, path: &str) -> AppResult<String> {
        if has_commits(root) {
            let diff = git_or_error(root, &["diff", "HEAD", "--", path])?;
            if !diff.trim().is_empty() {
                return Ok(diff);
            }
        }
        let text = new_file_text(root, path).ok_or_else(|| AppError("Sem prévia para este arquivo.".into()))?;
        Ok(text.lines().take(MAX_NEW_FILE_DIFF_LINES).map(|l| format!("+{l}\n")).collect())
    }

    fn commit(&self, root: &str, message: &str) -> AppResult<()> {
        git_or_error(root, &["add", "-A"])?;
        git_or_error(root, &["commit", "-m", message])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_and_numstat() {
        let status = parse_status(" M src/a b.rs\0?? novo.txt\0R  novo/nome.rs\0antigo.rs\0D  velho.rs\0");
        assert_eq!(
            status,
            [("M", "src/a b.rs"), ("?", "novo.txt"), ("R", "novo/nome.rs"), ("D", "velho.rs")]
                .map(|(s, p)| (s.to_owned(), p.to_owned()))
        );
        let counts = parse_numstat("3\t1\tsrc/a b.rs\0-\t-\timg.png\x005\t0\t\0antigo.rs\0novo/nome.rs\0");
        assert_eq!(counts["src/a b.rs"], (3, 1));
        assert_eq!(counts["img.png"], (0, 0));
        assert_eq!(counts["novo/nome.rs"], (5, 0));
    }

    /// Fluxo completo em um repositório temporário real.
    #[test]
    fn reports_changes_diff_and_commits() {
        let dir = std::env::temp_dir().join(format!("lume-git-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let root = dir.to_string_lossy().into_owned();
        assert!(!GitCli.status(&root).is_repo, "pasta comum não é repositório");

        git(&root, &["init", "-q", "-b", "principal"]).unwrap();
        git(&root, &["config", "user.email", "teste@exemplo.com"]).unwrap();
        git(&root, &["config", "user.name", "Teste"]).unwrap();
        fs::write(dir.join("a.txt"), "um\ndois\n").unwrap();

        let fresh = GitCli.status(&root);
        assert_eq!((fresh.branch.as_deref(), fresh.added, fresh.files[0].status.as_str()), (Some("principal"), 2, "?"));
        assert_eq!(GitCli.diff(&root, "a.txt").unwrap(), "+um\n+dois\n");

        GitCli.commit(&root, "Primeiro commit").unwrap();
        assert!(GitCli.status(&root).files.is_empty());

        fs::write(dir.join("a.txt"), "um\ntrês\nquatro\n").unwrap();
        let changed = GitCli.status(&root);
        assert_eq!((changed.added, changed.removed, changed.files[0].status.as_str()), (2, 1, "M"));
        assert!(GitCli.diff(&root, "a.txt").unwrap().contains("+três"));
        assert!(GitCli.commit(&root, "Segundo").is_ok());
        assert!(GitCli.commit(&root, "Nada a registrar").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
