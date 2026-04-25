use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

pub struct Printer {
    pub format: OutputFormat,
    pub color: bool,
}

impl Printer {
    pub fn new(json: bool, no_color: bool) -> Self {
        let color = !no_color && atty::is(atty::Stream::Stdout);
        Self {
            format: if json {
                OutputFormat::Json
            } else {
                OutputFormat::Text
            },
            color,
        }
    }

    pub fn line(&self, msg: &str) {
        println!("{msg}");
    }

    pub fn json(&self, value: &Value) {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    }

    pub fn warn(&self, msg: &str) {
        eprintln!("warning: {msg}");
    }

    pub fn error(&self, msg: &str) {
        eprintln!("error: {msg}");
    }

    pub fn table(&self, headers: &[&str], rows: Vec<Vec<String>>) {
        if rows.is_empty() {
            println!("(none)");
            return;
        }
        let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        let header_line: String = headers
            .iter()
            .zip(&widths)
            .map(|(h, w)| format!("{:<width$}", h, width = w))
            .collect::<Vec<_>>()
            .join("  ");

        let sep = "─".repeat(header_line.len());

        if self.color {
            println!("\x1b[1m{header_line}\x1b[0m");
        } else {
            println!("{header_line}");
        }
        println!("{sep}");

        for row in rows {
            let cells: String = row
                .iter()
                .zip(&widths)
                .map(|(c, w)| format!("{:<width$}", c, width = w))
                .collect::<Vec<_>>()
                .join("  ");
            println!("{cells}");
        }
    }

    pub fn kv(&self, pairs: &[(&str, &str)]) {
        let key_width = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (k, v) in pairs {
            if self.color {
                println!("\x1b[1m{:<width$}\x1b[0m  {}", k, v, width = key_width);
            } else {
                println!("{:<width$}  {}", k, v, width = key_width);
            }
        }
    }

    pub fn status_line(&self, ok: bool, label: &str, detail: &str) {
        let icon = if ok { "✓" } else { "✗" };
        if self.color {
            let color = if ok { "\x1b[32m" } else { "\x1b[31m" };
            println!("{color}[{icon}]\x1b[0m {label}  {detail}");
        } else {
            println!("[{icon}] {label}  {detail}");
        }
    }

    pub fn separator(&self) {
        println!("{}", "─".repeat(40));
    }
}
