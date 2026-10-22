//! fastembed-backed embedding pool. One [`TextEmbedding`] per resolved
//! model id, lazy-loaded on first use and shared across banks that
//! reference the same model.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

use super::MemoryError;

/// Friendly default when a memory bank does not declare `embed_model`.
/// Chosen for size (~30MB) and quality trade-off on English text.
pub const DEFAULT_EMBED_MODEL: &str = "BGE-small-en-v1.5";

/// Lazy cache keyed by fastembed model id. Each entry holds a
/// `TextEmbedding` behind its own mutex so concurrent banks sharing the
/// same model serialize only with one another, not with other models.
pub struct EmbedderPool {
    cache_dir: PathBuf,
    models: Mutex<HashMap<String, Arc<Mutex<TextEmbedding>>>>,
}

impl EmbedderPool {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            models: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve the effective model id for a bank, substituting the
    /// default when the bank left `embed_model` unset.
    pub fn resolve_model_id(override_id: Option<&str>) -> &str {
        override_id.unwrap_or(DEFAULT_EMBED_MODEL)
    }

    /// Embed `text` using `model_id` (or the default when `None`).
    /// Downloads the model on first use, then caches it for the lifetime
    /// of the pool.
    pub fn embed(&self, model_id: Option<&str>, text: &str) -> Result<Vec<f32>, MemoryError> {
        let model = Self::resolve_model_id(model_id).to_string();
        let handle = self.get_or_load(&model)?;
        let mut guard = handle.lock().expect("embedder mutex poisoned");
        let mut out = guard
            .embed(vec![text], None)
            .map_err(|e| MemoryError::Embed(e.to_string()))?;
        out.pop()
            .ok_or_else(|| MemoryError::Embed("embed returned no vector".into()))
    }

    /// Embedding dimension for the resolved model. Reads from the
    /// fastembed model registry; does not require the model to be
    /// downloaded.
    pub fn dim(&self, model_id: Option<&str>) -> Result<usize, MemoryError> {
        let model = Self::resolve_model_id(model_id);
        let kind = parse_model_id(model)?;
        let infos = TextEmbedding::list_supported_models();
        infos
            .into_iter()
            .find(|info| info.model == kind)
            .map(|info| info.dim)
            .ok_or_else(|| MemoryError::UnknownModel(model.to_string()))
    }

    fn get_or_load(&self, model: &str) -> Result<Arc<Mutex<TextEmbedding>>, MemoryError> {
        {
            let guard = self.models.lock().expect("embedder map poisoned");
            if let Some(handle) = guard.get(model) {
                return Ok(Arc::clone(handle));
            }
        }

        let kind = parse_model_id(model)?;
        let opts = TextInitOptions::new(kind.clone())
            .with_cache_dir(self.cache_dir.clone())
            .with_show_download_progress(false);
        let text_embed = TextEmbedding::try_new(opts)
            .map_err(|e| MemoryError::Embed(format!("loading model `{model}`: {e}")))?;
        let handle = Arc::new(Mutex::new(text_embed));

        let mut guard = self.models.lock().expect("embedder map poisoned");
        // Double-check: another thread may have loaded the same model
        // between our get and our insert. Keep the existing entry so the
        // two callers share a single embedder.
        Ok(Arc::clone(
            guard
                .entry(model.to_string())
                .or_insert_with(|| handle.clone()),
        ))
    }
}

/// Map the human-friendly id in a `memory_bank { embed_model = "..." }`
/// block to a fastembed `EmbeddingModel` variant. Accepts either the
/// HuggingFace-style label (e.g. `"BGE-small-en-v1.5"`) or a terse
/// alias. Unknown ids surface as `MemoryError::UnknownModel`.
fn parse_model_id(id: &str) -> Result<EmbeddingModel, MemoryError> {
    let lower = id.to_ascii_lowercase();
    let trimmed = lower.trim();
    let model = match trimmed {
        "bge-small-en-v1.5" | "bgesmallenv15" => EmbeddingModel::BGESmallENV15,
        "bge-small-en-v1.5-q" | "bgesmallenv15q" => EmbeddingModel::BGESmallENV15Q,
        "bge-base-en-v1.5" | "bgebaseenv15" => EmbeddingModel::BGEBaseENV15,
        "bge-base-en-v1.5-q" | "bgebaseenv15q" => EmbeddingModel::BGEBaseENV15Q,
        "bge-large-en-v1.5" | "bgelargeenv15" => EmbeddingModel::BGELargeENV15,
        "all-minilm-l6-v2" | "allminilml6v2" => EmbeddingModel::AllMiniLML6V2,
        "all-minilm-l12-v2" | "allminilml12v2" => EmbeddingModel::AllMiniLML12V2,
        "multilingual-e5-small" | "multilinguale5small" => EmbeddingModel::MultilingualE5Small,
        "multilingual-e5-base" | "multilinguale5base" => EmbeddingModel::MultilingualE5Base,
        "nomic-embed-text-v1.5" | "nomicembedtextv15" => EmbeddingModel::NomicEmbedTextV15,
        _ => return Err(MemoryError::UnknownModel(id.to_string())),
    };
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_model_defaults_to_bge_small() {
        assert_eq!(EmbedderPool::resolve_model_id(None), DEFAULT_EMBED_MODEL);
        assert_eq!(
            EmbedderPool::resolve_model_id(Some("multilingual-e5-small")),
            "multilingual-e5-small"
        );
    }

    #[test]
    fn parse_known_model_aliases() {
        assert!(matches!(
            parse_model_id("BGE-small-en-v1.5").unwrap(),
            EmbeddingModel::BGESmallENV15
        ));
        assert!(matches!(
            parse_model_id("bge-base-en-v1.5").unwrap(),
            EmbeddingModel::BGEBaseENV15
        ));
    }

    #[test]
    fn parse_unknown_model_errors() {
        assert!(matches!(
            parse_model_id("gpt-99"),
            Err(MemoryError::UnknownModel(_))
        ));
    }

    #[test]
    fn dim_for_default_without_loading() {
        let pool = EmbedderPool::new(PathBuf::from(".aciddog/memory/_models"));
        // BGE-small-en-v1.5 is 384-dim; `dim` reads the registry, not
        // the model file, so this works without any download.
        assert_eq!(pool.dim(None).unwrap(), 384);
    }
}
