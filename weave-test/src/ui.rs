use std::io::{Write, stdout};
use std::str::FromStr;
use crossterm::{cursor, QueueableCommand, style, terminal};
use crossterm::style::{Print, PrintStyledContent, Stylize};
use tokio::sync::mpsc;
use tokio::sync::mpsc::{Sender};
use tokio::task::JoinHandle;
use crate::test_results::TestResult;
use anyhow::{bail, Error, Result};
use crossterm::cursor::MoveDown;
use crossterm::terminal::ClearType;

#[derive(Debug,PartialEq, Clone)]
pub enum UIFormat {
    Colour,
    Plain,
    None,
    Json,
}

pub enum UIMessage {
    Text{message: String},
    StartSuite{name: String},
    FinishSuite{name: String, state: TestResult, lines: u16},
    StartStep{name: String},
    FinishStep{name: String, state: TestResult, lines: u16},
    Finish,
    Assert{message: String, success: bool},
    ReportSetInstance {index: usize},
    StartSet {name: String},
    FinishSet,
}

pub struct Ui {
    sender: Option<Sender<UIMessage>>,
    handler: Option<JoinHandle<Result<()>>>,
    lines_to_suite: u16,
    lines_to_step: u16,
    format: UIFormat,
}

impl FromStr for UIFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let text = s.to_string();
        let text = text.to_lowercase();
        let text = text.trim().to_string();

        if text == "colour" || text == "color"{
            return Ok(UIFormat::Colour);
        }

        if text == "plain" {
            return Ok(UIFormat::Plain);
        }

        if text == "none" {
            return Ok(UIFormat::None);
        }

        if text == "json" {
            return Ok(UIFormat::Json);
        }

        bail!("Unknown ui format passed in!");

    }
}

impl Ui {
   pub async fn new(format: UIFormat) -> Ui {

       let mut sender : Option<Sender<UIMessage>> = None;
       let mut handler: Option<JoinHandle<Result<()>>> = None;


       if format == UIFormat::Colour {
           let (tx, result) = Self::setup_colour_ui();

           sender = Some(tx);
           handler = Some(result);
       }

       Ui{
           sender,
           handler,
           lines_to_suite: 0,
           lines_to_step: 0,
           format
       }
   }

   fn setup_colour_ui() -> (Sender<UIMessage>, JoinHandle<Result<(), Error>>) {
       let (tx, mut rx) = mpsc::channel(100);

       let result = tokio::spawn(async move {
           let mut stdout = stdout();

           while let Some(msg) = rx.recv().await {
               match msg {
                   UIMessage::Text { message } => {
                       let message = message + "\n";
                       stdout
                           .queue(style::PrintStyledContent("\t\tℹ ".blue()))?
                           .queue(style::Print(message))?;
                   }
                   UIMessage::Finish => {
                       break;
                   },
                   UIMessage::StartSuite { name } => {
                       let name = name + "\n";
                       stdout
                           .queue(Print("❱ ["))?
                           .queue(PrintStyledContent("Running".yellow()))?
                           .queue(Print("]: "))?
                           .queue(Print(name))?;
                   }
                   UIMessage::FinishSuite { name, state, lines } => {
                       stdout
                           .queue(cursor::MoveUp(lines))?
                           .queue(terminal::Clear(ClearType::CurrentLine))?
                           .queue(Print("❱ ["))?;

                       let name = name + "\n";

                       match state {
                           TestResult::Pass => {
                               stdout.queue(PrintStyledContent("Pass".green()))?;
                           }
                           TestResult::Fail => {
                               stdout.queue(PrintStyledContent("Fail".red()))?;
                           }
                           TestResult::NotRun => {
                               stdout.queue(PrintStyledContent("Not Run".grey()))?;
                           }
                           TestResult::Inconclusive => {
                               stdout.queue(PrintStyledContent("Inconclusive".dark_yellow()))?;
                           }
                           TestResult::Skip => {
                               stdout.queue(PrintStyledContent("Skipped".grey()))?;
                           }
                       }

                       stdout
                           .queue(Print("]: "))?
                           .queue(Print(name))?
                           .queue(cursor::MoveDown(lines))?;
                   }
                   UIMessage::StartStep { name } => {
                       let name = name + "\n";


                       stdout
                           .queue(Print("\t➤ ["))?
                           .queue(PrintStyledContent("Running".yellow()))?
                           .queue(Print("]: "))?
                           .queue(Print(name))?;
                   }
                   UIMessage::FinishStep { name, state, lines } => {
                       stdout
                           .queue(cursor::MoveUp(lines))?
                           .queue(terminal::Clear(ClearType::CurrentLine))?
                           .queue(Print("\t➤ ["))?;

                       let name = name + "\n";

                       match state {
                           TestResult::Pass => {
                               stdout.queue(PrintStyledContent("Pass".green()))?;
                           }
                           TestResult::Fail => {
                               stdout.queue(PrintStyledContent("Fail".red()))?;
                           }
                           TestResult::NotRun => {
                               stdout.queue(PrintStyledContent("Not Run".grey()))?;
                           }
                           TestResult::Inconclusive => {
                               stdout.queue(PrintStyledContent("Inconclusive".dark_yellow()))?;
                           }
                           TestResult::Skip => {
                               stdout.queue(PrintStyledContent("Skipped".grey()))?;
                           }
                       }

                       stdout
                           .queue(Print("]: "))?
                           .queue(Print(name))?
                           .queue(MoveDown(lines))?;
                   }
                   UIMessage::Assert { message, success } => {
                       let message = message + "\n";

                       if success{
                           stdout.queue(style::PrintStyledContent("\t\t✔ ".green()))?;
                       }else{
                           stdout.queue(style::Print("\t\t✘ ".red()))?;
                       }

                       stdout.queue(style::Print(message))?;
                   },
                   UIMessage::ReportSetInstance { index} => {
                       stdout.queue(PrintStyledContent(format!("\t⬛ - Row: {index}\n").blue()))?;
                   }
                   UIMessage::StartSet { name } => {
                       stdout.queue(PrintStyledContent(format!("\t⬛ Running Set {name}\n").blue()))?;

                   }
                   UIMessage::FinishSet => {
                       stdout.queue(PrintStyledContent("\t⬛ End of Data Set\n".blue()))?;
                   }
               }

               stdout.flush().unwrap();
           }

           Ok(())
       });
       (tx, result)
   }

    pub async fn start_suite(&mut self, name: &str) -> Result<()>{
        if self.format == UIFormat::Colour {
            let sender = self.sender.as_ref().unwrap();
            self.lines_to_suite = 1;
            sender.send(UIMessage::StartSuite { name: name.to_string() }).await?;
        }

        if self.format == UIFormat::Plain{
            println!("Starting Suite {name}");
        }

        Ok(())
    }

    pub async fn finish_suite(&mut self, name: &str, result: TestResult) -> Result<()>{
        if self.format == UIFormat::Colour {

            let sender = self.sender.as_ref().unwrap();
            sender.send(UIMessage::FinishSuite {
                name: name.to_string(),
                state: result,
                lines: self.lines_to_suite,
            }).await?;

            self.lines_to_suite = 0;

            return Ok(());
        }

        if self.format == UIFormat::Plain{
            println!("Finished Suite {name}: Result: {result}");
        }

        Ok(())
    }

    pub async fn start_step(&mut self, name: &str) -> Result<()>{
        if self.format == UIFormat::Colour {

            let sender = self.sender.as_ref().unwrap();
            sender.send(UIMessage::StartStep { name: name.to_string() }).await?;

            self.lines_to_step = 1;
            self.lines_to_suite += 1;
        }

        if self.format == UIFormat::Plain{
            println!("Starting Step {name}");
        }

        Ok(())
    }

    pub async fn finish_step(&mut self, name: &str, result: TestResult) -> Result<()>{
        if self.format == UIFormat::Colour {

            let sender = self.sender.as_ref().unwrap();
            sender.send(UIMessage::FinishStep {
                name: name.to_string(),
                state: result,
                lines: self.lines_to_step,
            }).await?;

            self.lines_to_step = 0;

            return Ok(());
        }

        if self.format == UIFormat::Plain{
            println!("Finished Step {name}: Result: {result}");
        }

        Ok(())
    }

    pub async fn close(mut self) -> Result<()> {
        if self.format == UIFormat::Colour {
            let sender = self.sender.as_ref().unwrap();
            sender.send(UIMessage::Finish).await?;
        }

        Ok(())
    }

    pub async fn print(&mut self, text: &str) -> Result<()>{
        if self.format == UIFormat::Colour {

            let sender = self.sender.as_ref().unwrap();
            sender.send(UIMessage::Text { message: text.to_string() }).await?;
            self.lines_to_suite += 1;
            self.lines_to_step += 1;
        }

        if self.format == UIFormat::Plain {
            println!("{text}");
        }

        Ok(())
    }

    pub async fn assert(&mut self, text: &str, success: bool) -> Result<()> {
        if self.format == UIFormat::Colour {
            let sender = self.sender.as_ref().unwrap();
            sender.send(UIMessage::Assert{ message: text.to_string(), success }).await?;
            self.lines_to_suite += 1;
            self.lines_to_step += 1;
        }

        if self.format == UIFormat::Plain {

            if success{
                println!("Assert Ok: {text}");
            } else{
                println!("Assert Failed: {text}");
            }
        }


        Ok(())
    }

    pub async fn start_set(&mut self, name: &str) -> Result<()>{
        if self.format == UIFormat::Colour {
            let sender = self.sender.as_ref().unwrap();
            sender.send(UIMessage::StartSet {name: name.to_string()}).await?;
            self.lines_to_suite += 1;
            self.lines_to_step += 1;
        }

        if self.format == UIFormat::Plain {
            println!("Starting Data Set {name}");
        }

        Ok(())
    }

    pub async fn finish_set(&mut self) -> Result<()> {
        if self.format == UIFormat::Colour {
            let sender = self.sender.as_ref().unwrap();
            sender.send(UIMessage::FinishSet).await?;
            self.lines_to_suite += 1;
            self.lines_to_step += 1;
        }

        if self.format == UIFormat::Plain {
            println!("Finished Data Set");
        }

        Ok(())
    }

    pub async fn report_set_row(&mut self, index: usize)-> Result<()>{
        if self.format == UIFormat::Colour {
            let sender = self.sender.as_ref().unwrap();
            sender.send(UIMessage::ReportSetInstance{index}).await?;
            self.lines_to_suite += 1;
            self.lines_to_step += 1;
        }

        if self.format == UIFormat::Plain {
            println!("Set Row: {index}");
        }

        Ok(())
    }
}

