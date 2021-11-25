use std::fs::{read, write};
use std::path::Path;

pub fn extract_mp4(path: impl AsRef<Path>) -> std::io::Result<()> {
    let magic: &[u8; 16] = b"MotionPhoto_Data";
    let path = path.as_ref();
    if path.is_dir() {
        for entry in (path.read_dir()?).flatten() {
            extract_mp4(entry.path())?;
        }
        return Ok(());
    }

    let buf = read(&path)?;
    let idx = (0..buf.len() - magic.len()).find(|start| {
        let end = start + magic.len();
        &buf[*start..end] == magic
    });

    if idx.is_none() {
        return Ok(());
    }
    let idx = idx.unwrap();

    let mut path_buf = path.to_path_buf();
    path_buf.set_extension("");
    let mut file_name = path_buf.file_name().unwrap().to_owned();
    file_name.push("-motion.mp4");
    path_buf.set_file_name(file_name);

    write(path_buf, &buf[idx + magic.len()..])?;
    Ok(())
}
