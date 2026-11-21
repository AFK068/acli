use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::Command;

pub(super) fn run(path: &Path, name: &str) -> anyhow::Result<()> {
    execute(&mut prepare(path, name)?)
}

fn prepare(path: &Path, name: &str) -> anyhow::Result<Command> {
    crate::app::launch::prepare(
        path,
        name,
        &crate::skills::directory()?,
        &crate::skills::claude::directory()?,
        &crate::agents::mcp::user_dirs()?,
    )
}

// Hand over the terminal and exit with the agent's status. This is CLI-specific.
fn execute(command: &mut Command) -> anyhow::Result<()> {
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

    Err(start_error(command.get_program(), error))
}

fn start_error(program: &OsStr, error: io::Error) -> anyhow::Error {
    let program = program.to_string_lossy();
    if error.kind() == io::ErrorKind::NotFound {
        return anyhow::anyhow!("Could not find '{program}'. Install it and add it to PATH");
    }
    anyhow::Error::new(error).context(format!("Could not start '{program}'"))
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
