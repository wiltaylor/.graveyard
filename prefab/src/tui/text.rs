use std::time::Duration;
use anyhow::Ok;
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use prefab::template::TemplateOption;
use ratatui::{
    style::{Stylize, Style, Color},
    text::{Line, Span},
    widgets::{Paragraph, Block, Borders},
};
use regex::Regex;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;
use crate::tui::option_ui::EditorStatus;

use super::option_ui::OptionUi;

pub struct TextUI {
    option: TemplateOption,
    input: Input,
    status: EditorStatus,
    name: String,
}

impl TextUI {
    pub fn new(option: TemplateOption, name: String) -> TextUI {
        let input = if let Some(o) = option.get_value() {
            Input::new(o)
        }else{
            Input::default()
        };

        TextUI { option, input, status: EditorStatus::Continue, name }
    }
}

impl OptionUi for TextUI {
    fn render_edit(
        &mut self,
        terminal: &mut ratatui::Terminal<ratatui::prelude::CrosstermBackend<std::io::Stdout>>,
    ) -> anyhow::Result<()> {
        let prompt = self.option.get_prompt();
        let val = self.input.value().to_string();
        let name = self.get_name();
        let mandatory = self.option.is_mandatory();

        let input_text = if self.is_valid() {
            Line::from(vec![Span::raw(val.clone())])
        }else{
            Line::from(vec![Span::raw(val.clone()).red()])
        };

        let text = vec![
            Line::from(vec![Span::raw(prompt).green(), Span::raw(":").yellow()]),
            input_text
        ];

        terminal.draw(|frame| {
            let paragraph = Paragraph::new(text)
                .block(Block::default().title(Line::from(vec![
                    Span::raw("[").gray(),
                    Span::raw("Edit:"),
                    if mandatory {
                        Span::raw(name).yellow()
                    }else{
                        Span::raw(name).blue()
                    },
                    Span::raw("]").gray(),
                    Span::raw("──"),
                    Span::raw("[").gray(),
                    Span::raw("Enter").blue(),
                    Span::raw("-Save "),
                    Span::raw("Esc").blue(),
                    Span::raw("-Cancel"),
                    Span::raw("]").gray(),
                ])).borders(Borders::ALL))
                .style(Style::default().fg(Color::White));

            frame.set_cursor((val.len() as u16) + 1, 2);
            frame.render_widget(paragraph, frame.size())

        })?;

        Ok(())
    }

    fn update_input(&mut self) -> anyhow::Result<()> {
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if KeyCode::Esc == key.code {
                    self.status = EditorStatus::Cancel;
                    return Ok(())
                }else if KeyCode::Enter == key.code && self.is_valid() {
                    let v = self.input.value().clone().to_string();
                    let o = self.option.clone().set_value(v);
                    self.option = o.clone();
                    self.status = EditorStatus::Finished { option: o};
                    return Ok(());
                }else{
                    self.input.handle_event(&Event::Key(key));
                }
            }
        }

        Ok(())
    }

    fn get_status(&self) -> anyhow::Result<EditorStatus> {
        Ok(self.status.clone())
    }

    fn start_edit(&mut self) {
        self.status = EditorStatus::Continue;

        if let Some(v) = self.option.get_value() {
            self.input = Input::new(v);
        }else{
            self.input = Input::default();
        }
    }

    fn get_option(&self) -> TemplateOption {
        self.option.clone()
    }

    fn is_valid(&self) -> bool {

        let value = self.input.value();

        if value.is_empty() {
            return true;
        }

        if let Some(p) = self.option.get_pattern() {
            if let Result::Ok(r) = Regex::new(p.clone().as_str()) {
                return r.is_match(value);
            }
        }

        true
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}
