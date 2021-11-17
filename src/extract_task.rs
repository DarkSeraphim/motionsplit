use iced_futures::futures;
use std::path::Path;
use std::hash::{Hash, Hasher};
use iced_native::subscription::Recipe;

pub struct ExtractTask<P> {
    path: P
}

impl<H, P> Recipe<H, P> for ExtractTask<P> 
    where H: Hasher, 
          P: AsRef<Path> + Hash{
        type Output = ();

        fn hash(&self, state: &mut H) {
            self.path.hash(state);
        }

        fn stream(self: Box<Self>, 
                  input: iced_futures::BoxStream<P>,
        ) -> iced_futures::BoxStream<Self::Output> {
            todo!()
        }
}

pub enum Status {
    Progress {path: String, done: u32, total: u32 },
    Done,
}
