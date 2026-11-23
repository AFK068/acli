use crate::config::{AuthMode, Config, Profile, storage};
use ratatui::{
    layout::Constraint,
    style::{Color, Stylize},
    widgets::{Cell, Row, Table, Widget},
};
use std::io::{self, IsTerminal, Write};

const TABLE_WIDTH: u16 = 88;
const ROWS_PER_BLOCK: usize = 128;

pub(super) fn run(path: &std::path::Path) -> anyhow::Result<()> {
    let config = storage::load(path)?;
    print(&config)?;
    if config.profiles.is_empty() {
        eprintln!("Add your first profile with: acli profile add");
    }
    Ok(())
}

fn print(config: &Config) -> io::Result<()> {
    let mut output = io::stdout().lock();

    if !output.is_terminal() {
        for (name, profile) in &config.profiles {
            writeln!(output, "{name}\t{}\t{}", profile.agent, profile.auth)?;
        }
        return Ok(());
    }

    let mut terminal = ratatui::Terminal::with_options(
        ratatui::backend::CrosstermBackend::new(output),
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(1),
        },
    )?;
    render(&mut terminal, config)?;

    let area = terminal.get_frame().area();
    terminal.set_cursor_position((0, area.y))?;
    terminal.show_cursor()?;
    ratatui::backend::Backend::flush(terminal.backend_mut())
}

fn render<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    config: &Config,
) -> Result<(), B::Error> {
    let title = ratatui::text::Line::from(vec![
        " acli ".fg(ratatui::style::Color::Rgb(0, 170, 140)).bold(),
        ratatui::text::Span::raw(format!("· profiles ({}) ", config.profiles.len())),
    ]);
    let block = ratatui::widgets::Block::bordered()
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(ratatui::style::Color::DarkGray)
        .padding(ratatui::widgets::Padding::horizontal(1))
        .title(title);

    if config.profiles.is_empty() {
        return insert_widget(
            terminal,
            3,
            ratatui::widgets::Paragraph::new("No profiles configured.").block(block),
        );
    }

    let profiles: Vec<_> = config.profiles.iter().collect();
    // Bound each render buffer while allowing long lists to flow into scrollback.
    for chunk in profiles.chunks(ROWS_PER_BLOCK) {
        let rows = chunk.iter().map(|(name, profile)| {
            let is_default = config.default_profile.as_deref() == Some(name.as_str());
            profile_row(name.to_string(), profile, is_default)
        });
        insert_widget(
            terminal,
            chunk.len() as u16 + 4,
            profile_table(rows).block(block.clone()),
        )?;
    }

    Ok(())
}

fn insert_widget<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
    height: u16,
    widget: impl Widget,
) -> Result<(), B::Error> {
    terminal.insert_before(height, |buffer| {
        let mut area = buffer.area;
        area.width = area.width.min(TABLE_WIDTH);
        widget.render(area, buffer);
    })
}

pub(super) fn profile_table(rows: impl IntoIterator<Item = Row<'static>>) -> Table<'static> {
    Table::new(
        rows,
        [
            Constraint::Fill(3),
            Constraint::Fill(2),
            Constraint::Length(6),
            Constraint::Length(9),
        ],
    )
    .header(
        Row::new(["Profile", "Agent", "Auth", ""])
            .dim()
            .bottom_margin(1),
    )
    .column_spacing(2)
}

pub(super) fn profile_row(name: String, profile: &Profile, is_default: bool) -> Row<'static> {
    let default = if is_default {
        Cell::from(" Default ").fg(Color::Black).bg(Color::Cyan)
    } else {
        Cell::default()
    };
    let auth = Cell::from(profile.auth.to_string());
    let auth = match profile.auth {
        AuthMode::Native => auth,
        AuthMode::Api => auth.fg(Color::Cyan),
    };
    Row::new([
        Cell::from(name).bold(),
        Cell::from(profile.agent.to_string()),
        auth,
        default,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Agent;

    fn render_profiles(config: &Config) -> anyhow::Result<String> {
        let mut terminal = ratatui::Terminal::with_options(
            ratatui::backend::TestBackend::new(80, 10),
            ratatui::TerminalOptions {
                viewport: ratatui::Viewport::Inline(1),
            },
        )?;
        render(&mut terminal, config)?;
        let backend = terminal.backend();
        Ok(buffer_text(backend.scrollback()) + &buffer_text(backend.buffer()))
    }

    #[test]
    fn keeps_long_lists_in_terminal_scrollback() -> anyhow::Result<()> {
        let mut config = Config::default();
        for index in 0..160 {
            config.profiles.insert(
                format!("profile-{index:03}"),
                Profile::new(Agent::Codex, AuthMode::Native),
            );
        }
        let output = render_profiles(&config)?;
        for heading in ["Profile", "Agent", "Auth"] {
            assert!(output.contains(heading));
        }
        for name in config.profiles.keys() {
            assert!(output.contains(name), "missing profile {name}");
        }
        Ok(())
    }

    #[test]
    fn renders_empty_profile_list() -> anyhow::Result<()> {
        let config = Config::default();
        assert!(render_profiles(&config)?.contains("No profiles configured."));
        Ok(())
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        buffer.content.iter().map(|cell| cell.symbol()).collect()
    }
}
