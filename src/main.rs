#![windows_subsystem = "windows"]

use iced::*;
use iced::{
    button, executor, Align, Application, Button, Clipboard, Column, Command, Element, Length, Row,
    Rule, Settings, Subscription, Text,
};
use std::env;
use std::fmt::Display;
use std::fs::canonicalize;
use std::path::PathBuf;

mod extract;
mod file_task;

fn main() {
    let path = env::args().nth(1);
    match path {
        Some(s) => extract::extract_mp4(s).unwrap(),
        None => open_ui().unwrap(),
    }
}

fn open_ui() -> iced::Result {
    let mut settings = Settings::default();
    settings.window.size = (500, 375);
    MotionSplit::run(settings)
}

enum Status {
    Success,
    Progress(String),
    Working,
    Issue(String),
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Success => "Successfully extracted the motion pictures as mp4s",
                Self::Working => "Starting conversion...",
                Self::Issue(res) => res,
                Self::Progress(res) => res,
            }
        )
    }
}

#[derive(Default)]
struct MotionSplit {
    path: Option<PathBuf>,
    output_path: Option<PathBuf>,
    status: Option<Status>,
    filter_duplicates: bool,
    rename_files: bool,
    extract_mp4: bool,
    converting: bool,
    pick_file_button: button::State,
    pick_directory_button: button::State,
    pick_destination_button: button::State,
    convert_button: button::State,
    path_display: text_input::State,
    output_path_display: text_input::State,
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectFile,
    SelectDirectory,
    SelectDestination,
    ToggleDuplicate(bool),
    ToggleRename(bool),
    ToggleMotionExtract(bool),
    Convert,
    TaskUpdate(file_task::Update),
    Noop,
}

fn path_to_str(path: Option<&PathBuf>) -> &str {
    path.map(|p| p.to_str())
        .flatten()
        .map(|s| {
            if cfg!(windows) {
                s.trim_start_matches(r"\\?\")
            } else {
                s
            }
        })
        .unwrap_or("None")
}

impl Application for MotionSplit {
    type Message = Message;
    type Executor = executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("MotionSplit")
    }

    fn update(&mut self, message: Message, _clipboard: &mut Clipboard) -> Command<Self::Message> {
        if let Message::ToggleDuplicate(state) = message {
            self.filter_duplicates = state;
            return Command::none();
        }
        if let Message::ToggleRename(state) = message {
            self.rename_files = state;
            return Command::none();
        }
        if let Message::ToggleMotionExtract(state) = message {
            self.extract_mp4 = state;
            return Command::none();
        }
        if let Message::TaskUpdate(update) = message {
            match update {
                file_task::Update::Progress { path, done, total } => {
                    if done == total {
                        self.converting = false;
                        self.status = Some(Status::Success);
                    } else {
                        let mut path_message = path.to_string_lossy().into_owned();
                        if cfg!(windows) {
                            path_message = path_message.trim_start_matches(r"\\?\").to_string();
                        }
                        self.status = Some(Status::Progress(format!(
                            "{}: {}/{}",
                            path_message, done, total
                        )));
                    }
                }
                file_task::Update::Error(s) => self.status = Some(Status::Issue(s)),
            }

            return Command::none();
        }
        if let Message::Convert = message {
            match (self.path.as_ref(), self.output_path.as_ref()) {
                (Some(_), Some(_)) => {
                    self.status = Some(Status::Working);
                    self.converting = true;
                }
                (None, _) => {
                    self.status = Some(Status::Issue(
                        "Please select a file or directory to convert".into(),
                    ))
                }
                (_, None) => {
                    self.status = Some(Status::Issue(
                        "Please select a file or directory to write to".into(),
                    ))
                }
            }
            return Command::none();
        }

        if self.converting {
            return Command::none();
        }

        // The destination should have the same pathbuf type (file/dir) as the path
        let to_match = if let Message::SelectDestination = message {
            if let Some(path) = self.path.as_ref() {
                if path.is_dir() {
                    Message::SelectDirectory
                } else {
                    Message::SelectFile
                }
            } else {
                Message::Noop
            }
        } else {
            message.clone()
        };

        let dialog = native_dialog::FileDialog::default();
        let path = match to_match {
            Message::SelectFile => match dialog.show_open_single_file() {
                Ok(opt) => opt,
                Err(e) => {
                    dbg!(e);
                    None
                }
            },
            Message::SelectDirectory => match dialog.show_open_single_dir() {
                Ok(opt) => opt,
                Err(e) => {
                    dbg!(e);
                    None
                }
            },
            _ => return Command::none(),
        };

        let opt = path
            .map(|buf| match canonicalize(buf) {
                Err(e) => {
                    dbg!(e);
                    None
                }
                Ok(x) => Some(x),
            })
            .flatten();
        match message {
            Message::SelectDestination => self.output_path = opt,
            Message::SelectFile | Message::SelectDirectory => self.path = opt,
            _ => {}
        }
        Command::none()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        if self.converting {
            Subscription::from_recipe(file_task::FileTask::new(
                self.path.as_ref().unwrap().clone(),
                self.output_path.as_ref().unwrap().clone(),
                self.filter_duplicates,
                self.rename_files,
                self.extract_mp4,
            ))
        } else {
            Subscription::none()
        }
    }

    fn view(&mut self) -> Element<Message> {
        let path_message = path_to_str(self.path.as_ref());
        let output_path_message = path_to_str(self.output_path.as_ref());

        let mut pick_file = Button::new(&mut self.pick_file_button, Text::new("Select file"));
        let mut pick_directory = Button::new(
            &mut self.pick_directory_button,
            Text::new("Select directory"),
        );
        let mut pick_destination = Button::new(
            &mut self.pick_destination_button,
            Text::new("Select destination"),
        );
        let mut convert = Button::new(&mut self.convert_button, Text::new("Convert file(s)"));

        if !self.converting {
            pick_file = pick_file.on_press(Message::SelectFile);
            pick_directory = pick_directory.on_press(Message::SelectDirectory);
            if self.path.is_some() {
                pick_destination = pick_destination.on_press(Message::SelectDestination);
                if self.output_path.is_some() {
                    convert = convert.on_press(Message::Convert);
                }
            }
        }
        self.path_display.unfocus();

        Column::new()
            .push(
                Column::new()
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
                    .padding(20)
                    .spacing(5)
                    .align_items(Align::Start)
                    .push(
                        TextInput::new(&mut self.path_display, path_message, path_message, |_| {
                            Message::Noop
                        })
                        .padding(3),
                    )
                    .push(
                        Row::new()
                            .spacing(20)
                            .align_items(Align::Center)
                            .push(pick_file)
                            .push(pick_directory)
                            .push(Space::new(Length::Fill, Length::Shrink)),
                    )
                    .push(
                        TextInput::new(
                            &mut self.output_path_display,
                            output_path_message,
                            output_path_message,
                            |_| Message::Noop,
                        )
                        .padding(3),
                    )
                    .push(pick_destination)
                    .push(Checkbox::new(
                        self.filter_duplicates,
                        "Filter duplicates",
                        Message::ToggleDuplicate,
                    ))
                    .push(Checkbox::new(
                        self.rename_files,
                        "Rename files",
                        Message::ToggleRename,
                    ))
                    .push(Checkbox::new(
                        self.extract_mp4,
                        "Extract Samsung motion pictures",
                        Message::ToggleMotionExtract,
                    )),
            )
            .push(
                Column::new()
                    .align_items(Align::Center)
                    .push(
                        Row::new()
                            .padding(10)
                            .align_items(Align::End)
                            .push(Space::new(Length::Fill, Length::Shrink))
                            .push(convert),
                    )
                    .push(Rule::horizontal(0))
                    .push(
                        Row::new()
                            .padding(10)
                            .align_items(Align::Start)
                            .push(Text::new("Status: "))
                            .push(
                                Text::new(
                                    self.status
                                        .as_ref()
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| "".to_string()),
                                )
                                .width(Length::Fill),
                            ),
                    ),
            )
            .into()
    }
}
