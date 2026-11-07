use anyhow::Context;
use std::path::Path;
use std::process::Command;

pub(super) fn run(path: &Path, name: &str) -> anyhow::Result<()> {
    let mut command = crate::app::launch::prepare(path, name)?;
    execute(&mut command)
}

// Hand over the terminal and exit with the agent's status. This is CLI-specific.
fn execute(command: &mut Command) -> anyhow::Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    #[cfg(unix)]
    let error = {
        use std::os::unix::process::CommandExt;
        command.exec()
    };
    #[cfg(not(unix))]
    let error = match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => error,
    };

    if error.kind() == std::io::ErrorKind::NotFound {
        anyhow::bail!("Could not find '{program}'. Install it and add it to PATH");
    }
    Err(error).with_context(|| format!("Could not start '{program}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_missing_executable() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let mut command = Command::new(directory.path().join("missing-agent"));
        let error = execute(&mut command).unwrap_err();
        assert!(error.to_string().contains("Install it and add it to PATH"));
        Ok(())
    }
}
