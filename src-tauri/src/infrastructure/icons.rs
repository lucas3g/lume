//! Ícones dos conectores: o favicon do serviço, buscado uma vez e guardado em disco.

use crate::application::{ports::IconSource, AppResult};
use std::{fs, path::PathBuf, time::Duration};

/// Serviço público que devolve o favicon de um domínio em tamanho fixo.
const FAVICON_SERVICE: &str = "https://icons.duckduckgo.com/ip3/";
const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BYTES: usize = 512 * 1024;
/// Marca de que já se tentou e o domínio não tem ícone, para não repetir a busca.
const MISSING_MARK: &str = "missing";

pub struct FaviconFetcher {
    cache_dir: PathBuf,
}

impl FaviconFetcher {
    pub fn new(cache_dir: PathBuf) -> Self {
        FaviconFetcher { cache_dir }
    }

    fn cache_path(&self, domain: &str) -> PathBuf {
        let safe: String = domain.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
        self.cache_dir.join(format!("{safe}.ico"))
    }

    fn fetch(&self, domain: &str) -> Option<Vec<u8>> {
        let mut response = ureq::get(format!("{FAVICON_SERVICE}{domain}.ico"))
            .config()
            .timeout_global(Some(TIMEOUT))
            .build()
            .call()
            .ok()?;
        let bytes = response.body_mut().with_config().limit(MAX_BYTES as u64).read_to_vec().ok()?;
        (!bytes.is_empty()).then_some(bytes)
    }
}

impl IconSource for FaviconFetcher {
    fn icon(&self, domain: &str) -> AppResult<Option<Vec<u8>>> {
        let path = self.cache_path(domain);
        if let Ok(cached) = fs::read(&path) {
            return Ok((cached != MISSING_MARK.as_bytes()).then_some(cached));
        }
        let _ = fs::create_dir_all(&self.cache_dir);
        let fetched = self.fetch(domain);
        let _ = fs::write(&path, fetched.as_deref().unwrap_or(MISSING_MARK.as_bytes()));
        Ok(fetched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "acessa a internet"]
    fn fetches_and_caches_a_real_favicon() {
        let dir = std::env::temp_dir().join(format!("lume-icons-{}", uuid::Uuid::new_v4()));
        let fetcher = FaviconFetcher::new(dir.clone());
        let first = fetcher.icon("github.com").unwrap().expect("ícone do GitHub");
        assert!(fs::read_dir(&dir).unwrap().count() == 1, "guardado em disco");
        assert_eq!(fetcher.icon("github.com").unwrap().unwrap(), first, "segunda leitura vem do cache");
        fs::remove_dir_all(dir).unwrap();
    }
}
