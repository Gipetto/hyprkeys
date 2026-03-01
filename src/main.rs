use std::fs;
use std::env;
use comfy_table::{
    Attribute,
    Cell,
    ContentArrangement,
    Table, 
    presets::UTF8_FULL, 
    modifiers::UTF8_ROUND_CORNERS,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("hyprkeys - Hyprland keybinding lookup tool");
        println!();
        println!("USAGE:");
        println!("  hyprkeys [OPTIONS]");
        println!();
        println!("OPTIONS:");
        println!("  --binds <types>    Comma-separated list of bind types to show");
        println!("                     Types: bind, bindel, bindl, bindm");
        println!("                     Default: all types");
        println!();
        println!("EXAMPLES:");
        println!("  hyprkeys");
        println!("  hyprkeys --binds bind");
        println!("  hyprkeys --binds bind,bindel");
        println!("  hyprkeys | fzf");
        std::process::exit(0);
    }

    let path = format!("{}/.config/hypr/hyprland.conf", env!("HOME"));

    let content = fs::read_to_string(&path)
        .unwrap_or_else(|e| { eprintln!("Error reading {}: {}", path, e); std::process::exit(1); });

    let filter = parse_args(&args);
    let binds = parse_binds(&content, &filter);
    print_table(&binds);
}

fn parse_args(args: &[String]) -> Vec<String> {    
    for i in 0..args.len() {
        if args[i] == "--binds" {
            if let Some(val) = args.get(i + 1) {
                return val.split(',').map(|s| s.trim().to_string()).collect();
            }
        }
    }

    vec![
        "bind".to_string(), 
        "bindel".to_string(), 
        "bindl".to_string(), 
        "bindm".to_string()
    ]
}

#[derive(Debug)]
struct Bind {
    bind_type: String,
    modifiers: String,
    key: String,
    dispatcher: String,
    arg: String,
}

fn parse_binds(content: &str, filter: &[String]) -> Vec<Bind> {
    let mut results = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        let bind_type = if line.starts_with("bindel") { "bindel" }
            else if line.starts_with("bindl")  { "bindl"  }
            else if line.starts_with("bindm")  { "bindm"  }
            else if line.starts_with("bind ")  { "bind"   }
            else if line.starts_with("bind=")  { "bind"   }
            else { continue };

        if !filter.iter().any(|f| f == bind_type) {
            continue;
        }

        let Some(rhs) = line.splitn(2, '=').nth(1) else { continue };

        let parts: Vec<&str> = rhs.splitn(4, ',').map(|p| p.trim()).collect();
        if parts.len() < 3 { continue; }

        results.push(Bind {
            bind_type: bind_type.to_string(),
            modifiers: parts[0].to_string(),
            key:       parts[1].to_string(),
            dispatcher: parts[2].to_string(),
            arg: if parts.len() > 3 { parts[3].to_string() } else { String::new() },
        });
    }

    results
}

fn format_combo(bind: &Bind) -> String {
    let mods = expand_modifiers(&bind.modifiers);
    if mods.is_empty() {
        bind.key.clone()
    } else {
        format!("{} + {}", mods, bind.key)
    }
}

fn expand_modifiers(mods: &str) -> String {
    mods.replace("$mainMod", "SUPER")
        .replace(" SHIFT", " + SHIFT")
        .replace(" ALT", " + ALT")
        .replace(" CTRL", " + CTRL")
}

fn print_table(binds: &[Bind]) {
    if binds.is_empty() {
        println!("No binds found");
        return;
    }

    let combos: Vec<String> = binds.iter().map(format_combo).collect();

    let mut indexed: Vec<(usize, &Bind)> = binds.iter().enumerate().collect();
    indexed.sort_by(|a, b| combos[a.0].cmp(&combos[b.0]));

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["COMBO", "TYPE", "ACTION", "ARG"]);

    for (i, bind) in indexed {
        table.add_row(vec![
            Cell::new(&combos[i])
                .add_attribute(Attribute::Bold),
            Cell::new(&bind.bind_type),
            Cell::new(&bind.dispatcher),
            Cell::new(&bind.arg),
        ]);
        
    }

    println!("{table}");
}
