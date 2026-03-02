# Hyprkeys

I'm always forgetting my keybindings. This lets me look them up without alt-tabbing to a config file like some kind of animal.

## What is this?

Hyprkeys is a small terminal UI that parses your Hyprland config and presents your keybindings in a searchable, color-coded table. It follows `source` directives, so split configs work too.

## Installation
```bash
cargo install --path .
```

## Usage
```bash
hyprkeys [OPTIONS]
```

### Options

| Flag | Description |
|------|-------------|
| `-c, --config <path>` | Use a custom config file instead of `~/.config/hypr/hyprland.conf` |
| `-t, --theme <dark\|light>` | Color theme (default: `dark`) |
| `-h, --help` | Print help and exit |

### Controls

| Key | Action |
|-----|--------|
| Type anything | Fuzzy search bindings |
| `↑ / ↓` | Navigate results |
| `Enter` | Copy the selected binding as a `hyprctl dispatch` command |
| `Ctrl+U` | Clear search query |
| `Esc` or `:q` | Quit |

### Examples
```bash
# Default — dark theme, standard config path
hyprkeys

# Light terminal theme
hyprkeys --theme light

# Custom config path
hyprkeys --config ~/.config/hypr/binds.conf

# Both
hyprkeys --config ~/.config/hypr/binds.conf --theme light
```

## Color coding

Bindings are color-coded by category so you can find things at a glance:

| Color | Category |
|-------|----------|
| Cyan | Window management |
| Blue | Workspace switching |
| Green | Exec / launch |
| Magenta | Media keys |
| White | Everything else |

## Notes

- Pressing `Enter` on a binding copies a `hyprctl dispatch ...` command to the clipboard via `wl-copy`. Useful for testing bindings without reloading your config.
- `$mainMod` is left as-is in the output since that's what it actually says in your config and you know what it means.
- Variables other than `$mainMod` are expanded inline, with the original variable name shown as a comment (e.g. `kitty # $terminal`).
