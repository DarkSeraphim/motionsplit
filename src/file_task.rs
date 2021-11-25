use std::collections::{HashSet, VecDeque};
use std::ffi::OsStr;
use std::fs::{read, write};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::thread::spawn;

use iced_futures::futures;
use iced_futures::subscription::Recipe;
use regex::Regex;
use ring::digest::{Digest, SHA256};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone)]
pub enum Update {
    Progress {
        path: PathBuf,
        done: u32,
        total: u32,
    },
    Error(String),
}

pub struct FileTask<P, U> {
    path: P,
    output: U,
    filter_duplicates: bool,
    rename_files: bool,
    extract_mp4: bool,
}

impl<P, U> FileTask<P, U>
where
    P: AsRef<Path> + Send,
    U: AsRef<Path> + Send,
{
    pub fn new(
        path: P,
        output: U,
        filter_duplicates: bool,
        rename_files: bool,
        extract_mp4: bool,
    ) -> Self {
        Self {
            path,
            output,
            filter_duplicates,
            rename_files,
            extract_mp4,
        }
    }

    fn start_task(&mut self) -> UnboundedReceiver<Update> {
        let (sender, receiver): (UnboundedSender<Update>, UnboundedReceiver<Update>) =
            unbounded_channel();
        let pathclone: PathBuf = self.path.as_ref().into();
        let outclone: PathBuf = self.output.as_ref().into();
        let rename_files = self.rename_files;
        let filter_duplicates = self.filter_duplicates;
        let extract_mp4 = self.extract_mp4;
        spawn(move || {
            let ext: Option<&OsStr> = Some("jpg".as_ref());
            let mut deque = VecDeque::from([pathclone.clone()]);
            let mut files = Vec::new();
            let mut visited = HashSet::new();
            loop {
                let current_directory = deque.pop_front();
                if let Some(path) = current_directory {
                    visited.insert(path.clone());
                    if path.is_file() {
                        if path.extension() == ext {
                            files.push(path);
                        }
                    } else if path.is_dir() {
                        if let Ok(entries) = path.read_dir() {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if !visited.contains(&path) {
                                    deque.push_back(path);
                                }
                            }
                        }
                        // Ignore errors? Maybe report them nicely later
                    }
                } else {
                    break;
                }
            }
            let len = files.len() as u32;
            let mut seen_files: HashSet<Vec<u8>> = HashSet::new();
            let regex = Regex::new(r"(?:IMG-)?(\d{8})_.*").unwrap();

            for (idx, path) in files.iter().enumerate() {
                let data = read(path).unwrap();
                let hash: Digest = ring::digest::digest(&SHA256, &data);
                let res = if filter_duplicates && !seen_files.insert(hash.as_ref().to_vec()) {
                    Ok(())
                } else {
                    let relative = path.strip_prefix(&pathclone).unwrap();
                    let mut newpath = outclone.join(relative);
                    let path = if rename_files {
                        let filename = newpath.file_name().unwrap().to_owned();
                        // println!("{:?}", filename);
                        if let Some(captures) = regex.captures(filename.to_str().unwrap()) {
                            let ext = newpath.extension().unwrap().to_owned();
                            let mut date: std::ffi::OsString = captures[1].into();
                            let spacer: std::ffi::OsString = "_".into();
                            date.push(spacer);
                            date.push(filename);
                            newpath.set_file_name(date);
                            newpath.set_extension(ext);
                            write(&newpath, data).unwrap();
                            &newpath
                        } else {
                            path
                        }
                    } else {
                        path
                    };
                    if extract_mp4 {
                        crate::extract::extract_mp4(path)
                    } else {
                        Ok(())
                    }
                };

                let res = match res {
                    Err(e) => {
                        let res = sender.send(Update::Error(e.to_string()));
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        res
                    }
                    _ => sender.send(Update::Progress {
                        path: path.to_owned(),
                        done: idx as u32,
                        total: len,
                    }),
                };
                if res.is_err() {
                    panic!("Failed to send message");
                }
            }
            let res = sender.send(Update::Progress {
                path: pathclone,
                done: len,
                total: len,
            });
            if res.is_err() {
                panic!("Failed to send message");
            }
        });
        receiver
    }
}

impl<H, I, P, U> Recipe<H, I> for FileTask<P, U>
where
    H: Hasher,
    P: AsRef<Path> + Hash + Send + 'static,
    U: AsRef<Path> + Hash + Send + 'static,
{
    type Output = crate::Message;

    fn hash(&self, state: &mut H) {
        self.path.hash(state);
        self.output.hash(state);
        self.filter_duplicates.hash(state);
        self.rename_files.hash(state);
        self.extract_mp4.hash(state);
    }

    fn stream(
        mut self: Box<Self>,
        _input: iced_futures::BoxStream<I>,
    ) -> iced_futures::BoxStream<Self::Output> {
        let mut receiver = self.start_task();

        Box::pin(futures::stream::poll_fn(move |context| {
            receiver
                .poll_recv(context)
                .map(|opt| opt.map(crate::Message::TaskUpdate))
        }))
    }
}
