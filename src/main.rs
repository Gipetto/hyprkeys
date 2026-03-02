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

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Theme {
    media: Color,
    workspace: Color,
    window: Color,
    exec: Color,
    other: Color,
    /// Dispatcher name
    dispatcher: Color,
    /// Dim text: counts, path in title bar
    dim: Color,
    /// "search:" / "copied:" labels
    label: Color,
    /// "copied:" label specifically
    copied: Color,
    /// List selection highlight background
    highlight_bg: Color,
}

impl Theme {
    fn dark() -> Self {
        Theme {
            media: Color::Magenta,
            workspace: Color::Blue,
            window: Color::Cyan,
            exec: Color::Green,
            other: Color::White,
            dispatcher: Color::Yellow,
            dim: Color::DarkGray,
            label: Color::White,
            copied: Color::Green,
            highlight_bg: Color::DarkGray,
        }
    }

    fn light() -> Self {
        Theme {
            media: Color::Magenta,
            workspace: Color::LightRed,
            window: Color::Cyan,
            exec: Color::Green,
            other: Color::Reset,
            dispatcher: Color::Yellow,
            dim: Color::Indexed(247),
            label: Color::Black,
            copied: Color::Green,
            highlight_bg: Color::Gray,
        }
    }
}

// ---------------------------------------------------------------------------
// Category
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Category {
    Media,
    Workspace,
    Window,
    Exec,
    Other,
}

// ---------------------------------------------------------------------------
// Bind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Bind {
    modifiers: String,
    key: String,
    dispatcher: String,
    arg: String,
    arg_display: String,
}

impl Bind {
    fn category(&self) -> Category {
        let key_lc = self.key.to_lowercase();
        let disp_lc = self.dispatcher.to_lowercase();

        if key_lc.starts_with("xf86audio") || key_lc.starts_with("xf86monbright") {
            return Category::Media;
        }
        if disp_lc == "workspace"
            || disp_lc == "movetoworkspace"
            || disp_lc == "movetoworkspacesilent"
        {
            return Category::Workspace;
        }
        if disp_lc.contains("window")
            || matches!(
                disp_lc.as_str(),
                "movefocus"
                    | "movewindow"
                    | "resizeactive"
                    | "swapwindow"
                    | "togglefloating"
                    | "fullscreen"
                    | "pseudo"
                    | "togglesplit"
                    | "killactive"
            )
        {
            return Category::Window;
        }
        if disp_lc == "exec" || disp_lc == "exec-once" {
            return Category::Exec;
        }
        Category::Other
    }

    fn category_color(&self, theme: &Theme) -> Color {
        match self.category() {
            Category::Media => theme.media,
            Category::Workspace => theme.workspace,
            Category::Window => theme.window,
            Category::Exec => theme.exec,
            Category::Other => theme.other,
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        std::process::exit(0);
    }

    // --config / -c
    let path = if let Some(pos) = args.iter().position(|a| a == "--config" || a == "-c") {
        args.get(pos + 1).cloned().unwrap_or_else(|| {
            eprintln!("--config requires a path argument");
            std::process::exit(1);
        })
    } else {
        let home = env::var("HOME").unwrap_or_else(|_| {
            eprintln!("HOME not set");
            std::process::exit(1);
        });
        format!("{}/.config/hypr/hyprland.conf", home)
    };

    let theme = if let Some(pos) = args.iter().position(|a| a == "--theme" || a == "-t") {
        match args.get(pos + 1).map(|s| s.as_str()) {
            Some("light") => Theme::light(),
            Some("dark") | None => Theme::dark(),
            Some(other) => {
                eprintln!("Unknown theme '{}'. Valid options: dark, light", other);
                std::process::exit(1);
            }
        }
    } else {
        Theme::dark()
    };

    let content = load_config(&path);
    let binds = parse_binds(&content);

    run_tui(binds, &path, theme).unwrap();
}

fn print_help() {
    println!("hyprkeys - Hyprland keybinding lookup tool");
    println!();
    println!("USAGE:");
    println!("  hyprkeys [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("  -c, --config <path>        Use a custom hyprland config file");
    println!("  -t, --theme  <dark|light>  Color theme (default: dark)");
    println!();
    println!("CONTROLS:");
    println!("  Type          Filter bindings by fuzzy search");
    println!("  Up/Down       Navigate results");
    println!("  Ctrl+U        Clear search query");
    println!("  :q or Esc     Quit");
    println!();
    println!("EXAMPLES:");
    println!("  hyprkeys");
    println!("  hyprkeys --theme light");
    println!("  hyprkeys --config ~/.config/hypr/binds.conf --theme light");
}

// ---------------------------------------------------------------------------
// Config loading (follows `source` directives)
// ---------------------------------------------------------------------------

fn load_config(path: &str) -> String {
    load_config_inner(path, 0)
}

fn load_config_inner(path: &str, depth: u8) -> String {
    if depth > 16 {
        return String::new();
    }

    let expanded = expand_tilde(path);
    let content = match fs::read_to_string(&expanded) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", expanded, e);
            return String::new();
        }
    };

    let mut result = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rhs) = trimmed
            .strip_prefix("source")
            .and_then(|s| s.trim_start().strip_prefix('='))
        {
            result.push_str(&load_config_inner(rhs.trim(), depth + 1));
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        let home = env::var("HOME").unwrap_or_default();
        format!("{}/{}", home, &path[2..])
    } else {
        path.to_string()
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

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
                vars.insert(format!("${}", parts[0].trim()), parts[1].trim().to_string());
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

// ---------------------------------------------------------------------------
// TUI
// ---------------------------------------------------------------------------

fn run_tui(binds: Vec<Bind>, path: &str, theme: Theme) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let matcher = SkimMatcherV2::default();
    let mut query = String::new();
    let mut list_state = ListState::default();
    list_state.select(Some(0));
    let mut copied: Option<(String, std::time::Instant)> = None;

    loop {
        let filtered: Vec<&Bind> = if query.is_empty() {
            binds.iter().collect()
        } else {
            let mut scored: Vec<(i64, &Bind)> = binds
                .iter()
                .filter_map(|b| matcher.fuzzy_match(&format_bind(b), &query).map(|s| (s, b)))
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            scored.into_iter().map(|(_, b)| b).collect()
        };

        let selected = list_state.selected().unwrap_or(0);
        if selected >= filtered.len() && !filtered.is_empty() {
            list_state.select(Some(filtered.len() - 1));
        }

        if let Some((_, t)) = &copied {
            if t.elapsed() > std::time::Duration::from_secs(2) {
                copied = None;
            }
        }

        let result_count = filtered.len();
        let total_count = binds.len();

        terminal.draw(|f| {
            let area = f.area();

            let outer = Block::default()
                .borders(Borders::ALL)
                .title(Line::from(vec![
                    Span::styled(" hyprkeys ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(format!("─ {} ", path), Style::default().fg(theme.dim)),
                ]));
            let inner_area = outer.inner(area);
            f.render_widget(outer, area);

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(inner_area);

            // Search bar
            let search_line = if let Some(ref c) = copied {
                Line::from(vec![
                    Span::styled(
                        "copied: ",
                        Style::default()
                            .fg(theme.copied)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(c.0.as_str(), Style::default().fg(theme.label)),
                ])
            } else if query.is_empty() {
                Line::from(vec![
                    Span::styled(
                        "search: ",
                        Style::default()
                            .fg(theme.label)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("({} bindings)", total_count),
                        Style::default().fg(theme.dim),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled(
                        "search: ",
                        Style::default()
                            .fg(theme.label)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(query.as_str(), Style::default().fg(theme.label)),
                    Span::styled(
                        format!("  ({result_count})"),
                        Style::default().fg(theme.dim),
                    ),
                ])
            };
            f.render_widget(
                Paragraph::new(search_line).block(Block::default().borders(Borders::ALL)),
                chunks[0],
            );

            // Binding list
            let items: Vec<ListItem> = filtered
                .iter()
                .map(|b| {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:<35}", format_combo(b)),
                            Style::default()
                                .fg(b.category_color(&theme))
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(&b.dispatcher, Style::default().fg(theme.dispatcher)),
                        Span::raw(" "),
                        Span::styled(&b.arg_display, Style::default().fg(theme.other)),
                    ]))
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL))
                .highlight_style(
                    Style::default()
                        .bg(theme.highlight_bg)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("▶ ");

            f.render_stateful_widget(list, chunks[1], &mut list_state);
        })?;

        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => break,
                    (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                        query.clear();
                        copied = None;
                        list_state.select(Some(0));
                    }
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
