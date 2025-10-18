use std::io::Stdout;
use std::time::Duration;
use crossterm::event;
use crossterm::event::{Event, KeyCode};
use ratatui::backend::CrosstermBackend;
use ratatui::prelude::{Color, Span, Style, Stylize};
use ratatui::Terminal;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;
use prefab::template::TemplateOption;
use crate::tui::option_ui::{EditorStatus, OptionUi};

pub struct NumberUI {
    option: TemplateOption,
    input: Input,
    status: EditorStatus,
    is_float: bool,
    name: String,
}

impl NumberUI {
    pub fn new(option: TemplateOption, name: String) -> NumberUI {
        let float = matches!(option, TemplateOption::Float {..});

        let val = option.get_value().unwrap_or("".to_string());
        NumberUI {
            option,
            input: Input::from(val),
            status: EditorStatus::Continue,
            is_float: float,
            name
        }
    }
}

impl OptionUi for NumberUI {
    fn render_edit(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<()> {

        let val = self.input.value().to_string();
        let prompt = self.option.get_prompt();
        let name = self.get_name();
        let mandatory = self.option.is_mandatory();

        let text = vec![
            Line::from(vec![Span::raw(prompt).green(), Span::raw(":").yellow()]),
            Line::from(vec![Span::raw(val.clone())])
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
                } else if KeyCode::Enter == key.code {
                    let v = self.input.value().clone().to_string();
                    let o = self.option.clone().set_value(v);
                    self.option = o.clone();
                    self.status = EditorStatus::Finished { option: o };
                    return Ok(());
                } else {
                    let orig = self.input.value().clone().to_string();

                    self.input.handle_event(&Event::Key(key));
                    let v = self.input.value().clone().to_string();

                    if v.is_empty() || v == *"-" || (self.is_float && v.parse::<f64>().is_ok()) ||
                        v.parse::<i64>().is_ok() {
                        //Keep value
                    }else {
                        self.input = Input::new(orig);
                    }
                }
            }
        }

        Ok(())

    }

    fn get_status(&self) -> anyhow::Result<EditorStatus> { Ok(self.status.clone())     }

    fn start_edit(&mut self) {
        self.status = EditorStatus::Continue;

        if let Some(v) = self.option.get_value() {
            self.input = Input::new(v);
        }else{
            self.input = Input::default();
        }
    }

    fn get_option(&self) -> TemplateOption { self.option.clone() }

    fn is_valid(&self) -> bool {
        true
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}