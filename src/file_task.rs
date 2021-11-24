use std::collections::{HashSet, VecDeque};
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::thread::spawn;

use iced_futures::futures;
use iced_futures::subscription::Recipe;
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
}

impl<P, U> FileTask<P, U>
where
    P: AsRef<Path> + Send,
    U: AsRef<Path> + Send,
{
    pub fn new(path: P, output: U, filter_duplicates: bool, rename_files: bool) -> Self {
        Self { path, output, filter_duplicates, rename_files }
    }

    fn start_task(&mut self) -> UnboundedReceiver<Update> {
        let (sender, receiver): (UnboundedSender<Update>, UnboundedReceiver<Update>) =
            unbounded_channel();
        let pathclone: PathBuf = self.path.as_ref().into();
        let outclone: PathBuf = self.output.as_ref().into();
        let rename_files = self.rename_files;
        let filter_duplicates = self.filter_duplicates;
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
            let mut seen_files: HashSet<&[u8]> = HashSet::new();

            for (idx, path) in files.iter().enumerate() {
                // TODO: compute hash
                let hash: &[u8] = &[0];
                let res = if filter_duplicates && !seen_files.insert(hash) {
                    Ok(())
                } else {
                    let relative = path.strip_prefix(&pathclone).unwrap();
                    let mut newpath = outclone.join(relative); 
                    if rename_files {
                        let filename = newpath.file_name().unwrap().to_owned();
                        // TODO: overwrite filename
                        newpath.set_file_name(filename);
                    }
                    crate::extract::extract_mp4(path)
                };

                let res = match res {
                    Err(e) => {
                        let res = sender.send(Update::Error(e.to_string()));
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        res
                    },
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
