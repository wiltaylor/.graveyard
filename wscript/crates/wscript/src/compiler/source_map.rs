use super::token::Span;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    pub entries: Vec<SourceMapEntry>,
}

#[derive(Debug, Clone)]
pub struct SourceMapEntry {
    pub wasm_offset: u32,
    pub span: Span,
    pub fn_name: Option<String>,
    pub local_names: HashMap<u32, String>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_entry(&mut self, entry: SourceMapEntry) -> u32 {
        let id = self.entries.len() as u32;
        self.entries.push(entry);
        id
    }

    pub fn lookup_by_wasm_offset(&self, offset: u32) -> Option<&SourceMapEntry> {
        self.entries.iter().find(|e| e.wasm_offset == offset)
    }

    pub fn lookup_by_source_line(&self, line: u32) -> Vec<&SourceMapEntry> {
        self.entries
            .iter()
            .filter(|e| e.span.line == line)
            .collect()
    }
}
