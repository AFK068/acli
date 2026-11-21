use crate::config::Agent;
use anyhow::{Context, ensure};
#[cfg(windows)]
pub(super) use fs::remove_dir as unlink;
#[cfg(windows)]
pub(super) use junction::{create as link, get_target as read_link};
use std::{fs, path::Path};
#[cfg(unix)]
pub(super) use std::{
    fs::{read_link, remove_file as unlink},
    os::unix::fs::symlink as link,
};

pub(super) fn points_to(link: &Path, source: &Path) -> bool {
    read_link(link).is_ok_and(|target| target == source)
}

pub(crate) fn connect(agent: Agent, profile: &Path, shared: &Path) -> anyhow::Result<()> {
    if matches!(agent, Agent::ClaudeCode | Agent::ZCode) {
        sync(shared, &profile.join(super::profile_skills(agent)))?;
    }
    Ok(())
}

fn sync(shared: &Path, target: &Path) -> anyhow::Result<()> {
    if let Ok(existing) = fs::read_link(target) {
        ensure!(
            existing == shared || target.canonicalize()? == shared.canonicalize()?,
            "Profile skills directory already points elsewhere: {}",
            target.display()
        );
        return Ok(());
    }
    let _lock = super::lock(shared)?;
    let names = super::list(shared)?;
    if names.is_empty() && !target.try_exists()? {
        return Ok(());
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(target)? {
        let entry = entry?;
        let source = shared.join(entry.file_name());
        if points_to(&entry.path(), &source) && !source.join("SKILL.md").is_file() {
            unlink(entry.path())?;
        }
    }
    for name in names {
        let source = shared.join(&name);
        let dest = target.join(&name);
        // Preserve local skills, including links owned by the profile.
        if dest.symlink_metadata().is_ok() {
            continue;
        }
        link(&source, &dest)
            .with_context(|| format!("Could not link shared skill at '{}'", dest.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_support::{TestApp, write_skill};

    #[test]
    fn shared_links_expose_the_whole_skill_and_its_edits() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let source = app.root.path().join("download/review");
        write_skill(&source, "review")?;
        fs::create_dir(source.join("references"))?;
        fs::write(source.join("references/check.md"), "before")?;
        let installed = crate::skills::install(&app.shared, &source)?;
        sync(&app.shared, &app.claude)?;
        let reference = app.claude.join("review/references/check.md");
        assert_eq!(fs::read_to_string(&reference)?, "before");
        fs::write(installed.join("references/check.md"), "after")?;
        assert_eq!(fs::read_to_string(reference)?, "after");
        Ok(())
    }

    #[test]
    fn removal_cleans_only_the_managed_link() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let source = app.root.path().join("download/review");
        write_skill(&source, "review")?;
        write_skill(&app.claude.join("local"), "personal")?;
        crate::skills::install(&app.shared, &source)?;
        sync(&app.shared, &app.claude)?;
        crate::skills::remove(&app.shared, "review")?;
        sync(&app.shared, &app.claude)?;
        assert!(app.claude.join("review").symlink_metadata().is_err());
        assert!(app.claude.join("local/SKILL.md").is_file());
        assert!(source.join("SKILL.md").is_file());
        Ok(())
    }
}
