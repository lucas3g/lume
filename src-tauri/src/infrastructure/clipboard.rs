//! Área de transferência do sistema.

use crate::application::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::borrow::Cow;

/// Põe na área de transferência uma imagem em base64 (PNG, JPEG, WebP ou GIF).
pub fn copy_image(data: &str) -> AppResult<()> {
    let bytes = STANDARD.decode(data.trim()).map_err(|_| AppError("A imagem está corrompida.".into()))?;
    let rgba = image::load_from_memory(&bytes)
        .map_err(|_| AppError("Formato de imagem não suportado.".into()))?
        .into_rgba8();
    let (width, height) = rgba.dimensions();
    let image = arboard::ImageData { width: width as usize, height: height as usize, bytes: Cow::Owned(rgba.into_raw()) };
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_image(image))
        .map_err(|e| AppError(format!("Não foi possível copiar a imagem: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejeita_base64_invalido() {
        assert!(copy_image("não é base64").is_err());
    }

    #[test]
    fn rejeita_bytes_que_nao_sao_imagem() {
        assert!(copy_image(&STANDARD.encode(b"texto")).is_err());
    }
}
