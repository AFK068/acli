use anyhow::Context;
use std::{fs, io::Write, path::Path};

pub(crate) fn write(path: &Path, contents: &str) -> anyhow::Result<()> {
    // Replace the original only after the entire new file has been written.
    let mut file = tempfile::NamedTempFile::new_in(parent_dir(path))?;
    file.write_all(contents.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path)
        .with_context(|| format!("Could not save '{}'", path.display()))?;
    Ok(())
}

pub(crate) fn lock(path: &Path) -> anyhow::Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("Could not open lock '{}'", path.display()))?;
    file.lock()
        .with_context(|| format!("Could not lock '{}'", path.display()))?;
    Ok(file)
}

pub(crate) fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

pub(crate) fn create_dir(path: &Path) -> anyhow::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .with_context(|| format!("Could not create directory '{}'", path.display()))
}
