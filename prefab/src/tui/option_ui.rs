use std::io::Stdout;

use prefab::template::TemplateOption;
use ratatui::{widgets::ListItem, Terminal, prelude::CrosstermBackend};
use anyhow::Result;
use ratatui::prelude::{Line, Span, Stylize};

#[derive(Debug, Clone)]
pub enum EditorStatus {
    Continue,
    Cancel,
    Finished{option: TemplateOption}
}

pub trait OptionUi {
    fn render_list_item(&self) -> Result<ListItem> {
        let prompt = self.get_option().get_prompt();
        let value = self.get_option().get_value();

        Ok(ListItem::new(
            Line::from(vec![
                Span::raw(prompt).green(),
                Span::raw(" => ").yellow(),

                if let Some(val) = value {
                    Span::raw(val).white()
                }else{
                    Span::raw("Empty").gray()

                }
            ])))
    }

    fn render_edit(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()>;
    fn update_input(&mut self) -> Result<()>;
    fn get_status(&self) -> Result<EditorStatus>;
    fn start_edit(&mut self);
    fn get_option(&self) -> TemplateOption;
}