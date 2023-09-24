use std::time::Duration;
use anyhow::Ok;
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use prefab::template::TemplateOption;
use ratatui::{
    style::{Stylize, Style, Color},
    text::{Line, Span},
    widgets::{ListItem, Paragraph, Block, Borders},
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
    match_pattern: bool
}

impl TextUI {
    pub fn new(option: TemplateOption) -> TextUI {
        let input = if let TemplateOption::FreeText { prompt:_, value } = &option {
            if let Some(val) = value { Input::new(val.clone()) } else { Input::default() }
        }else if let TemplateOption::Regex{ prompt:_, pattern:_, value} = &option {
            if let Some(val) = value {Input::new(val.clone())} else { Input::default() }
        }
        else {
            Input::default()
        };

        TextUI { option, input, status: EditorStatus::Continue, match_pattern: true }
    }

    fn get_option_parts(&self) -> (String, Option<String>, Option<String>){
        match &self.option {
            TemplateOption::FreeText { prompt, value } =>
                (prompt.clone(), None, value.clone()),
            TemplateOption::Regex { prompt, pattern, value } =>
                (prompt.clone(), Some(pattern.clone()), value.clone()),
            _ => panic!("Expected free text or regex!")
        }
    }
}

impl OptionUi for TextUI {
    fn render_list_item(&self) -> anyhow::Result<ListItem> {
        let (prompt, _, value) = self.get_option_parts();

        let val= if let Some(v) = value {
            Span::raw(v).white()
        }else {
            Span::raw("Empty").gray()
        };

        Ok(ListItem::new(Line::from(vec![
            Span::raw(prompt).green(),
            Span::raw(" => ").yellow(),
            val
        ])))
    }

    fn render_edit(
        &mut self,
        terminal: &mut ratatui::Terminal<ratatui::prelude::CrosstermBackend<std::io::Stdout>>,
    ) -> anyhow::Result<()> {
        let (prompt, pattern, value) = self.get_option_parts();
        let val = self.input.value().to_string();

        let input_text = if self.match_pattern {
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
                .block(Block::default().title("[Edit]-(Enter: Confirm, Esq - Cancel) ").borders(Borders::ALL))
                .style(Style::default().fg(Color::White));

            frame.set_cursor((val.len() as u16) + 1, 2);
            frame.render_widget(paragraph, frame.size())

        })?;

        Ok(())
    }

    fn update_input(&mut self) -> anyhow::Result<()> {
        let (_, pattern,  _) = self.get_option_parts();

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if KeyCode::Esc == key.code {
                    self.status = EditorStatus::Cancel;
                    return Ok(())
                }else if KeyCode::Enter == key.code && self.match_pattern {
                    let v = self.input.value().clone().to_string();
                    let o = self.option.clone().set_value(v);
                    self.option = o.clone();
                    self.status = EditorStatus::Finished { option: o};
                    return Ok(());
                }else{
                    self.input.handle_event(&Event::Key(key));
                    let v = self.input.value().clone().to_string();

                    if let Some(p) = pattern {
                        if let Result::Ok(r) = Regex::new(p.clone().as_str()) {
                            self.match_pattern = r.is_match(v.as_str());
                        }
                    }
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
}
