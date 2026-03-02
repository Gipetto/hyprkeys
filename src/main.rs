use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{self, Event, KeyCode, KeyModifiers},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        std::process::exit(0);
    }

    let home = env::var("HOME").unwrap_or_else(|_| {
        eprintln!("HOME not set");
        std::process::exit(1);
    });

    let path = format!("{}/.config/hypr/hyprland.conf", home);
    let content = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", path, e);
        std::process::exit(1);
    });

    let binds = parse_binds(&content);

    run_tui(binds, &path).unwrap();
}

fn print_help() {
    println!("hyprkeys - Hyprland keybinding lookup tool");
    println!();
    println!("USAGE:");
    println!("  hyprkeys");
    println!();
    println!("CONTROLS:");
    println!("  Type          Filter bindings by fuzzy search");
    println!("  Up/Down       Navigate results");
    println!("  :q or Esc     Quit");
    println!();
    println!("EXAMPLES:");
    println!("  hyprkeys");
    println!("  hyprkeys --help");
}

#[derive(Debug, Clone)]
struct Bind {
    modifiers: String,
    key: String,
    dispatcher: String,
    arg: String,         // plain expanded, for copying
    arg_display: String, // with # comment, for display
}

fn parse_binds(content: &str) -> Vec<Bind> {
    let vars = parse_variables(content);
    let mut results = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        let is_bind = line.starts_with("bindel")
            || line.starts_with("bindl")
            || line.starts_with("bindm")
            || line.starts_with("bind ")
            || line.starts_with("bind=");

        if !is_bind {
            continue;
        }
        let Some(rhs) = line.splitn(2, '=').nth(1) else {
            continue;
        };
        let parts: Vec<&str> = rhs.splitn(4, ',').map(|p| p.trim()).collect();
        if parts.len() < 3 {
            continue;
        }

        results.push(Bind {
            modifiers: expand_variables(parts[0], &vars),
            key: expand_variables(parts[1], &vars),
            dispatcher: parts[2].to_string(),
            arg: if parts.len() > 3 {
                expand_variables(parts[3], &vars)
            } else {
                String::new()
            },
            arg_display: if parts.len() > 3 {
                expand_variables_display(parts[3], &vars)
            } else {
                String::new()
            },
        });
    }

    results.sort_by(|a, b| format_combo(a).cmp(&format_combo(b)));
    results
}

fn parse_variables(content: &str) -> std::collections::HashMap<String, String> {
    let mut vars = std::collections::HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if let Some(rhs) = line.strip_prefix('$') {
            let parts: Vec<&str> = rhs.splitn(2, '=').collect();
            if parts.len() == 2 {
                let key = format!("${}", parts[0].trim());
                let val = parts[1].trim().to_string();
                vars.insert(key, val);
            }
        }
    }

    vars
}

fn expand_variables(s: &str, vars: &std::collections::HashMap<String, String>) -> String {
    let mut result = s.to_string();
    for (key, val) in vars {
        if key == "$mainMod" {
            continue;
        }
        if result.contains(key.as_str()) {
            result = result.replace(key.as_str(), val);
        }
    }
    result
}

fn expand_variables_display(s: &str, vars: &std::collections::HashMap<String, String>) -> String {
    let mut result = s.to_string();
    for (key, val) in vars {
        if key == "$mainMod" {
            continue;
        }
        if result.contains(key.as_str()) {
            result = result.replace(key.as_str(), &format!("{} # {}", val, key));
        }
    }
    result
}

fn friendly_key(key: &str) -> &str {
    match key {
        "XF86AudioRaiseVolume" => "Volume Up",
        "XF86AudioLowerVolume" => "Volume Down",
        "XF86AudioMute" => "Mute",
        "XF86AudioMicMute" => "Mic Mute",
        "XF86AudioPlay" => "Play/Pause",
        "XF86AudioPause" => "Pause",
        "XF86AudioNext" => "Next Track",
        "XF86AudioPrev" => "Prev Track",
        "XF86AudioStop" => "Stop",
        other => other,
    }
}

fn format_combo(bind: &Bind) -> String {
    let mods = bind
        .modifiers
        .replace("$mainMod", "SUPER")
        .replace(" SHIFT", " + SHIFT")
        .replace(" ALT", " + ALT")
        .replace(" CTRL", " + CTRL");
    let key = friendly_key(&bind.key);
    if mods.is_empty() {
        key.to_string()
    } else {
        format!("{} + {}", mods, key)
    }
}

fn format_bind(bind: &Bind) -> String {
    let combo = format_combo(bind);
    format!(
        "{:<35} {}",
        combo,
        if bind.arg_display.is_empty() {
            bind.dispatcher.clone()
        } else {
            format!("{} {}", bind.dispatcher, bind.arg_display)
        }
    )
}

fn run_tui(binds: Vec<Bind>, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let matcher = SkimMatcherV2::default();
    let mut query = String::new();
    let mut list_state = ListState::default();
    list_state.select(Some(0));
    let mut copied: Option<(String, std::time::Instant)> = None;

    loop {
        let filtered: Vec<&Bind> = if query.is_empty() {
            binds.iter().collect() // already sorted by combo from parse_binds
        } else {
            let mut scored: Vec<(i64, &Bind)> = binds
                .iter()
                .filter_map(|b| {
                    let text = format_bind(b);
                    matcher.fuzzy_match(&text, &query).map(|score| (score, b))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            scored.into_iter().map(|(_, b)| b).collect()
        };

        let selected = list_state.selected().unwrap_or(0);
        if selected >= filtered.len() && !filtered.is_empty() {
            list_state.select(Some(filtered.len() - 1));
        }

        if let Some((_, time)) = &copied {
            if time.elapsed() > std::time::Duration::from_secs(2) {
                copied = None;
            }
        }

        terminal.draw(|f| {
            let area = f.area();

            let outer = Block::default()
                .borders(Borders::ALL)
                .title(Line::from(vec![
                    Span::styled(" hyprkeys ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(format!("─ {} ", path), Style::default().fg(Color::DarkGray)),
                ]));

            let inner_area = outer.inner(area);
            f.render_widget(outer, area);

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(inner_area);

            let search_text = if let Some(ref c) = copied {
                Line::from(vec![
                    Span::styled(
                        "copied: ",
                        Style::default()
                            .add_modifier(Modifier::BOLD)
                            .fg(Color::Green),
                    ),
                    Span::raw(c.0.as_str()),
                ])
            } else {
                Line::from(vec![
                    Span::styled("search: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(query.as_str()),
                ])
            };

            let search = Paragraph::new(search_text).block(Block::default().borders(Borders::ALL));

            f.render_widget(search, chunks[0]);

            let items: Vec<ListItem> = filtered
                .iter()
                .map(|b| {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:<35}", format_combo(b)),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(&b.dispatcher, Style::default().fg(Color::Yellow)),
                        Span::raw(" "),
                        Span::raw(&b.arg_display),
                    ]))
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ");

            f.render_stateful_widget(list, chunks[1], &mut list_state);
        })?;

        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => break,
                    (KeyCode::Char(c), _) => {
                        copied = None;
                        query.push(c);
                        if query == ":q" {
                            break;
                        }
                        list_state.select(Some(0));
                    }
                    (KeyCode::Backspace, _) => {
                        copied = None;
                        query.pop();
                        list_state.select(Some(0));
                    }
                    (KeyCode::Down, _) => {
                        let i = list_state.selected().unwrap_or(0);
                        if i + 1 < filtered.len() {
                            list_state.select(Some(i + 1));
                        }
                    }
                    (KeyCode::Up, _) => {
                        let i = list_state.selected().unwrap_or(0);
                        if i > 0 {
                            list_state.select(Some(i - 1));
                        }
                    }
                    (KeyCode::Enter, _) => {
                        if let Some(i) = list_state.selected() {
                            if let Some(bind) = filtered.get(i) {
                                let command = if bind.arg.is_empty() {
                                    format!("hyprctl dispatch {}", bind.dispatcher)
                                } else {
                                    format!("hyprctl dispatch {} {}", bind.dispatcher, bind.arg)
                                };
                                let _ = std::process::Command::new("wl-copy").arg(&command).spawn();
                                copied = Some((command, std::time::Instant::now()));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
