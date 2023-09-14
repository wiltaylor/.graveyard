use thiserror::Error;

#[derive(Error, Debug)]
pub enum PrefabError {
    
    #[error("Can't find template at `{0}`!")]
    TemplateNotFound(String)
}