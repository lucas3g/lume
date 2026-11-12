//! Leitura de arquivos escolhidos pelo usuário para anexar a mensagens e projetos.

use crate::application::{
    ports::{FileLoader, FileSaver},
    AppError, AppResult,
};
use crate::domain::chat::{Attachment, AttachmentKind};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{fs, path::Path};

const MAX_TEXT_BYTES: u64 = 500 * 1024;
const MAX_BINARY_BYTES: u64 = 10 * 1024 * 1024;

pub struct FsFileLoader;

/// Tipo do anexo pela extensão; o que não é imagem nem PDF é tratado como texto.
fn kind_of(path: &Path) -> (AttachmentKind, &'static str) {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match extension.as_str() {
        "png" => (AttachmentKind::Image, "image/png"),
        "jpg" | "jpeg" => (AttachmentKind::Image, "image/jpeg"),
        "gif" => (AttachmentKind::Image, "image/gif"),
        "webp" => (AttachmentKind::Image, "image/webp"),
        "pdf" => (AttachmentKind::Pdf, "application/pdf"),
        _ => (AttachmentKind::Text, "text/plain"),
    }
}

impl FileLoader for FsFileLoader {
    fn load(&self, path: &str) -> AppResult<Attachment> {
        let path = Path::new(path);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let fail = |reason: &str| AppError(format!("{name}: {reason}"));
        let size = fs::metadata(path).map_err(|_| fail("não foi possível abrir o arquivo."))?.len();
        let (kind, media_type) = kind_of(path);
        let limit = if kind == AttachmentKind::Text { MAX_TEXT_BYTES } else { MAX_BINARY_BYTES };
        if size > limit {
            return Err(fail(&format!("é maior que o limite de {} KB.", limit / 1024)));
        }
        let bytes = fs::read(path).map_err(|_| fail("não foi possível ler o arquivo."))?;
        let data = match kind {
            AttachmentKind::Text => String::from_utf8(bytes).map_err(|_| fail("este tipo de arquivo não é suportado."))?,
            AttachmentKind::Image | AttachmentKind::Pdf => STANDARD.encode(bytes),
        };
        Ok(Attachment { name, kind, media_type: media_type.into(), data })
    }
}

impl FileSaver for FsFileLoader {
    fn save(&self, path: &str, content: &str) -> AppResult<()> {
        fs::write(path, content).map_err(|e| AppError(format!("Não foi possível salvar o arquivo: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_text_and_images_and_rejects_binary_and_missing_files() {
        let dir = std::env::temp_dir().join(format!("relay-files-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        fs::write(dir.join("nota.md"), "olá").unwrap();
        fs::write(dir.join("foto.PNG"), [0x89, b'P', b'N', b'G']).unwrap();
        fs::write(dir.join("dados.bin"), [0xff, 0xfe, 0x00]).unwrap();

        let text = FsFileLoader.load(&path("nota.md")).unwrap();
        assert_eq!((text.name.as_str(), text.kind, text.data.as_str()), ("nota.md", AttachmentKind::Text, "olá"));
        let image = FsFileLoader.load(&path("foto.PNG")).unwrap();
        assert_eq!((image.kind, image.media_type.as_str(), image.data.as_str()), (AttachmentKind::Image, "image/png", "iVBORw=="));
        assert!(FsFileLoader.load(&path("dados.bin")).unwrap_err().0.contains("não é suportado"));
        assert!(FsFileLoader.load(&path("nao-existe.txt")).is_err());

        FsFileLoader.save(&path("saida.html"), "<p>oi</p>").unwrap();
        assert_eq!(fs::read_to_string(dir.join("saida.html")).unwrap(), "<p>oi</p>");
        assert!(FsFileLoader.save(&path("pasta-inexistente/x.html"), "").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
