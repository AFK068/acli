use super::{
    ProfileCommand,
    list_profiles::{profile_row, profile_table},
    mcp, run_agent, skills,
};
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
    style::{Color, Modifier, Style, Stylize},
    text::Line,
    widgets::{Block, HighlightSpacing, Paragraph, Row, Table, TableState, Tabs, Wrap},
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

    fn entries(self, path: &Path) -> anyhow::Result<Vec<Entry>> {
        let rows = match self {
            Self::Profiles => {
                let config = storage::load(path)?;
                config
                    .profiles
                    .into_iter()
                    .map(|(name, profile)| {
                        let is_default = config.default_profile.as_deref() == Some(name.as_str());
                        Entry {
                            row: profile_row(clean(&name), &profile, is_default),
                            name: name.into(),
                            is_default,
                        }
                    })
                    .collect()
            }
            Self::Skills => skills::list(path)?
                .into_iter()
                .map(|name| Entry {
                    row: Row::new([clean(&name.to_string_lossy())]),
                    name,
                    is_default: false,
                })
                .collect(),
            Self::Mcp => mcp::list(path)?
                .into_iter()
                .map(|(name, server)| Entry {
                    row: Row::new([clean(&name), clean(&server.command)]),
                    name: name.into(),
                    is_default: false,
                })
                .collect(),
        };
        Ok(rows)
    }

    fn table(self, entries: &[Entry]) -> Table<'static> {
        let rows = entries.iter().map(|entry| entry.row.clone());
        match self {
            Self::Profiles => profile_table(rows),
            Self::Skills => Table::new(rows, [Constraint::Fill(1)]),
            Self::Mcp => Table::new(rows, [Constraint::Fill(2), Constraint::Fill(3)]),
        }
        .block(Block::bordered())
        .highlight_symbol(Line::from("› ".fg(Color::Cyan)))
        .highlight_spacing(HighlightSpacing::Always)
        .row_highlight_style(Style::new().add_modifier(Modifier::BOLD))
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

struct Entry {
    name: OsString,
    row: Row<'static>,
    is_default: bool,
}

enum Action {
    Add,
    Run(OsString),
    SetDefault(OsString),
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
    let mut selection = TableState::default();
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
        Action::SetDefault(selected) => {
            let name = name(selected)?;
            storage::set_default(path, Some(&name))?;
            return Ok(format!("Default profile set to '{name}'"));
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
    selection: &mut TableState,
    notice: &str,
) -> anyhow::Result<Option<Action>> {
    let mut screen = Screen(Terminal::new(CrosstermBackend::new(io::stdout()))?);
    enable_raw_mode()?;
    execute!(screen.0.backend_mut(), EnterAlternateScreen)?;
    let mut message = notice;
    loop {
        let page = Page::TABS[*tab].0;
        let (entries, status) = match page.entries(path) {
            Ok(entries) => (entries, clean(message)),
            Err(error) => (Vec::new(), clean(&format!("{error:#}"))),
        };
        let selected = selection
            .selected()
            .or_else(|| entries.iter().position(|entry| entry.is_default))
            .unwrap_or(0);
        selection.select(entries.len().checked_sub(1).map(|last| selected.min(last)));
        let table = page.table(&entries);
        let hints = if page == Page::Profiles {
            "Tab: switch  ↑↓: select  Enter: run  s: set default  a: add  r: rename  d: remove  q: quit"
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
                if entries.is_empty() {
                    frame
                        .render_widget(Paragraph::new("No entries").block(Block::bordered()), body);
                } else {
                    frame.render_stateful_widget(&table, body, selection);
                }
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
            let selected = selection.selected().and_then(|index| entries.get(index));
            match (key.code, selected) {
                (KeyCode::Char('q') | KeyCode::Esc, _) => return Ok(None),
                (KeyCode::Tab | KeyCode::BackTab, _) => {
                    let count = Page::TABS.len();
                    let step = if key.code == KeyCode::Tab {
                        1
                    } else {
                        count - 1
                    };
                    *tab = (*tab + step) % count;
                    *selection = TableState::default();
                    message = "";
                    break;
                }
                (KeyCode::Up, Some(_)) => selection.select_previous(),
                (KeyCode::Down, Some(_)) => selection.select_next(),
                (KeyCode::Char('a'), _) => return Ok(Some(Action::Add)),
                (KeyCode::Char('d'), Some(row)) => {
                    return Ok(Some(Action::Remove(row.name.clone())));
                }
                (KeyCode::Enter, Some(row)) if page == Page::Profiles => {
                    return Ok(Some(Action::Run(row.name.clone())));
                }
                (KeyCode::Char('s'), Some(row)) if page == Page::Profiles => {
                    return Ok(Some(Action::SetDefault(row.name.clone())));
                }
                (KeyCode::Char('r'), Some(row)) if page == Page::Profiles => {
                    return Ok(Some(Action::Rename(row.name.clone())));
                }
                _ => {}
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
