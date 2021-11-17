use std::env;
use std::fs::canonicalize;
use std::path::PathBuf;

use iced::{button, Align, Button, Column, Element, Row, Settings, Text, Application, executor, Command, Clipboard, Subscription};

mod extract;
mod extract_task;

fn main() {
    let path = env::args().skip(1).next();
    match path {
        Some(s) => extract::extract_mp4(s).unwrap(),
        None => open_ui().unwrap()
    }
}

fn open_ui() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.size = (800, 600);
    MotionSplit::run(settings)
}

enum Status {
    Success,
    Working,
    Issue(String),
}

impl ToString for Status {
    fn to_string(&self) -> String {
        match self {
            Self::Success => String::from("Successfully extracted the motion pictures as mp4s"),
            Self::Working => String::from("Starting conversion..."),
            Self::Issue(res) => res.to_string()
        }
    }
}

#[derive(Default)]
struct MotionSplit {
    path: Option<PathBuf>,
    status: Option<Status>,
    converting: bool,
    pick_file_button: button::State,
    pick_directory_button: button::State,
    convert_button: button::State,
}

#[derive(Debug, Clone, Copy)]
enum Message {
    SelectFile,
    SelectDirectory,
    Convert,
}

impl Application for MotionSplit {
    type Message = Message;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>){
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("MotionSplit")
    }

    fn update(&mut self, message: Message, _clipboard: &mut Clipboard) -> Command<Self::Message> {
        if let Message::Convert = message {
            match self.path.as_ref() {
                Some(_) => {
                    self.status = Some(Status::Working);
                    self.converting = true;
                }
                None => {
                    self.status = Some(Status::Issue("Please select a file or directory to convert".into()))
                }
            }
            return Command::none();
        }

        if self.converting {
            return Command::none();
        }

        let dialog = native_dialog::FileDialog::default();
        let path = match message {
            Message::SelectFile => { 
                match dialog.show_open_single_file() {
                    Ok(opt) => opt,
                    Err(e) => {
                        dbg!(e);
                        None
                    }
                }
            },
            Message::SelectDirectory => {
                match dialog.show_open_single_dir() {
                    Ok(opt) => opt,
                    Err(e) => {
                        dbg!(e);
                        None
                    }
                }
            },
            _ => return Command::none(),
        };

        self.path = path.map(|buf| {
            match canonicalize(buf) {
                Err(e) => {
                    dbg!(e);
                    None
                },
                Ok(x) => Some(x)
            }
        }).flatten();
        Command::none()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        if self.converting {
            Subscription::from_recipe()
        } else {
            Subscription::none()
        }
    }

    fn view(&mut self) -> Element<Message> {
        let mut path_message = self.path.as_ref()
            .map(|p| p.to_str()).flatten()
            .unwrap_or("None");
        if cfg!(windows) && path_message.starts_with("\\\\?\\") {
           path_message = &path_message["\\\\?\\".len()..path_message.len()] 
        }
        Column::new()
            .padding(20)
            .align_items(Align::Center)
            .push(
                Row::new()
                    .spacing(20)
                    .align_items(Align::Center)
                    .push(
                        Button::new(&mut self.pick_file_button, Text::new("Select file"))
                            .on_press(Message::SelectFile)
                            
                    )
                    .push(
                        Button::new(&mut self.pick_directory_button, Text::new("Select directory"))
                            .on_press(Message::SelectDirectory)
                    )
            )
            .push(Text::new(path_message).size(20)) 
            .push(
                Button::new(&mut self.convert_button, Text::new("Convert file(s)"))
                    .on_press(Message::Convert)
            )
            .push(Text::new(self.status.as_ref().map(|s| s.to_string()).unwrap_or("".to_string())))
            .into()
    }
}
