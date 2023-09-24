use std::io::Stdout;

use prefab::template::TemplateOption;
use ratatui::{widgets::ListItem, Terminal, prelude::CrosstermBackend};
use anyhow::Result;

#[derive(Debug, Clone)]
pub enum EditorStatus {
    Continue,
    Cancel,
    Finished{option: TemplateOption}
}

pub trait OptionUi {
    fn render_list_item(&self) -> Result<ListItem>;
    fn render_edit(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()>;
    fn update_input(&mut self) -> Result<()>;
    fn get_status(&self) -> Result<EditorStatus>;
    fn start_edit(&mut self);
    fn get_option(&self) -> TemplateOption;
}