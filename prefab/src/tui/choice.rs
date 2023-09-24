use std::io::Stdout;
use std::time::Duration;
use anyhow::anyhow;
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use ratatui::backend::CrosstermBackend;
use ratatui::prelude::{Color, Constraint, Direction, Layout, Modifier, Span, Style, Stylize};
use ratatui::Terminal;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use prefab::template::TemplateOption;
use crate::tui::option_ui::{EditorStatus, OptionUi};

pub struct ChoiceUI {
    option: TemplateOption,
    index: usize,
    status: EditorStatus,
    state: ListState,
    item_count: usize,
}

impl ChoiceUI {
    pub fn new(option: TemplateOption) -> ChoiceUI {

        let count = if let TemplateOption::Choice { prompt, options, value, mandatory } = &option {
            options.len()
        }else{
            0
        };

        ChoiceUI {
            option,
            index: 0,
            status: EditorStatus::Continue,
            state: ListState::default(),
            item_count: count,
        }
    }
}

impl OptionUi for ChoiceUI {
    fn render_list_item(&self) -> anyhow::Result<ListItem> {
        if let TemplateOption::Choice { prompt, options, value, mandatory } = &self.option {
            let result = ListItem::new(if let Some(value) = value {
                Line::from(vec![
                    Span::raw(prompt).green(),
                    Span::raw(" => ").yellow(),
                    Span::raw(value).white(),

                ])
            }else {
                Line::from(vec![
                    Span::raw(prompt).green(),
                    Span::raw(" => ").yellow(),
                    Span::raw("Empty").dark_gray(),
                ])
            });

            anyhow::Ok(result)
        }else{
            Err(anyhow!("Option is not a choice field!"))
        }
    }

    fn render_edit(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
        if let TemplateOption::Choice { prompt, options, value, mandatory } = &self.option {
            let val = value.clone().unwrap_or("".to_string());
            let mut items: Vec<ListItem> = options.iter().map(|op| {
                ListItem::new(Line::from(vec![Span::raw(op)]))
            }).collect();

            let mut state = ListState::default();
            state.select(Some(self.index));

            items.push(ListItem::new(Line::from(vec![Span::raw("--Empty--")])));

            terminal.draw(|frame| {
                let list = List::new(items.clone())
                    .block(Block::default().title("Options").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White))
                    .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
                    .highlight_symbol(">>");

                let text = vec![
                    Line::from(vec![Span::raw(prompt).green(), Span::raw(":").yellow()]),
                    Line::from(vec![Span::raw(val.clone())])
                ];

                let paragraph = Paragraph::new(text)
                    .block(Block::default().title("[Edit]-(Enter: Confirm, Esc - Cancel) ").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White));

                let rects = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(
                        [
                            Constraint::Length(3),
                            Constraint::Percentage(100)
                        ]
                            .as_ref(),
                    )
                    .split(frame.size());

                //let greeting = Paragraph::new("Hello World!");
                frame.render_widget(paragraph, rects[0]);
                frame.render_stateful_widget(list, rects[1], &mut state)
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
                        self.index = self.item_count;
                    } else {
                        self.index -= 1;
                    }

                    self.state.select(Some(self.index));
                }

                if KeyCode::Down == key.code {
                    if self.index == self.item_count {
                        self.index = 0;
                    } else {
                        self.index += 1;
                    }

                    self.state.select(Some(self.index));
                }

                if KeyCode::Enter == key.code {
                    if self.index == self.item_count {
                        self.option = self.option.clone().set_value("".to_string());
                    } else if let TemplateOption::Choice { prompt:_, options, value:_, mandatory } = &self.option {
                        self.option = self.option.clone().set_value(options[self.index].clone());
                    }

                    self.status = EditorStatus::Finished { option: self.option.clone()};

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
        self.state.select(Some(self.index));
    }

    fn get_option(&self) -> TemplateOption {
        self.option.clone()
    }
}