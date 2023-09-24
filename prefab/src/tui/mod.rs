mod option_ui;
mod text;
mod boolean;
mod number;
mod choice;

use std::{io::{Stdout, self}, time::Duration, collections::HashMap};
use crossterm::{terminal::{enable_raw_mode, EnterAlternateScreen, disable_raw_mode, LeaveAlternateScreen}, execute, event::{self, Event, KeyCode}};
use ratatui::{prelude::CrosstermBackend, Terminal, widgets::{ListItem, List, Block, Borders, ListState}, style::{Modifier, Style, Color, Stylize}, text::{Line, Span}};
use anyhow::Result;
use prefab::template::TemplateOption;

use crate::tui::boolean::BooleanUI;
use crate::tui::choice::ChoiceUI;
use crate::tui::text::TextUI;
use crate::tui::number::NumberUI;
use crate::tui::option_ui::{EditorStatus, OptionUi};

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

pub fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen,)?;
    Ok(terminal.show_cursor()?)
}

pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, options: &mut HashMap<String,TemplateOption>) -> Result<()> {
    option_menu(terminal, options)
}

fn option_menu(terminal: &mut Terminal<CrosstermBackend<Stdout>>, options: &mut HashMap<String, TemplateOption>) -> Result<()> {
    let mut state = ListState::default();
    state.select(Some(0));

    let mut elements = get_elements(options).unwrap();

    loop {
        let not_ready = elements.iter().any(|e| !e.is_valid() || (e.get_option().is_empty() && e.get_option().is_mandatory()));

        terminal.draw(|frame| {

            let mut items: Vec<ListItem> = elements.iter()
                .map(|e| e.render_list_item().unwrap())
                .collect();


            if not_ready {
                items.push(ListItem::new(Line::from(vec![Span::raw("--Done--").red()])));
            }else{
                items.push(ListItem::new(Line::from(vec![Span::raw("--Done--").green()])));
            }

            let list = List::new(items)
                .block(Block::default().title("Options").borders(Borders::ALL))
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
                .highlight_symbol(">>");

            frame.render_stateful_widget(list, frame.size(), &mut state)
        })?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if KeyCode::Esc == key.code {
                    break;
                }

                if KeyCode::Up == key.code {
                    if let Some(index) = state.selected() {
                        let mut index = index;

                        if index == 0 {
                            index = elements.len() - 1;
                        } else if index > elements.len() - 1 {
                            index = 0;
                        } else {
                            index -= 1;
                        }

                        state.select(Some(index));
                    }
                }

                if KeyCode::Down == key.code {
                    if let Some(index) = state.selected() {
                        let mut index = index;

                        if index >= options.len() {
                            index = 0;
                        } else {
                            index += 1;
                        }

                        state.select(Some(index));
                    }
                }

                if KeyCode::Enter == key.code {
                    if let Some(index) = state.selected() {
                        let mut index = index;

                        if index == elements.len() && not_ready {
                            continue;
                        }

                        //Handle done option
                        if index == elements.len() {
                            apply_elements_to_options(options, &elements);
                            break;
                        }

                        let element = elements.get_mut(index).unwrap();

                        element.start_edit();

                        loop {
                            element.render_edit(terminal)?;
                            element.update_input()?;
                            match element.get_status()?{
                                EditorStatus::Continue => {}
                                EditorStatus::Cancel => break,
                                EditorStatus::Finished { option:_ } => {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn apply_elements_to_options(options:&mut HashMap<String, TemplateOption>, elements: &Vec<Box<dyn OptionUi>>) {
    let opts =options.clone();
    let keys: Vec<&String> = opts.keys().collect();

    for i in 0..keys.len() {
        let mut op = options.get_mut(keys[i]).unwrap();
        *op = elements[i].get_option().clone();
    }
}

fn get_elements(options: &HashMap<String, TemplateOption>) -> Result<Vec<Box<dyn OptionUi>>> {
    let mut result: Vec<Box<dyn OptionUi>> = vec![];
    for k in options.keys() {
        let opt = options.get(k).unwrap();

        result.push(match opt{
            TemplateOption::FreeText { .. } => Box::new(TextUI::new(opt.clone(), k.clone())),
            TemplateOption::Boolean { .. } => Box::new(BooleanUI::new(opt.clone(), k.clone())),
            TemplateOption::Integer { .. } => Box::new(NumberUI::new(opt.clone(), k.clone())),
            TemplateOption::Float { .. } => Box::new(NumberUI::new(opt.clone(), k.clone())),
            TemplateOption::Regex { .. } => Box::new(TextUI::new(opt.clone(), k.clone())),
            TemplateOption::Choice { .. } => Box::new(ChoiceUI::new(opt.clone(), k.clone())),
        });
    }

    Ok(result)
}