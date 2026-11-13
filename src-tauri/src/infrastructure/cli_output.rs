//! Leitura da saída dos CLIs, abertura da página de login no navegador padrão e
//! entrega do código de autenticação colado pelo usuário.
//!
//! No Windows o CLI do Claude roda sem console e não consegue abrir o navegador:
//! ele só imprime o endereço de login. O Lume lê a saída e abre esse endereço.
//! Esse endereço (e às vezes o próprio login) termina numa página com um código de
//! autenticação, que o CLI espera receber pela entrada padrão.

use crate::application::{AppError, AppResult};
use std::{
    process::{ExitStatus, Output, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader},
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};

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

// ---- código de autenticação ----

/// Onde o login foi aberto, para o código colado chegar ao login certo: o de uma
/// conta e o de um conector podem estar abertos ao mesmo tempo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeScope {
    Account,
    Connector,
}

/// Login que espera o código: identificação da vaga e por onde o código chega.
type WaitingLogin = (u64, UnboundedSender<String>);

/// Login em andamento de cada lugar, que aceita o código colado pelo usuário.
static LOGIN_INPUT: Mutex<[Option<WaitingLogin>; 2]> = Mutex::new([None, None]);
static NEXT_LOGIN: AtomicU64 = AtomicU64::new(1);

/// Vaga do login para receber o código; ao terminar, o login deixa de aceitá-lo.
struct CodeSlot {
    scope: CodeScope,
    id: u64,
}

impl CodeSlot {
    /// Abre a vaga do lugar; se outro login dali ainda espera o código, ele a mantém
    /// e este segue sem receber código (só pelo retorno do navegador).
    fn open(scope: CodeScope) -> Option<(CodeSlot, UnboundedReceiver<String>)> {
        let mut slots = LOGIN_INPUT.lock().unwrap_or_else(|e| e.into_inner());
        let slot = &mut slots[scope as usize];
        if slot.as_ref().is_some_and(|(_, tx)| !tx.is_closed()) {
            return None;
        }
        let id = NEXT_LOGIN.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = unbounded_channel();
        *slot = Some((id, tx));
        Some((CodeSlot { scope, id }, rx))
    }
}

impl Drop for CodeSlot {
    fn drop(&mut self) {
        let mut slots = LOGIN_INPUT.lock().unwrap_or_else(|e| e.into_inner());
        let slot = &mut slots[self.scope as usize];
        if slot.as_ref().is_some_and(|(id, _)| *id == self.id) {
            *slot = None;
        }
    }
}

/// Código como o CLI espera: sem espaços nem quebras de linha que vieram ao copiar.
fn clean_code(code: &str) -> String {
    code.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Linha escrita na entrada do CLI para cada código (a mesma no login assíncrono e no bloqueante).
fn code_line(code: &str) -> String {
    format!("{code}\n")
}

/// Entrega ao login em andamento no lugar indicado o código de autenticação mostrado no navegador.
pub(crate) fn submit_login_code(scope: CodeScope, code: &str) -> AppResult<()> {
    let code = clean_code(code);
    if code.is_empty() {
        return Err(AppError("Cole o código de autenticação mostrado no navegador.".into()));
    }
    let slots = LOGIN_INPUT.lock().unwrap_or_else(|e| e.into_inner());
    match &slots[scope as usize] {
        Some((_, tx)) if tx.send(code).is_ok() => Ok(()),
        _ => Err(AppError("Nenhum login está esperando um código. Clique em Entrar de novo.".into())),
    }
}

// ---- versão assíncrona (tokio) ----

/// Tarefa que é cancelada se quem a criou desistir (ex.: tempo esgotado).
struct Task(tokio::task::JoinHandle<()>);

impl Drop for Task {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn spawn_reader<R: AsyncRead + Unpin + Send + 'static>(pipe: Option<R>, sink: Captured, page: Arc<LoginPage>) -> Task {
    Task(tokio::spawn(async move {
        let Some(pipe) = pipe else { return };
        let mut reader = BufReader::new(pipe);
        let mut line = Vec::new();
        while reader.read_until(b'\n', &mut line).await.is_ok_and(|n| n > 0) {
            sink.push(&line, &page);
            line.clear();
        }
    }))
}

/// Repassa ao CLI, pela entrada padrão, os códigos que o usuário colar.
fn spawn_code_writer(stdin: Option<tokio::process::ChildStdin>, mut codes: UnboundedReceiver<String>) -> Task {
    Task(tokio::spawn(async move {
        let Some(mut stdin) = stdin else { return };
        while let Some(code) = codes.recv().await {
            if stdin.write_all(code_line(&code).as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                break;
            }
        }
    }))
}

/// Executa o login de um CLI (que espera o navegador concluir) e devolve a saída.
/// Com `open_page`, abre a página de login no navegador padrão (ver `LoginPage`).
/// Com `code`, o código de autenticação colado naquele lugar chega por `submit_login_code`;
/// sem ele, a entrada fica vazia, como num CLI que nunca pede código.
pub(crate) async fn run_login(mut cmd: tokio::process::Command, open_page: bool, code: Option<CodeScope>) -> std::io::Result<Output> {
    let slot = code.and_then(CodeSlot::open);
    let stdin = if slot.is_some() { Stdio::piped() } else { Stdio::null() };
    let mut child = cmd.stdin(stdin).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let (_slot, _writer) = match slot {
        Some((slot, codes)) => (Some(slot), Some(spawn_code_writer(child.stdin.take(), codes))),
        None => (None, None),
    };
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
    /// Em login, a vaga que recebe o código colado; sai junto com o processo.
    _slot: Option<CodeSlot>,
}

impl BlockingRun {
    /// Começa a ler as saídas do processo. Em `login` (de conector), abre a página de login
    /// no navegador e repassa à entrada padrão (que deve estar aberta) o código colado.
    pub fn watch(child: &mut std::process::Child, login: bool) -> Self {
        let page = Arc::new(LoginPage::new(login));
        let outputs = Outputs { stdout: Captured::default(), stderr: Captured::default() };
        let readers = [
            read_lines_blocking(child.stdout.take(), outputs.stdout.clone(), page.clone()),
            read_lines_blocking(child.stderr.take(), outputs.stderr.clone(), page),
        ];
        // Sem vaga (outro login de conector ainda espera o código), a entrada fecha: EOF.
        let slot = if login { CodeSlot::open(CodeScope::Connector) } else { None };
        let stdin = child.stdin.take();
        let slot = slot.map(|(slot, mut codes)| {
            if let Some(mut stdin) = stdin {
                // A thread termina quando a vaga fecha (fim do processo) ou a entrada falha.
                std::thread::spawn(move || {
                    use std::io::Write;
                    while let Some(code) = codes.blocking_recv() {
                        if stdin.write_all(code_line(&code).as_bytes()).and_then(|_| stdin.flush()).is_err() {
                            break;
                        }
                    }
                });
            }
            slot
        });
        BlockingRun { outputs, readers: readers.into_iter().flatten().collect(), _slot: slot }
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
    use super::{clean_code, login_url, submit_login_code, CodeScope, CodeSlot};

    /// Os testes do código de autenticação usam a mesma vaga global: um de cada vez.
    static CODE_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// O CLI pede o código sem quebra de linha e espera pela entrada padrão.
    #[cfg(unix)]
    #[tokio::test]
    async fn login_waiting_for_a_code_receives_the_pasted_one() {
        let _turn = CODE_TESTS.lock().await;
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "printf 'Paste code here if prompted > '; read code; echo \"got:$code\""]);
        let login = tokio::spawn(super::run_login(cmd, false, Some(CodeScope::Account)));
        // Espera o login abrir a vaga do código.
        let mut tries = 0;
        while submit_login_code(CodeScope::Account, "abc#123").is_err() {
            tries += 1;
            assert!(tries < 200, "o login não passou a aceitar o código");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let output = login.await.unwrap().unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).ends_with("got:abc#123\n"));
        assert!(submit_login_code(CodeScope::Account, "abc").is_err(), "o login terminou");
    }

    /// Sem lugar para o código (ex.: Codex), o CLI recebe a entrada vazia e não ocupa vaga.
    #[cfg(unix)]
    #[tokio::test]
    async fn login_without_code_gets_empty_input() {
        let _turn = CODE_TESTS.lock().await;
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "read code; echo \"got:$code\""]);
        let output = super::run_login(cmd, false, None).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "got:\n");
    }

    #[test]
    fn pasted_code_reaches_only_the_login_in_progress() {
        let _turn = CODE_TESTS.blocking_lock();
        assert!(submit_login_code(CodeScope::Account, "abc").is_err(), "sem login em andamento");
        let (slot, mut codes) = CodeSlot::open(CodeScope::Account).unwrap();
        assert!(submit_login_code(CodeScope::Account, "  \n ").is_err(), "código vazio");
        submit_login_code(CodeScope::Account, " IAmq#BQ2p\nvVjC ").unwrap();
        assert_eq!(codes.try_recv().unwrap(), "IAmq#BQ2pvVjC");
        drop(slot);
        assert!(submit_login_code(CodeScope::Account, "abc").is_err(), "o login terminou");
        assert_eq!(clean_code("a b\tc"), "abc");
    }

    #[test]
    fn account_and_connector_logins_keep_their_own_codes() {
        let _turn = CODE_TESTS.blocking_lock();
        let (_account, mut account_codes) = CodeSlot::open(CodeScope::Account).unwrap();
        let (_connector, mut connector_codes) = CodeSlot::open(CodeScope::Connector).unwrap();
        // Um segundo login no mesmo lugar não toma a vaga de quem ainda espera.
        assert!(CodeSlot::open(CodeScope::Account).is_none());
        submit_login_code(CodeScope::Connector, "c1").unwrap();
        submit_login_code(CodeScope::Account, "a1").unwrap();
        assert_eq!(account_codes.try_recv().unwrap(), "a1");
        assert_eq!(connector_codes.try_recv().unwrap(), "c1");
        assert!(account_codes.try_recv().is_err());
    }

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
