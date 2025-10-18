use std::io::Stdout;
use std::time::Duration;
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
    name: String,
}

impl ChoiceUI {
    pub fn new(option: TemplateOption, name: String) -> ChoiceUI {

        let count = option.get_choice_options().unwrap_or(vec![]).len();

        ChoiceUI {
            option,
            index: 0,
            status: EditorStatus::Continue,
            state: ListState::default(),
            item_count: count,
            name
        }
    }
}

impl OptionUi for ChoiceUI {
    fn render_edit(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {
        let prompt = self.option.get_prompt();
        let value = self.option.get_value();
        let options = self.option.get_choice_options().expect("You need to have specified options for a choice type!");
        let name = self.get_name();
        let mandatory = self.option.is_mandatory();

        let mut state = ListState::default();
        state.select(Some(self.index));

        let mut items : Vec<ListItem> = options.iter().map(|op| {
           ListItem::new(Line::from(vec![Span::raw(op.clone())]))
        }).collect();

        items.push(ListItem::new(Line::from(vec![Span::raw("--Empty--")])));

        let val = value.clone().unwrap_or("".to_string());

        terminal.draw(|frame| {
            let list = List::new(items.clone())
                .block(Block::default().borders(Borders::ALL))
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
                .highlight_symbol(">>");

            let text = vec![
                Line::from(vec![Span::raw(prompt).green(), Span::raw(":").yellow()]),
                Line::from(vec![Span::raw(val.clone())])
            ];

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
                    Span::raw("↑↓").blue(),
                    Span::raw("-Select Items "),
                    Span::raw("Enter").blue(),
                    Span::raw("-Save "),
                    Span::raw("Esc").blue(),
                    Span::raw("-Cancel"),
                    Span::raw("]").gray(),
                ])).borders(Borders::ALL))                .style(Style::default().fg(Color::White));

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

            frame.render_widget(paragraph, rects[0]);
            frame.render_stateful_widget(list, rects[1], &mut state)
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
                    } else if let TemplateOption::Choice { options, .. } = &self.option {
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