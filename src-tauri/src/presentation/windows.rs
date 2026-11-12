//! Janelas, ícone na barra de menus e atalho global da entrada rápida.

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub const MAIN_WINDOW: &str = "main";
pub const QUICK_WINDOW: &str = "quick";
pub const QUICK_SHORTCUT: &str = "CommandOrControl+Shift+Space";
/// Evento que pede à janela principal para abrir uma conversa.
pub const OPEN_CONVERSATION: &str = "open-conversation";

const QUICK_ROUTE: &str = "quick";
const QUICK_SIZE: (f64, f64) = (680.0, 132.0);
const MENU_OPEN: &str = "open";
const MENU_QUICK: &str = "quick";
const MENU_QUIT: &str = "quit";

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

pub fn hide_quick_entry(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(QUICK_WINDOW) {
        let _ = window.hide();
    }
}

/// Mostra a entrada rápida, ou a esconde se já estiver visível.
pub fn toggle_quick_entry(app: &AppHandle) {
    let window = match app.get_webview_window(QUICK_WINDOW) {
        Some(window) => window,
        None => {
            let built = WebviewWindowBuilder::new(app, QUICK_WINDOW, WebviewUrl::App(QUICK_ROUTE.into()))
                .title("Entrada rápida")
                .inner_size(QUICK_SIZE.0, QUICK_SIZE.1)
                .decorations(false)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .center()
                .build();
            match built {
                Ok(window) => window,
                Err(_) => return,
            }
        }
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// A entrada rápida enviou uma mensagem: traz a janela principal para a conversa.
pub fn open_conversation(app: &AppHandle, conversation_id: &str) {
    hide_quick_entry(app);
    show_main(app);
    let _ = app.emit(OPEN_CONVERSATION, conversation_id);
}

/// Fechar a janela principal só a esconde (o app segue na barra de menus);
/// a entrada rápida some ao perder o foco.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    match (window.label(), event) {
        (MAIN_WINDOW, WindowEvent::CloseRequested { api, .. }) => {
            api.prevent_close();
            let _ = window.hide();
        }
        (QUICK_WINDOW, WindowEvent::Focused(false)) => {
            let _ = window.hide();
        }
        _ => {}
    }
}

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, MENU_OPEN, "Abrir o Lume", true, None::<&str>)?,
            &MenuItem::with_id(app, MENU_QUICK, "Entrada rápida", true, Some(QUICK_SHORTCUT))?,
            &MenuItem::with_id(app, MENU_QUIT, "Sair", true, None::<&str>)?,
        ],
    )?;
    let mut tray = TrayIconBuilder::new().tooltip("Lume").menu(&menu).on_menu_event(|app, event| {
        match event.id().as_ref() {
            MENU_OPEN => show_main(app),
            MENU_QUICK => toggle_quick_entry(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        }
    });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Registra o atalho global; se outro aplicativo já o usa, o app segue sem ele.
pub fn setup_shortcut(app: &AppHandle) {
    let _ = app.global_shortcut().on_shortcut(QUICK_SHORTCUT, |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            toggle_quick_entry(app);
        }
    });
}
