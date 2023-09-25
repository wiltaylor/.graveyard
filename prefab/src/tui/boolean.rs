use std::io::Stdout;
use std::time::Duration;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use prefab::template::TemplateOption;
use crate::tui::option_ui::{EditorStatus, OptionUi};
use crossterm::event;
use crossterm::event::{Event, KeyCode};

use ratatui::{
    style::{Stylize, Style, Color},
    text::{Line, Span},
    widgets::{ListItem, Paragraph, Block, Borders},
};
use ratatui::layout::{Direction, Layout};
use ratatui::prelude::Constraint;
use ratatui::style::Modifier;
use ratatui::widgets::{List, ListState};


pub struct BooleanUI {
    option: TemplateOption,
    status: EditorStatus,
    state: ListState,
    index: usize,
    name: String,
}

impl BooleanUI {
    pub fn new(option: TemplateOption, name: String) -> BooleanUI {
        BooleanUI { option, status: EditorStatus::Continue, state: ListState::default(), index: 0, name }
    }
}

impl OptionUi for BooleanUI {
    fn render_edit(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
        let prompt = self.option.get_prompt();
        let value = self.option.get_value();

        let mut items: Vec<ListItem> = vec![
            ListItem::new(Line::from("True")),
            ListItem::new(Line::from("False")),
        ];

        if !self.option.is_mandatory() {
            items.push(ListItem::new(Line::from("Empty")));
        }

        let val = if let Some(v) = value {
            Span::from(v).white()
        } else {
            Span::from("Empty".to_string()).gray()
        };

        terminal.draw(|frame| {
            let list = List::new(items)
                .block(Block::default().title("Edit").borders(Borders::ALL))
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
                .highlight_symbol(">>");

            let text: Vec<Line> = vec![
                Line::from(vec![Span::raw(prompt).green(), Span::raw(": ").yellow(), val])
            ];

            let paragraph = Paragraph::new(text)
                .block(Block::default().title("Edit").borders(Borders::ALL))
                .style(Style::default().fg(Color::White));

            let rects = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Percentage(100)
                ].as_ref())
                .split(frame.size());

            frame.render_widget(paragraph, rects[0]);
            frame.render_stateful_widget(list, rects[1], &mut self.state)
        })?;

        Ok(())
    }

    fn update_input(&mut self) -> anyhow::Result<()> {
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if KeyCode::Esc == key.code {
                    self.status = EditorStatus::Cancel;
                    return Ok(());
                }

                if KeyCode::Up == key.code {
                    if self.index == 0 {
                        self.index = 2;
                    } else {
                        self.index -= 1;
                    }

                    self.state.select(Some(self.index));
                }

                if KeyCode::Down == key.code {
                    if self.index == 2 {
                        self.index = 0;
                    } else {
                        self.index += 1;
                    }

                    self.state.select(Some(self.index));
                }

                if KeyCode::Enter == key.code {
                    match self.index {
                        0 => self.option = self.option.clone().set_value("true".to_string()),
                        1 => self.option = self.option.clone().set_value("false".to_string()),
                        2 => self.option = self.option.clone().set_value("".to_string()),
                        _ => panic!("Some how broke out of menu bounds!")
                    }

                    self.status = EditorStatus::Finished { option: self.option.clone()};
                }
            }
        }

        Ok(())
    }

    fn get_status(&self) -> anyhow::Result<EditorStatus> { Ok(self.status.clone()) }
    fn start_edit(&mut self) {
        self.status = EditorStatus::Continue;
        self.index = 0;
        self.state.select(Some(0));
    }
    fn get_option(&self) -> TemplateOption {
        self.option.clone()
    }

    fn is_valid(&self) -> bool {
        true
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}