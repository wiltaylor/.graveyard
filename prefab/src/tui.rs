use std::{io::{Stdout, self}, time::Duration, mem, collections::HashMap};
use crossterm::{terminal::{enable_raw_mode, EnterAlternateScreen, disable_raw_mode, LeaveAlternateScreen}, execute, event::{self, Event, KeyCode}};
use ratatui::{prelude::{CrosstermBackend, Constraint, Direction, Layout}, Terminal, widgets::{Paragraph, ListItem, List, Block, Borders, ListState}, style::{Modifier, Style, Color, Stylize}, text::{Line, Span}};
use anyhow::Result;
use prefab::template::TemplateOption;

use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;
use regex::Regex as re;

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

fn get_option_list(options: &[TemplateOption]) -> Vec<ListItem> {
    let mut items: Vec<ListItem> = options.clone().iter().map(|opt| {
        ListItem::new(match opt {
            TemplateOption::FreeText { prompt, value } => {
                if let Some(value) = value {
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw(value).white()
                    ])

                }else{
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw("Empty").dark_gray()
                    ])
                }
            },
            TemplateOption::Boolean { prompt, value } => {
                if let Some(value) = value {
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        if *value { Span::raw("True").green() } else {Span::raw("False").red()}
                    ])
                }else{
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw("Empty").dark_gray()
                    ])
                }
            },
            TemplateOption::Integer { prompt, value } => {
                if let Some(value) = value {
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw(format!("{}", value))
                    ])
                }else{
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw("Empty").dark_gray()
                    ])
                }
            },
            TemplateOption::Float { prompt, value } => {
                if let Some(value) = value {
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw(format!("{}", value))
                    ])
                }else{
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw("Empty").dark_gray()
                    ])
                }
            },
            TemplateOption::Regex { prompt, pattern, value } => {
                if let Some(value) = value {
                    let mut valid = false;

                    if let Ok(regex) = re::new(pattern) {
                        valid = regex.is_match(value) 
                    }
                      
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        if valid { Span::raw(value).white() } else { Span::raw(value).red() }
                    ])

                }else{
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw("Empty").dark_gray()
                    ])
                }
            },
            TemplateOption::Choice { prompt, options:_, value } => {
                if let Some(value) = value {
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw(value).white()
                    ])

                }else{
                    Line::from(vec![
                        Span::raw(prompt).green(),
                        Span::raw(" => ").yellow(),
                        Span::raw("Empty").dark_gray()
                    ])
                }
            },
        })
    }).collect();

    items.push(ListItem::new(Line::from(vec![Span::raw("--Done--").green()])));

    items
}

fn run_edit_option(terminal: &mut Terminal<CrosstermBackend<Stdout>>, option: &TemplateOption) -> Result<TemplateOption> {
    let original = option.clone();
    let mut option = option.clone();
    let mut dec = false;
    let mut state = ListState::default();
    state.select(Some(0));

    loop {

        let mut new_value: Option<String> = None;
        
        match &option {
            TemplateOption::FreeText { prompt, value } => {

                let val = value.clone().unwrap_or("".to_string()); 
                let mut input = Input::new(val.clone());
                new_value = Some(val.clone());
               
                let text = vec![
                    Line::from(vec![Span::raw(prompt).green(), Span::raw(":").yellow()]),
                    Line::from(vec![Span::raw(val.clone())])
                ];
        
                terminal.draw(|frame| {
                    let paragraph = Paragraph::new(text)
                    .block(Block::default().title("[Edit]-(Enter: Confirm, Esq - Cancel) ").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White));
        
                    frame.set_cursor((val.len() as u16) + 1, 2);
                    frame.render_widget(paragraph, frame.size())
        
                })?;
        
                if event::poll(Duration::from_millis(250))? {
                    if let Event::Key(key) = event::read()? {
                        if KeyCode::Esc == key.code {
                            return Ok(original);
                        }else if KeyCode::Enter == key.code {
                            return Ok(option);
                        }else{
                            input.handle_event(&Event::Key(key));
                            new_value = Some(input.value().clone().to_string());
                        }
                    }
                }
            },
            TemplateOption::Boolean { prompt, value } => {
                let text = vec![
                    Line::from(vec![Span::raw(prompt).green(), Span::raw(":").yellow()]),
                    if let Some(v) = value {
                        if *v {
                            Line::from(vec![Span::raw("True").green()])
                        }else{
                            Line::from(vec![Span::raw("False").red()])
                        }  
                    } else {
                        Line::from(vec![Span::raw("Empty").dark_gray()])
                    }
                ];

                terminal.draw(|frame| {
                    let paragraph = Paragraph::new(text)
                    .block(Block::default().title("[Edit]-(Enter - Confirm, Esq - Cancel, Y - Check , N - Uncheck, E - Clear) ").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White));
        
                    frame.render_widget(paragraph, frame.size())
        
                })?;


                if event::poll(Duration::from_millis(250))? {
                    if let Event::Key(key) = event::read()? {
                        if KeyCode::Esc == key.code {
                            return Ok(original);
                        }else if KeyCode::Char('y') == key.code {
                            new_value = Some("true".to_string());
                        }else if KeyCode::Char('n') == key.code {
                            new_value = Some("false".to_string());
                        }else if KeyCode::Char('e') == key.code {
                            new_value = Some("".to_string());
                        }else if KeyCode::Enter == key.code {
                            return Ok(option);
                        }
                    }
                }

            },
            TemplateOption::Integer { prompt, value } => {
                let val = if let Some(v) = value { format!("{}", v) } else { "".to_string() }; 
                let mut input = Input::new(val.clone());
                
                new_value = Some(val.clone());
               
                let text = vec![
                    Line::from(vec![Span::raw(prompt).green(), Span::raw(":").yellow()]),
                    Line::from(vec![Span::raw(val.clone())])
                ];
        
                terminal.draw(|frame| {
                    let paragraph = Paragraph::new(text)
                    .block(Block::default().title("[Edit]-(Enter: Confirm, Esc - Cancel) ").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White));
        
                    frame.set_cursor((val.len() as u16) + 1, 2);
                    frame.render_widget(paragraph, frame.size())
        
                })?;
        
                if event::poll(Duration::from_millis(250))? {
                    if let Event::Key(key) = event::read()? {
                        if KeyCode::Esc == key.code {
                            return Ok(original);
                        }else if KeyCode::Enter == key.code {
                            return Ok(option);
                        }else if (key.code >= KeyCode::Char('0') && key.code <= KeyCode::Char('9')) || key.code == KeyCode::Backspace {
                            input.handle_event(&Event::Key(key));
                            new_value = Some(input.value().clone().to_string());
                        }
                    }
                }
            },
            TemplateOption::Float { prompt, value } => {
                let mut val = if let Some(v) = value { format!("{}", v) } else { "".to_string() }; 
                if dec && !val.contains('.') {
                    val += "."
                }
                let mut input = Input::new(val.clone());
                
                new_value = Some(val.clone());
               
                let text = vec![
                    Line::from(vec![Span::raw(prompt).green(), Span::raw(":").yellow()]),
                    Line::from(vec![Span::raw(val.clone())])
                ];
        
                terminal.draw(|frame| {
                    let paragraph = Paragraph::new(text)
                    .block(Block::default().title("[Edit]-(Enter: Confirm, Esc - Cancel) ").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White));
        
                    frame.set_cursor((val.len() as u16) + 1, 2);
                    frame.render_widget(paragraph, frame.size())
        
                })?;
        
                if event::poll(Duration::from_millis(250))? {
                    if let Event::Key(key) = event::read()? {
                        if KeyCode::Esc == key.code {
                            return Ok(original);
                        }else if KeyCode::Enter == key.code {
                            return Ok(option);
                        }else if (key.code >= KeyCode::Char('0') && key.code <= KeyCode::Char('9')) || key.code == KeyCode::Char('.') || key.code == KeyCode::Backspace {
                            if key.code == KeyCode::Char('.') {
                                dec = true;
                            }

                            if key.code == KeyCode::Backspace && val.ends_with('.') {
                                dec = false;
                            }
                            
                            input.handle_event(&Event::Key(key));
                            new_value = Some(input.value().clone().to_string());
                        }
                    }
                }

            },
            TemplateOption::Regex { prompt, pattern, value } => {
                let val = value.clone().unwrap_or("".to_string()); 
                let mut input = Input::new(val.clone());
                new_value = Some(val.clone());

                let mut valid = false;
                if let Ok(regex) = re::new(pattern) {
                    valid = regex.is_match(&val) 
                }
               
                let text = vec![
                    Line::from(vec![Span::raw(prompt).green(), Span::raw(":").yellow()]),
                    Line::from(vec![if valid {Span::raw(val.clone()) } else { Span::raw(val.clone()).red()}])
                ];
        
                terminal.draw(|frame| {
                    let paragraph = Paragraph::new(text)
                    .block(Block::default().title("[Edit]-(Enter: Confirm, Esc - Cancel) ").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White));
        
                    frame.set_cursor((val.len() as u16) + 1, 2);
                    frame.render_widget(paragraph, frame.size())
        
                })?;
        
                if event::poll(Duration::from_millis(250))? {
                    if let Event::Key(key) = event::read()? {
                        if KeyCode::Esc == key.code {
                            return Ok(original);
                        }else if KeyCode::Enter == key.code && valid {
                            return Ok(option);
                        }else{
                            input.handle_event(&Event::Key(key));
                            new_value = Some(input.value().clone().to_string());
                        }
                    }
                }
            },
            TemplateOption::Choice { prompt, options, value } => {
                let val = value.clone().unwrap_or("".to_string()); 
                let selected_item = if let Some(v) = value {
                    v
                } else { "" };
                let mut items: Vec<ListItem> = options.iter().map(|op| {
                    ListItem::new(if selected_item == op { Line::from(vec![Span::raw(op).yellow()])} else { Line::from(vec![Span::raw(op)])})
                }).collect();

                items.push(ListItem::new(Line::from(vec![Span::raw("--Empty--").dark_gray()])));

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
                
                if event::poll(Duration::from_millis(250))? {
                    if let Event::Key(key) = event::read()? {
                        if KeyCode::Esc == key.code {
                            return Ok(original);
                        }else if KeyCode::Enter == key.code  {
                            return Ok(option);
                        }
                        
                        if KeyCode::Up == key.code {
                            if let Some(index) = state.selected() {
                                let mut index = index;
        
                                if index == 0 {
                                    index = items.len() - 1;
                                }else if index > items.len() - 1 {
                                    index = 0;
                                }else{
                                    index -= 1;
                                }

                                if index == items.len() - 1{
                                    state.select(Some(index));
                                    new_value = Some("".to_string());

                                }else{
                                    state.select(Some(index));
                                    new_value = Some(options[index].clone());
                                }
        

                            }
                        }
        
                        if KeyCode::Down == key.code {
                            if let Some(index) = state.selected() {
                                let mut index = index;
        
                                if index >= options.len(){
                                    index = 0;
                                }else{
                                    index += 1;
                                }

                                if index == items.len() - 1 {
                                    state.select(Some(index));
                                    new_value = Some("".to_string())
                                }else{
                                    state.select(Some(index));
                                    new_value = Some(options[index].clone());
                                }
        
                            }
                        }

                    }
                }
            },
        }

        if let Some(value) = new_value {
            option = option.set_value(value);
        }
    }

}

fn run_setting_list(terminal: &mut Terminal<CrosstermBackend<Stdout>>, options: &mut HashMap<String, TemplateOption>) -> Result<()> {
    let mut state = ListState::default();
    state.select(Some(0));

    let keys :Vec<String> = options.keys().cloned().collect();
    let mut values: Vec<TemplateOption> = keys.iter().map(|x| options[x].clone()).collect();
    
    loop {
        let items = get_option_list(&values);
        terminal.draw(|frame| {
            let list = List::new(items.clone())
                .block(Block::default().title("Options").borders(Borders::ALL))
                .style(Style::default().fg(Color::White))
                .highlight_style(Style::default().add_modifier(Modifier::ITALIC))
                .highlight_symbol(">>");

            //let greeting = Paragraph::new("Hello World!");
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
                            index = items.len() - 1;
                        }else if index > items.len() - 1 {
                            index = 0;
                        }else{
                            index -= 1;
                        }

                        state.select(Some(index));
                    }
                }

                if KeyCode::Down == key.code {
                    if let Some(index) = state.selected() {
                        let mut index = index;

                        if index >= options.len(){
                            index = 0;
                        }else{
                            index += 1;
                        }

                        state.select(Some(index));
                    }
                }

                if KeyCode::Enter == key.code {
                    if let Some(index) = state.selected() {

                        // Done options
                        if index == items.len() - 1 {

                        let mut i= 0;
                        while i < values.len() {
                            *options.get_mut(&keys[i]).unwrap() = values[i].clone();
                            let _= mem::replace(&mut options.get_mut(&keys[i]), Some(&mut values[i]));
                            i += 1;
                        }

                        break;
                    }

                        

                        let result = run_edit_option(terminal, &values[index])?;
                        let _ = mem::replace(&mut values[index], result);
                    }
                }
            }

        }
    }
    Ok(())
   
}

pub fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, options: &mut HashMap<String,TemplateOption>) -> Result<()> {
    run_setting_list(terminal, options)
}