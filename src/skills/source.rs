use anyhow::{Context, ensure};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use walkdir::WalkDir;

// Keeping the temporary directory alive lets CLI and TUI select before installing.
pub(crate) struct Source {
    pub(crate) root: PathBuf,
    _checkout: Option<tempfile::TempDir>,
}

impl Source {
    pub(crate) fn open(input: &str) -> anyhow::Result<Self> {
        if Path::new(input).is_dir() {
            return Ok(Self {
                root: Path::new(input).canonicalize()?,
                _checkout: None,
            });
        }
        let (repo, tree) = github_source(input)?;
        let tmp = tempfile::tempdir()?;
        let root = tmp.path().join(repo.split_once('/').unwrap().1);
        let url = format!("https://github.com/{repo}.git");
        std::fs::create_dir(&root)?;
        git(&root, &["init", "-q", "--template="])?;
        let (revision, folder) = match tree {
            Some(tree) => {
                let refs = git(&root, &["ls-remote", "--refs", "--heads", "--tags", &url])?;
                resolve_revision(&tree, &refs)?
            }
            None => ("HEAD".into(), PathBuf::new()),
        };
        git(&root, &["fetch", "-q", "--depth=1", "--", &url, &revision])?;
        git(&root, &["checkout", "-q", "--detach", "FETCH_HEAD"])?;
        let selected = root
            .join(folder)
            .canonicalize()
            .context("GitHub skill path not found")?;
        ensure!(
            selected.starts_with(root.canonicalize()?),
            "Skill points outside repository"
        );
        Ok(Self {
            root: selected,
            _checkout: Some(tmp),
        })
    }

    pub(crate) fn candidates(&self) -> anyhow::Result<Vec<PathBuf>> {
        let mut found = Vec::new();
        let mut entries = WalkDir::new(&self.root).into_iter();
        while let Some(entry) = entries.next() {
            let entry = entry?;
            if !entry.file_type().is_dir() {
                continue;
            }
            if entry.file_name() == ".git" || entry.file_name() == "node_modules" {
                entries.skip_current_dir();
            } else if entry.path().join("SKILL.md").is_file() {
                found.push(entry.into_path());
                entries.skip_current_dir();
            }
        }
        found.sort();
        ensure!(!found.is_empty(), "No skill directory with SKILL.md found");
        Ok(found)
    }
}

fn github_source(input: &str) -> anyhow::Result<(String, Option<String>)> {
    let input = input.strip_prefix("https://github.com/").unwrap_or(input);
    let input = input
        .split(['?', '#'])
        .next()
        .unwrap_or(input)
        .trim_end_matches('/');
    let parts: Vec<_> = input.splitn(4, '/').collect();
    ensure!(
        parts.len() >= 2,
        "Expected a directory, owner/repo or GitHub URL"
    );
    let owner = parts[0];
    let repo = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
    for name in [owner, repo] {
        ensure!(
            !name.is_empty()
                && name != "."
                && name != ".."
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)),
            "Invalid GitHub repository name"
        );
    }
    let tree = match parts.as_slice() {
        [_, _] => None,
        [_, _, kind @ ("tree" | "blob"), tail] => {
            let decoded = percent_encoding::percent_decode_str(tail).decode_utf8()?;
            let tail = if *kind == "blob" {
                decoded
                    .strip_suffix("/SKILL.md")
                    .context("Select a SKILL.md file")?
            } else {
                &decoded
            };
            ensure!(
                !tail.contains(['\\', ':', '\0'])
                    && tail.split('/').all(|p| !p.is_empty()
                        && !matches!(p, "." | "..")
                        && !p.eq_ignore_ascii_case(".git")),
                "Invalid GitHub skill path"
            );
            Some(tail.to_owned())
        }
        _ => anyhow::bail!("Expected a GitHub repository, tree or SKILL.md link"),
    };
    Ok((format!("{owner}/{repo}"), tree))
}

fn resolve_revision(tree: &str, refs: &str) -> anyhow::Result<(String, PathBuf)> {
    // GitHub tree URLs combine the branch and path. Branch names can contain '/'.
    let refs: Vec<_> = refs
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect();
    for branch in Path::new(tree).ancestors() {
        let Some(branch) = branch.to_str() else {
            continue;
        };
        let head = format!("refs/heads/{branch}");
        let tag = format!("refs/tags/{branch}");
        if let Some((sha, _)) = refs.iter().find(|(_, name)| *name == head || *name == tag) {
            return Ok((
                (*sha).into(),
                tree[branch.len()..].trim_start_matches('/').into(),
            ));
        }
    }
    let (sha, path) = tree.split_once('/').unwrap_or((tree, ""));
    ensure!(
        sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()),
        "Branch, tag or commit not found"
    );
    Ok((sha.into(), path.into()))
}

fn git(root: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "core.hooksPath=.git/acli-empty-hooks",
            "-c",
            "core.fsmonitor=false",
        ])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .output()
        .context("Could not run Git to download this skill")?;
    ensure!(
        output.status.success(),
        "Git: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8(output.stdout)?)
}
