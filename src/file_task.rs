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

pub struct FileTask<P> {
    path: P,
}

impl<P> FileTask<P>
where
    P: AsRef<Path> + Send,
{
    pub fn new(path: P) -> Self {
        Self { path }
    }

    fn start_task(&mut self) -> UnboundedReceiver<Update> {
        let (sender, receiver): (UnboundedSender<Update>, UnboundedReceiver<Update>) =
            unbounded_channel();
        let pathclone: PathBuf = self.path.as_ref().into();
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
            for (idx, path) in files.iter().enumerate() {
                let res = match crate::extract::extract_mp4(path) {
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

impl<H, I, P> Recipe<H, I> for FileTask<P>
where
    H: Hasher,
    P: AsRef<Path> + Hash + Send + 'static,
{
    type Output = crate::Message;

    fn hash(&self, state: &mut H) {
        self.path.hash(state);
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
