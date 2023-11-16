use std::collections::{HashSet, VecDeque, HashMap};
use std::ffi::OsStr;
use std::fs::{read, write};
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::thread::spawn;

use exif::{Exif, Reader, Tag, In};
use iced_futures::futures;
use iced_futures::subscription::Recipe;
use itertools::Itertools;
use regex::Regex;
use ring::digest::{Context, SHA256};
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

struct Photo {
    path: PathBuf,
    exif: Option<Exif>,
    accurate: bool,
}

impl Photo {
    fn has_valid_exif_date(&self) -> bool {
        self.get_exif_date().is_some()
    }

    fn get_exif_date(&self) -> Option<String> {
        self.exif.as_ref().map(|exif| {
            exif.get_field(Tag::DateTimeOriginal, In::PRIMARY)
                .map(|field| field.display_value().to_string())
        }).flatten()
    }

    fn get_best_effort_date(&self) -> Option<String> {
        self.get_exif_date()
            .map(|date| {
                let date_sep = ":\\-_";
                let exif_regex = Regex::new(&format!(r"(\d{{4}})[{}]?(\d{{2}})[{}]?(\d{{2}})", date_sep, date_sep)).unwrap();
                exif_regex.captures(&date).map(|captures| {
                    #[allow(unstable_name_collisions)]
                    captures.iter().skip(1)
                        .flat_map(|group| group.map(|g| g.as_str().to_string()))
                        .intersperse(String::from("-"))
                        .collect()
                })
            })
            .flatten()
            .or_else(|| {
                let filename_regex = Regex::new(r"(?:IMG-)?(\d{4})(\d{2})(\d{2})_.*").unwrap();
                let x = filename_regex.captures(self.path.file_name().to_owned().unwrap().to_str().unwrap())
                    .map(|captures| {
                        #[allow(unstable_name_collisions)]
                        captures.iter().skip(1)
                            .flat_map(|group| group.map(|g| g.as_str().to_string()))
                            .intersperse(String::from("-"))
                            .collect()
                    });
                dbg!(&x);
                x
            })
    }

    fn is_accurate(&self) -> bool {
        self.accurate
    }
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
            while let Some(path) = deque.pop_front() {
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
                    // TODO: Ignore errors? Maybe report them nicely later
                }
            }

            // Let's compute a hashmap of rewritables :)
            let mut final_files: HashMap<Vec<u8>, Vec<Photo>> = HashMap::new();
            for (idx, file) in files.iter().enumerate() {
                if let Ok(data) = read(file) {
                    let key: Vec<u8> = if filter_duplicates {
                        // At some point in the future, if we want to parallelise this, we'd use a buffered
                        // reader?
                        let mut ctx = Context::new(&SHA256);
                        ctx.update(&data);
                        ctx.finish().as_ref().to_vec()
                    } else {
                        idx.to_be_bytes().to_vec()
                    };

                    let mut cursor = Cursor::new(&data);
                    let exif = Reader::new().read_from_container(&mut cursor).ok();

                    final_files.entry(key).or_default().push(Photo {
                        path: file.clone(),
                        exif,
                        accurate: true
                    })
                }
            }

            let len = final_files.len() as u32;

            for (idx, photo) in final_files.values().enumerate() {
                // TODO: figure out correct date using `photo`
                let photo = match photo.iter().reduce(|first, second| {
                            // TODO: research if second condition matters (exif data should be
                            // valid, or at least we're not able to resolve a conflict anyway...?)
                            if !first.has_valid_exif_date() && (second.has_valid_exif_date() || (!first.is_accurate() && second.is_accurate())) {
                                second
                            } else {
                                first
                            }
                        }) {
                    Some(photo) => photo,
                    None => continue
                };

                let res = {
                    let relative = photo.path.strip_prefix(&pathclone).unwrap();
                    let mut newpath = outclone.join(relative);
                    let path = if rename_files {
                        let filename = newpath.file_name().unwrap().to_owned();
                        // println!("{:?}", filename);
                        if let Some(date) = photo.get_best_effort_date() {
                            let ext = newpath.extension().unwrap().to_owned();
                            let mut date: std::ffi::OsString = date.into();
                            let spacer: std::ffi::OsString = "_".into();
                            date.push(spacer);
                            date.push(filename);
                            newpath.set_file_name(date);
                            newpath.set_extension(ext);
                            write(&newpath, read(&photo.path).unwrap()).unwrap();
                            &newpath
                        } else {
                            &photo.path
                        }
                    } else {
                        &photo.path
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
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        res
                    }
                    _ => sender.send(Update::Progress {
                        path: photo.path.to_owned(),
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
