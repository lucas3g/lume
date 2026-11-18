//! Controle de versão pela linha de comando do git.

use super::{claude_cli::shell_path, hide_console, workspace_fs::resolve};
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
    git_with(root, &[], args)
}

/// Como `git`, com variáveis de ambiente a mais (ex.: um índice temporário).
fn git_with(root: &str, envs: &[(&str, &std::ffi::OsStr)], args: &[&str]) -> Option<String> {
    let output = hide_console(&mut Command::new("git")).arg("-C").arg(root).envs(envs.iter().copied()).args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_or_error(root: &str, args: &[&str]) -> AppResult<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(root).args(args);
    run_or_error(&mut cmd, "git")
}

/// Roda o comando com o PATH do shell do usuário (o app aberto pelo Finder não o tem), para o
/// git achar os ajudantes de login (ex.: `gh`) e para achar o próprio `gh`.
fn with_shell_path(cmd: &mut Command) -> &mut Command {
    if let Some(path) = shell_path() {
        cmd.env("PATH", path);
    }
    cmd
}

fn run_or_error(cmd: &mut Command, name: &str) -> AppResult<String> {
    let output = hide_console(cmd).output().map_err(|e| AppError(format!("Não foi possível executar o {name}: {e}")))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = if detail.trim().is_empty() { String::from_utf8_lossy(&output.stdout) } else { detail };
    Err(AppError(format!("O {name} recusou a operação: {}", detail.trim())))
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

    fn snapshot(&self, root: &str) -> Option<String> {
        // Um índice temporário recebe a pasta e vira uma árvore no repositório; o índice de verdade fica
        // intocado. Ele começa como cópia do real, para o git não reler os arquivos que não mudaram.
        // Dos arquivos novos, entram só os pequenos (como no diff) e no máximo `MAX_FILES`: cada um
        // vira um objeto no repositório a cada foto, e uma pasta de build esquecida fora do .gitignore
        // encheria o repositório.
        let index = std::env::temp_dir().join(format!("lume-index-{}", uuid::Uuid::new_v4()));
        let with_index = |args: &[&str]| git_with(root, &[("GIT_INDEX_FILE", index.as_os_str())], args).map(|o| o.trim().to_owned());
        let tree = (|| {
            let real = git(root, &["rev-parse", "--git-path", "index"])?;
            let real = std::path::Path::new(root).join(real.trim());
            if fs::copy(&real, &index).is_err() && has_commits(root) {
                with_index(&["read-tree", "HEAD"])?;
            }
            with_index(&["add", "-u", "--", "."])?;
            let others = git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
            let small: Vec<&str> = others
                .split('\0')
                .filter(|p| !p.is_empty())
                .filter(|p| fs::metadata(std::path::Path::new(root).join(p)).is_ok_and(|m| m.len() <= MAX_COUNTED_BYTES))
                .take(MAX_FILES)
                .collect();
            for chunk in small.chunks(100) {
                let args: Vec<&str> = ["add", "--"].into_iter().chain(chunk.iter().copied()).collect();
                with_index(&args)?;
            }
            with_index(&["write-tree"])
        })();
        let _ = fs::remove_file(&index);
        tree.filter(|t| !t.is_empty())
    }

    fn diff_between(&self, root: &str, from: &str, to: &str) -> AppResult<String> {
        git_or_error(root, &["diff", from, to])
    }

    fn commit(&self, root: &str, message: &str) -> AppResult<()> {
        git_or_error(root, &["add", "-A"])?;
        git_or_error(root, &["commit", "-m", message])?;
        Ok(())
    }

    fn add_worktree(&self, root: &str, path: &str, branch: &str) -> AppResult<()> {
        if !self.status(root).is_repo {
            return Err(AppError("A pasta do projeto não é um repositório git.".into()));
        }
        git_or_error(root, &["worktree", "add", "-b", branch, path]).map(|_| ())
    }

    fn worktrees(&self, root: &str) -> AppResult<Vec<String>> {
        let listing = git(root, &["worktree", "list", "--porcelain"]).unwrap_or_default();
        Ok(listing.lines().filter_map(|l| l.strip_prefix("worktree ")).map(str::to_owned).collect())
    }

    fn has_branch(&self, root: &str, branch: &str) -> bool {
        git(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_some()
    }

    fn is_repo(&self, root: &str) -> bool {
        git(root, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|o| o.trim() == "true")
    }

    fn current_branch(&self, root: &str) -> Option<String> {
        git(root, &["branch", "--show-current"]).map(|b| b.trim().to_owned()).filter(|b| !b.is_empty())
    }

    fn remote(&self, root: &str) -> Option<String> {
        let listing = git(root, &["remote"])?;
        let remotes: Vec<&str> = listing.lines().map(str::trim).filter(|r| !r.is_empty()).collect();
        remotes.iter().find(|r| **r == "origin").or(remotes.first()).map(|r| (*r).to_owned())
    }

    fn create_branch(&self, root: &str, branch: &str) -> AppResult<()> {
        git_or_error(root, &["checkout", "-b", branch]).map(|_| ())
    }

    fn merge(&self, root: &str, branch: &str) -> AppResult<()> {
        // Um merge seu pela metade não pode ser desfeito pelo `--abort` de um conflito nosso.
        if git(root, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_some() {
            return Err(AppError(format!("A pasta {root} está no meio de um merge: termine-o ou desfaça-o antes.")));
        }
        if git(root, &["merge", "--ff-only", "--quiet", branch]).is_some() {
            return Ok(());
        }
        let merged = git_or_error(root, &["merge", "--no-edit", "--quiet", branch]);
        if merged.is_err() {
            // Conflito (ou outro motivo): a pasta volta a ficar como estava.
            let _ = git(root, &["merge", "--abort"]);
        }
        merged.map(|_| ()).map_err(|e| AppError(format!("Não deu para juntar o ramo {branch} em {root}: {}", e.0)))
    }

    fn push(&self, root: &str, remote: &str, branch: &str) -> AppResult<()> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(root).args(["push", "-u", remote, branch]);
        run_or_error(with_shell_path(&mut cmd), "git").map(|_| ())
    }

    fn remove_worktree(&self, root: &str, path: &str, branch: &str) -> AppResult<()> {
        git_or_error(root, &["worktree", "remove", path])?;
        git_or_error(root, &["branch", "-d", branch]).map(|_| ())
    }

    fn head(&self, root: &str) -> Option<String> {
        git(root, &["rev-parse", "HEAD"]).map(|h| h.trim().to_owned()).filter(|h| !h.is_empty())
    }

    fn commits_since(&self, root: &str, commit: &str) -> u32 {
        git(root, &["rev-list", "--count", &format!("{commit}..HEAD")]).and_then(|n| n.trim().parse().ok()).unwrap_or(0)
    }

    fn checkout(&self, root: &str, branch: &str) -> AppResult<()> {
        git_or_error(root, &["checkout", "-q", branch]).map(|_| ())
    }

    fn pull(&self, root: &str, remote: &str, branch: &str) -> AppResult<()> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(root).args(["pull", "--ff-only", "-q", remote, branch]);
        run_or_error(with_shell_path(&mut cmd), "git").map(|_| ())
    }

    fn delete_branch(&self, root: &str, branch: &str) -> AppResult<()> {
        git_or_error(root, &["branch", "-d", branch]).map(|_| ())
    }

    fn merge_pr(&self, root: &str, url: &str, remote: &str, branch: &str) -> AppResult<()> {
        let mut cmd = Command::new("gh");
        cmd.current_dir(root).args(["pr", "merge", url, "--merge"]);
        run_or_error(with_shell_path(&mut cmd), "gh")?;
        // O GitHub pode já ter apagado o ramo sozinho: a falha aqui não desfaz o merge.
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(root).args(["push", "-q", remote, "--delete", branch]);
        let _ = run_or_error(with_shell_path(&mut cmd), "git");
        Ok(())
    }

    fn gh_ready(&self) -> bool {
        let mut cmd = Command::new("gh");
        cmd.args(["auth", "status"]);
        hide_console(with_shell_path(&mut cmd)).output().is_ok_and(|o| o.status.success())
    }

    fn open_pr(&self, root: &str, base: &str, head: &str, title: &str, body: &str) -> AppResult<String> {
        let mut cmd = Command::new("gh");
        cmd.current_dir(root).args(["pr", "create", "--base", base, "--head", head, "--title", title, "--body", body]);
        let out = run_or_error(with_shell_path(&mut cmd), "gh")?;
        // O `gh` escreve o link do pull request na última linha.
        Ok(out.lines().rev().map(str::trim).find(|l| l.starts_with("http")).unwrap_or(out.trim()).to_owned())
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

        assert_eq!(GitCli.worktrees(&root).unwrap().len(), 1, "só a cópia principal");
        let branch_dir = dir.with_file_name(format!("{}-ramo", dir.file_name().unwrap().to_string_lossy()));
        GitCli.add_worktree(&root, &branch_dir.to_string_lossy(), "lume/ramo").unwrap();
        assert_eq!(GitCli.worktrees(&root).unwrap().len(), 2);
        assert!(GitCli.add_worktree(&root, &branch_dir.to_string_lossy(), "lume/ramo").is_err(), "nome repetido é recusado");
        fs::remove_dir_all(&branch_dir).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    fn repo(name: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("lume-git-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let root = dir.to_string_lossy().into_owned();
        git(&root, &["init", "-q", "-b", "main"]).unwrap();
        git(&root, &["config", "user.email", "teste@exemplo.com"]).unwrap();
        git(&root, &["config", "user.name", "Teste"]).unwrap();
        fs::write(dir.join("a.txt"), "um\n").unwrap();
        GitCli.commit(&root, "Primeiro").unwrap();
        (dir, root)
    }

    /// Entrega na main: o commit da cópia isolada chega à pasta principal e a cópia some.
    #[test]
    fn merges_the_worktree_branch_and_removes_the_copy() {
        let (dir, root) = repo("merge");
        assert!(GitCli.is_repo(&root) && GitCli.remote(&root).is_none());
        assert_eq!(GitCli.current_branch(&root).as_deref(), Some("main"));
        let copy = dir.with_file_name(format!("{}-esteira", dir.file_name().unwrap().to_string_lossy()));
        let copy_root = copy.to_string_lossy().into_owned();
        GitCli.add_worktree(&root, &copy_root, "lume/esteira").unwrap();
        fs::write(copy.join("b.txt"), "novo\n").unwrap();
        GitCli.commit(&copy_root, "Exporta CSV").unwrap();

        GitCli.merge(&root, "lume/esteira").unwrap();
        assert!(dir.join("b.txt").exists(), "avançou a main");
        GitCli.remove_worktree(&root, &copy_root, "lume/esteira").unwrap();
        assert!(!copy.exists() && !GitCli.has_branch(&root, "lume/esteira"));
        fs::remove_dir_all(dir).unwrap();
    }

    /// Conflito no merge: a pasta principal volta a ficar como estava.
    #[test]
    fn conflicting_merge_is_undone() {
        let (dir, root) = repo("conflito");
        GitCli.create_branch(&root, "lume/ramo").unwrap();
        fs::write(dir.join("a.txt"), "do ramo\n").unwrap();
        GitCli.commit(&root, "Ramo").unwrap();
        git(&root, &["checkout", "-q", "main"]).unwrap();
        fs::write(dir.join("a.txt"), "da main\n").unwrap();
        GitCli.commit(&root, "Main").unwrap();

        assert!(GitCli.merge(&root, "lume/ramo").is_err());
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "da main\n");
        assert!(GitCli.status(&root).files.is_empty(), "sem merge pela metade");

        // Um merge do usuário pela metade fica como está.
        assert!(git(&root, &["merge", "lume/ramo"]).is_none());
        assert!(GitCli.merge(&root, "lume/ramo").unwrap_err().0.contains("no meio de um merge"));
        assert!(git(&root, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_some(), "o merge dele continua");
        fs::remove_dir_all(dir).unwrap();
    }

    /// Fotos da pasta: o diff entre elas traz só o que mudou no meio, arquivos novos inclusive,
    /// sem tocar no que o usuário deixou preparado para commit.
    #[test]
    fn snapshots_show_only_what_changed_between_them() {
        let dir = std::env::temp_dir().join(format!("lume-git-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let root = dir.to_string_lossy().into_owned();
        assert_eq!(GitCli.snapshot(&root), None, "pasta comum não tem foto");
        git(&root, &["init", "-q"]).unwrap();
        git(&root, &["config", "user.email", "teste@exemplo.com"]).unwrap();
        git(&root, &["config", "user.name", "Teste"]).unwrap();
        fs::write(dir.join("a.txt"), "um\n").unwrap();
        GitCli.commit(&root, "Primeiro").unwrap();

        fs::write(dir.join("a.txt"), "um\ndois\n").unwrap();
        git(&root, &["add", "a.txt"]).unwrap();
        let first = GitCli.snapshot(&root).unwrap();
        fs::write(dir.join("a.txt"), "um\ndois\ntrês\n").unwrap();
        fs::write(dir.join("novo.txt"), "novo\n").unwrap();
        fs::write(dir.join("grande.bin"), vec![b'x'; MAX_COUNTED_BYTES as usize + 1]).unwrap();
        let second = GitCli.snapshot(&root).unwrap();

        let delta = GitCli.diff_between(&root, &first, &second).unwrap();
        assert!(delta.contains("+três") && delta.contains("+novo"), "{delta}");
        assert!(!delta.contains("+dois"), "o que já estava na primeira foto não aparece");
        assert!(!delta.contains("grande.bin"), "arquivo novo grande não vai para o repositório");
        let staged = git(&root, &["diff", "--cached", "--name-only"]).unwrap();
        assert_eq!(staged.trim(), "a.txt", "o índice do usuário fica como estava");
        assert!(GitCli.status(&root).files.iter().any(|f| f.path == "novo.txt" && f.status == "?"), "o arquivo novo continua fora do git");
        fs::remove_dir_all(dir).unwrap();
    }
}
