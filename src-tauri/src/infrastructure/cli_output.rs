//! Leitura da saída dos CLIs e abertura da página de login no navegador padrão.
//!
//! No Windows o CLI do Claude roda sem console e não consegue abrir o navegador:
//! ele só imprime o endereço de login. O Lume lê a saída e abre esse endereço.

use std::{
    process::{ExitStatus, Output, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

/// Quanto esperar as saídas fecharem depois que o processo termina. Um processo filho
/// do CLI (o navegador, por exemplo) pode herdar as saídas e mantê-las abertas.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// Endereço de autorização (OAuth) que o CLI imprime para o caso de o navegador não abrir.
/// Ignora os outros endereços (do servidor do conector, avisos de atualização, o
/// `http://localhost` do retorno) e os códigos de cor e de link do terminal.
pub(crate) fn login_url(line: &str) -> Option<String> {
    // Códigos de terminal (ESC, BEL) viram espaço: assim um link OSC-8 ou uma cor
    // colada no endereço não entram nele.
    let clean: String = line.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    clean.match_indices("https://").find_map(|(start, _)| {
        let url = clean[start..].split_whitespace().next()?;
        let url = url.trim_end_matches(['.', ',', ';', ')', ']', '>', '"', '\'']);
        // Todo pedido de autorização OAuth leva `client_id`; um endereço cortado antes dele não serve.
        url.contains("client_id=").then(|| url.to_owned())
    })
}

/// Abre no navegador padrão, uma única vez, a página de login que o CLI imprimir.
/// Só age no Windows: nos demais sistemas o próprio CLI abre o navegador.
pub(crate) struct LoginPage {
    enabled: bool,
    opened: AtomicBool,
}

impl LoginPage {
    pub fn new(open_in_browser: bool) -> Self {
        LoginPage { enabled: open_in_browser && cfg!(windows), opened: AtomicBool::new(false) }
    }

    fn see(&self, line: &[u8]) {
        if !self.enabled || self.opened.load(Ordering::SeqCst) {
            return;
        }
        if let Some(url) = login_url(&String::from_utf8_lossy(line))
            && !self.opened.swap(true, Ordering::SeqCst)
        {
            open_in_browser(&url);
        }
    }
}

fn open_in_browser(url: &str) {
    #[cfg(windows)]
    let _ = tauri_plugin_opener::open_url(url, None::<&str>);
    #[cfg(not(windows))]
    let _ = url;
}

/// Saída de um processo lida aos poucos; quem espera o processo pega o que já chegou
/// mesmo que a leitura não tenha terminado.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    fn push(&self, line: &[u8], page: &LoginPage) {
        page.see(line);
        self.0.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(line);
    }

    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Saídas de um processo do CLI, lidas em paralelo para ele nunca bloquear com o buffer cheio.
pub(crate) struct Outputs {
    stdout: Captured,
    stderr: Captured,
}

impl Outputs {
    fn into_output(self, status: ExitStatus) -> Output {
        Output { status, stdout: self.stdout.take(), stderr: self.stderr.take() }
    }
}

/// Mensagem de erro do CLI: a saída de erro, ou a padrão quando a de erro vem vazia.
pub(crate) fn failure_detail(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !stderr.is_empty() {
        return stderr;
    }
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

// ---- versão assíncrona (tokio) ----

/// Tarefa de leitura que é cancelada se quem a criou desistir (ex.: tempo esgotado).
struct Reader(tokio::task::JoinHandle<()>);

impl Drop for Reader {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn spawn_reader<R: AsyncRead + Unpin + Send + 'static>(pipe: Option<R>, sink: Captured, page: Arc<LoginPage>) -> Reader {
    Reader(tokio::spawn(async move {
        let Some(pipe) = pipe else { return };
        let mut reader = BufReader::new(pipe);
        let mut line = Vec::new();
        while reader.read_until(b'\n', &mut line).await.is_ok_and(|n| n > 0) {
            sink.push(&line, &page);
            line.clear();
        }
    }))
}

/// Executa o login de um CLI (que espera o navegador concluir) e devolve a saída.
/// Com `open_page`, abre a página de login no navegador padrão (ver `LoginPage`).
pub(crate) async fn run_login(mut cmd: tokio::process::Command, open_page: bool) -> std::io::Result<Output> {
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let page = Arc::new(LoginPage::new(open_page));
    let outputs = Outputs { stdout: Captured::default(), stderr: Captured::default() };
    let mut readers = [
        spawn_reader(child.stdout.take(), outputs.stdout.clone(), page.clone()),
        spawn_reader(child.stderr.take(), outputs.stderr.clone(), page),
    ];
    let status = child.wait().await?;
    for reader in &mut readers {
        let _ = tokio::time::timeout(DRAIN_GRACE, &mut reader.0).await;
    }
    Ok(outputs.into_output(status))
}

// ---- versão bloqueante (threads) ----

fn read_lines_blocking<R: std::io::Read + Send + 'static>(
    pipe: Option<R>,
    sink: Captured,
    page: Arc<LoginPage>,
) -> Option<std::thread::JoinHandle<()>> {
    let pipe = pipe?;
    Some(std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(pipe);
        let mut line = Vec::new();
        while reader.read_until(b'\n', &mut line).is_ok_and(|n| n > 0) {
            sink.push(&line, &page);
            line.clear();
        }
    }))
}

/// Processo bloqueante com as saídas sendo lidas em segundo plano.
pub(crate) struct BlockingRun {
    outputs: Outputs,
    readers: Vec<std::thread::JoinHandle<()>>,
}

impl BlockingRun {
    /// Começa a ler as saídas do processo; com `open_page`, abre a página de login.
    pub fn watch(child: &mut std::process::Child, open_page: bool) -> Self {
        let page = Arc::new(LoginPage::new(open_page));
        let outputs = Outputs { stdout: Captured::default(), stderr: Captured::default() };
        let readers = [
            read_lines_blocking(child.stdout.take(), outputs.stdout.clone(), page.clone()),
            read_lines_blocking(child.stderr.take(), outputs.stderr.clone(), page),
        ];
        BlockingRun { outputs, readers: readers.into_iter().flatten().collect() }
    }

    /// Depois que o processo terminou: espera um pouco as saídas fecharem e devolve o
    /// que foi lido. Uma leitura presa por um processo filho é abandonada.
    pub fn finish(self, status: ExitStatus) -> Output {
        let started = Instant::now();
        while !self.readers.iter().all(|r| r.is_finished()) && started.elapsed() < DRAIN_GRACE {
            std::thread::sleep(Duration::from_millis(20));
        }
        self.outputs.into_output(status)
    }
}

#[cfg(test)]
mod tests {
    use super::login_url;

    #[test]
    fn finds_authorization_url_printed_by_the_cli() {
        assert_eq!(
            login_url("If the browser didn't open, visit: https://claude.ai/oauth/authorize?code=true&client_id=abc&state=x").as_deref(),
            Some("https://claude.ai/oauth/authorize?code=true&client_id=abc&state=x")
        );
        assert_eq!(
            login_url("navigate to this URL: https://auth.openai.com/oauth/authorize?client_id=a&state=b.").as_deref(),
            Some("https://auth.openai.com/oauth/authorize?client_id=a&state=b")
        );
    }

    #[test]
    fn ignores_addresses_that_are_not_authorization() {
        assert_eq!(login_url("Starting local login server on http://localhost:1455."), None);
        assert_eq!(login_url("Opening browser to sign in…"), None);
        assert_eq!(login_url("Authenticating with https://mcp.notion.com/mcp ..."), None);
        assert_eq!(login_url("Update available: https://github.com/anthropics/claude-code"), None);
        // Endereço cortado antes do client_id (quebrado em duas linhas).
        assert_eq!(login_url("visit: https://claude.ai/oauth/authorize?code=true&resp"), None);
    }

    #[test]
    fn picks_authorization_url_after_other_addresses_on_the_same_line() {
        assert_eq!(
            login_url("Server https://mcp.exemplo.com · open https://auth.exemplo.com/authorize?client_id=1").as_deref(),
            Some("https://auth.exemplo.com/authorize?client_id=1")
        );
    }

    #[test]
    fn strips_terminal_codes_around_the_url() {
        let osc8 = "\x1b]8;;https://claude.ai/oauth/authorize?client_id=a\x07Click here\x1b]8;;\x07";
        assert_eq!(login_url(osc8).as_deref(), Some("https://claude.ai/oauth/authorize?client_id=a"));
        let colored = "\x1b[36mhttps://auth.openai.com/oauth/authorize?client_id=a\x1b[0m";
        assert_eq!(login_url(colored).as_deref(), Some("https://auth.openai.com/oauth/authorize?client_id=a"));
    }
}
