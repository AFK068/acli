use super::{ProfileCommand, mcp, run_agent, skills};
use crate::config::storage;
use dialoguer::{Confirm, Input};
use ratatui::crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, enable_raw_mode},
};
use ratatui::{
    DefaultTerminal, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::{Color, Stylize},
    widgets::{Block, List, ListState, Paragraph, Tabs, Wrap},
};
use std::{
    ffi::OsString,
    io::{self, IsTerminal},
    path::Path,
};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Profiles,
    Skills,
    Mcp,
}

impl Page {
    const TABS: [(Self, &str); 3] = [
        (Self::Profiles, "Profiles"),
        (Self::Skills, "Skills"),
        (Self::Mcp, "MCP"),
    ];

    fn rows(self, path: &Path) -> anyhow::Result<Vec<Row>> {
        let rows = match self {
            Self::Profiles => storage::load(path)?
                .profiles
                .into_iter()
                .map(|(name, profile)| Row {
                    name: name.into(),
                    details: format!("{}  ·  {}", profile.agent, profile.auth),
                })
                .collect(),
            Self::Skills => skills::list(path)?
                .into_iter()
                .map(|name| Row {
                    name,
                    details: String::new(),
                })
                .collect(),
            Self::Mcp => mcp::list(path)?
                .into_iter()
                .map(|(name, server)| Row {
                    name: name.into(),
                    details: server.command,
                })
                .collect(),
        };
        Ok(rows)
    }

    fn add(self, path: &Path) -> anyhow::Result<()> {
        match self {
            Self::Profiles => ProfileCommand::Add.execute(path),
            Self::Skills => {
                let source = Input::new()
                    .with_prompt("Skill directory or GitHub source")
                    .interact_text()?;
                skills::Command::Add {
                    source,
                    skill: None,
                }
                .execute(path)
            }
            Self::Mcp => mcp::Command::Add {
                name: None,
                program: vec![],
            }
            .execute(path),
        }
    }

    fn remove(self, path: &Path, name: String) -> anyhow::Result<()> {
        if Confirm::new()
            .with_prompt(format!("Remove '{}'?", clean(&name)))
            .default(false)
            .interact_opt()?
            != Some(true)
        {
            return Ok(());
        }
        match self {
            Self::Profiles => ProfileCommand::Remove { name }.execute(path),
            Self::Skills => skills::Command::Remove { name }.execute(path),
            Self::Mcp => mcp::Command::Remove { name }.execute(path),
        }
    }
}

struct Row {
    name: OsString,
    details: String,
}

enum Action {
    Add,
    Run(OsString),
    Rename(OsString),
    Remove(OsString),
}

pub(super) fn run(path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal() && io::stderr().is_terminal(),
        "Run 'acli tui' in an interactive terminal"
    );
    // The foreground agent also receives Ctrl+C. Keep this process alive to reopen the menu.
    ctrlc::set_handler(|| {})?;

    let mut tab = 0;
    let mut selection = ListState::default();
    let mut notice = String::new();
    while let Some(action) = choose(path, &mut tab, &mut selection, &notice)? {
        notice = match perform(path, Page::TABS[tab].0, action) {
            Ok(message) => message,
            Err(error)
                if matches!(
                    error.downcast_ref::<dialoguer::Error>(),
                    Some(dialoguer::Error::IO(error)) if error.kind() == io::ErrorKind::Interrupted
                ) =>
            {
                String::new()
            }
            Err(error) => format!("{error:#}"),
        };
    }
    Ok(())
}

fn perform(path: &Path, page: Page, action: Action) -> anyhow::Result<String> {
    let name = |name: OsString| {
        name.into_string()
            .map_err(|_| anyhow::anyhow!("This name contains invalid UTF-8"))
    };
    match action {
        Action::Add => page.add(path)?,
        Action::Run(selected) => {
            let name = name(selected)?;
            let status = run_agent::wait(path, &name)?;
            return Ok(format!("{name}: {status}"));
        }
        Action::Rename(selected) => {
            let name = name(selected)?;
            let new_name: String = Input::new()
                .with_prompt("New profile name")
                .interact_text()?;
            ProfileCommand::Rename {
                name,
                new_name: new_name.trim().into(),
            }
            .execute(path)?;
        }
        Action::Remove(selected) => page.remove(path, name(selected)?)?,
    }
    Ok(String::new())
}

fn choose(
    path: &Path,
    tab: &mut usize,
    selection: &mut ListState,
    notice: &str,
) -> anyhow::Result<Option<Action>> {
    let mut screen = Screen(Terminal::new(CrosstermBackend::new(io::stdout()))?);
    enable_raw_mode()?;
    execute!(screen.0.backend_mut(), EnterAlternateScreen)?;
    let mut message = notice;
    loop {
        let page = Page::TABS[*tab].0;
        let (rows, status) = match page.rows(path) {
            Ok(rows) => (rows, clean(message)),
            Err(error) => (Vec::new(), clean(&format!("{error:#}"))),
        };
        selection.select(
            rows.len()
                .checked_sub(1)
                .map(|last| selection.selected().unwrap_or(0).min(last)),
        );
        let mut items: Vec<_> = rows
            .iter()
            .map(|row| {
                let name = clean(&row.name.to_string_lossy());
                if row.details.is_empty() {
                    name
                } else {
                    format!("{}    {}", name, clean(&row.details))
                }
            })
            .collect();
        if items.is_empty() {
            items.push("No entries".into());
        }
        let hints = if page == Page::Profiles {
            "Tab: switch  ↑↓: select  Enter: run  a: add  r: rename  d: remove  q: quit"
        } else {
            "Tab: switch  ↑↓: select  a: add  d: remove  q: quit"
        };
        loop {
            screen.0.draw(|frame| {
                let [tabs, body, help, footer] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(2),
                    Constraint::Length(2),
                ])
                .areas(frame.area());
                frame.render_widget(
                    Tabs::new(Page::TABS.map(|(_, title)| title))
                        .select(*tab)
                        .highlight_style(Color::Cyan),
                    tabs,
                );
                let list = List::new(items.iter().map(String::as_str))
                    .block(Block::bordered())
                    .highlight_symbol("> ")
                    .highlight_style(Color::Cyan);
                frame.render_stateful_widget(list, body, selection);
                frame.render_widget(Paragraph::new(hints).dim().wrap(Wrap { trim: true }), help);
                frame.render_widget(
                    Paragraph::new(status.as_str()).wrap(Wrap { trim: true }),
                    footer,
                );
            })?;

            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                return Ok(None);
            }
            if key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                continue;
            }
            let action: fn(OsString) -> Action = match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                KeyCode::Tab | KeyCode::BackTab => {
                    let count = Page::TABS.len();
                    let step = if key.code == KeyCode::Tab {
                        1
                    } else {
                        count - 1
                    };
                    *tab = (*tab + step) % count;
                    *selection = ListState::default();
                    message = "";
                    break;
                }
                KeyCode::Up if !rows.is_empty() => {
                    selection.select_previous();
                    continue;
                }
                KeyCode::Down if !rows.is_empty() => {
                    selection.select_next();
                    continue;
                }
                KeyCode::Char('a') => return Ok(Some(Action::Add)),
                KeyCode::Char('d') => Action::Remove,
                KeyCode::Enter if page == Page::Profiles => Action::Run,
                KeyCode::Char('r') if page == Page::Profiles => Action::Rename,
                _ => continue,
            };
            if let Some(row) = selection.selected().and_then(|index| rows.get(index)) {
                return Ok(Some(action(row.name.clone())));
            }
        }
    }
}

fn clean(text: &str) -> String {
    text.replace(char::is_control, " ")
}

struct Screen(DefaultTerminal);

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = self.0.show_cursor();
        let _ = ratatui::try_restore();
    }
}
