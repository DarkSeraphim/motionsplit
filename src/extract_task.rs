use iced_futures::futures;
use iced_futures::subscription::Recipe;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::thread::spawn;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

type Update = (PathBuf, u32, u32);

pub struct ExtractTask<P> {
    path: P,
}

impl<P> ExtractTask<P>
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
            let mut deque = VecDeque::from([pathclone.clone()]);
            let mut files = Vec::new();
            let mut visited = HashSet::new();
            loop {
                let current_directory = deque.pop_front();
                if let Some(path) = current_directory {
                    visited.insert(path.clone());
                    if path.is_file() {
                        files.push(path);
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
                sender
                    .send((path.to_owned(), idx as u32, len))
                    .expect("Failed to send message");
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            sender
                .send((pathclone, len, len))
                .expect("Failed to send message");
        });
        receiver
    }
}

impl<H, I, P> Recipe<H, I> for ExtractTask<P>
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
            receiver.poll_recv(context).map(|opt| {
                opt.map(|(path, done, total)| {
                    let path = path.to_string_lossy().into_owned();
                    crate::Message::Progress { path, done, total }
                })
            })
        }))
    }
}
