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

    #[test]
    fn two_profiles_share_edits_and_keep_personal_skills() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let download = temp.path().join("download/review");
        fs::create_dir_all(download.join("references"))?;
        fs::write(download.join("SKILL.md"), "review")?;
        fs::write(download.join("references/check.md"), "before")?;
        let shared = temp.path().join("shared");
        let installed = crate::skills::install(&shared, &download)?;
        let profiles = [
            temp.path().join("work/skills"),
            temp.path().join("personal/skills"),
        ];
        for profile in &profiles {
            sync(&shared, profile)?;
            sync(&shared, profile)?;
            assert_eq!(
                fs::read_to_string(profile.join("review/references/check.md"))?,
                "before"
            );
            fs::create_dir(profile.join("local"))?;
        }
        fs::write(installed.join("SKILL.md"), "after")?;
        for profile in &profiles {
            assert_eq!(
                fs::read_to_string(profile.join("review/SKILL.md"))?,
                "after"
            );
        }
        crate::skills::remove(&shared, "review")?;
        for profile in &profiles {
            sync(&shared, profile)?;
            assert!(fs::symlink_metadata(profile.join("review")).is_err());
            assert!(profile.join("local").is_dir());
        }
        assert!(download.join("SKILL.md").is_file());
        Ok(())
    }
}
