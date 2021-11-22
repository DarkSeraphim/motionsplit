use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

pub fn extract_mp4(path: impl AsRef<Path>) -> std::io::Result<()> {
    let magic: [u8; 16] = [
        // M,    o,    t,    i,    o,    n,    P,    h,    o,    t,    o,    _,    D,    a,    t,
        0x4D, 0x6F, 0x74, 0x69, 0x6F, 0x6E, 0x50, 0x68, 0x6F, 0x74, 0x6F, 0x5F, 0x44, 0x61, 0x74,
        // a
        0x61,
    ];
    let path = path.as_ref();
    if path.is_dir() {
        for entry in (path.read_dir()?).flatten() {
            extract_mp4(entry.path())?;
        }
        return Ok(());
    }

    let mut f = File::open(&path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;

    let idx = (0..buf.len() - magic.len()).find(|start| {
        let end = start + magic.len();
        buf[*start..end] == magic
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

    let mut out = File::create(path_buf)?;
    out.write_all(&buf[idx + magic.len()..])?;

    Ok(())
}
