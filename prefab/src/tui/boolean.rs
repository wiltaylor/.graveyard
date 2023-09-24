use std::io::Stdout;
use std::time::Duration;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use prefab::template::TemplateOption;
use crate::tui::option_ui::{EditorStatus, OptionUi};
use anyhow::anyhow;
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
    input: Option<bool>,
    status: EditorStatus,
    state: ListState,
    index: usize,
}

impl BooleanUI {
    pub fn new(option: TemplateOption) -> BooleanUI {
        let input = if let TemplateOption::Boolean { prompt: _, value } = &option {
            value.clone()
        } else {
            None
        };

        BooleanUI { option, input, status: EditorStatus::Continue, state: ListState::default(), index: 0 }
    }
}

impl OptionUi for BooleanUI {
    fn render_list_item(&self) -> anyhow::Result<ListItem> {
        if let TemplateOption::Boolean { prompt, value } = &self.option {
            let result = ListItem::new(if let Some(value) = value {
                Line::from(vec![
                    Span::raw(prompt).green(),
                    Span::raw(" => ").yellow(),
                    Span::raw(if *value { "True" } else { "False" }).white(),
                ])
            } else {
                Line::from(vec![
                    Span::raw(prompt).green(),
                    Span::raw(" => ").yellow(),
                    Span::raw("Empty").dark_gray(),
                ])
            });

            anyhow::Ok(result)
        } else {
            Err(anyhow!("Option is not a boolean field!"))
        }
    }

    fn render_edit(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
        if let TemplateOption::Boolean { prompt, value } = &self.option {
            let items: Vec<ListItem> = vec![
                ListItem::new(Line::from("True")),
                ListItem::new(Line::from("False")),
                ListItem::new(Line::from("Empty")),
            ];

            let val = if let Some(v) = value {
                Span::from(format!("{}", v)).white()
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
        }

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
        self.state.select(Some(self.index));
    }
    fn get_option(&self) -> TemplateOption {
        self.option.clone()
    }
}