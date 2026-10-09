use crate::types;
use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::sync::Mutex;
use std::time::Duration;

/// A dependency as it is listed in Cargo.toml
pub struct Row {
    pub name: String,

    /// The section the dependency is listed in, e.g. `dev-dependencies`
    pub section: String,

    /// The version requirement, e.g. `^1.0`
    pub req: String,
}

/// Shows a row per dependency, with a spinner until its check is done.
///
/// Rows are checked in parallel, but printed for good in their original order:
/// a row leaves the live area once every row above it is done too. The live
/// area is cut off at the terminal height, so this also keeps the output
/// complete when there are more rows than fit on the screen.
pub struct Report {
    multi: MultiProgress,
    rows: Vec<Row>,
    bars: Vec<ProgressBar>,
    label_width: usize,
    req_width: usize,
    state: Mutex<State>,
}

struct State {
    /// The final line of each done row that is still in the live area
    lines: Vec<Option<String>>,

    /// How many rows, from the top, have left the live area
    printed: usize,
}

/// The section a row is listed in, unless it's the default one. The same crate
/// can be listed in several sections with different requirements.
fn section_label(row: &Row) -> String {
    match row.section.as_str() {
        "dependencies" => String::new(),
        section => format!(" [{}]", section),
    }
}

impl Report {
    pub fn new(rows: Vec<Row>) -> Report {
        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::stdout());

        let label_width = rows
            .iter()
            .map(|row| row.name.len() + section_label(row).len())
            .max()
            .unwrap_or(0);

        let req_width = rows.iter().map(|row| row.req.len()).max().unwrap_or(0);

        let mut report = Report {
            multi,
            label_width,
            req_width,
            bars: vec![],
            state: Mutex::new(State {
                lines: vec![None; rows.len()],
                printed: 0,
            }),
            rows,
        };

        let style = ProgressStyle::with_template(" {spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⢹", "⢺", "⢼", "⣸", "⣇", "⡧", "⡗", "⡏", "✔"]);

        report.bars = (0..report.rows.len())
            .map(|i| {
                let bar = report.multi.add(
                    ProgressBar::new_spinner()
                        .with_style(style.clone())
                        .with_message(report.columns(i, "")),
                );

                if !report.multi.is_hidden() {
                    bar.enable_steady_tick(Duration::from_millis(80));
                }

                bar
            })
            .collect();

        report
    }

    /// A row's columns after its icon, padded to line up with the other rows
    fn columns(&self, i: usize, status: &str) -> String {
        let row = &self.rows[i];
        let section = section_label(row);
        let padding = self.label_width - row.name.len() - section.len();

        format!(
            "{}{}{}  {:<req_width$}  {}",
            row.name.cyan(),
            section.dimmed(),
            " ".repeat(padding),
            row.req,
            status,
            req_width = self.req_width,
        )
    }

    /// Replace row `i`'s spinner with the outcome of its check
    pub fn finish(&self, i: usize, outcome: Result<&types::Crate, &str>) {
        let (icon, status) = match outcome {
            Ok(krate) if krate.update_available => (
                "\u{eacc}".yellow(),
                format!("{} {}", "->".dimmed(), krate.latest_version.green()),
            ),

            Ok(_) => ("✔".green(), "up to date".dimmed().to_string()),

            Err(e) => ("✖".red(), e.red().to_string()),
        };

        let line = format!(" {} {}", icon, self.columns(i, &status));

        let mut state = self.state.lock().unwrap();
        let state = &mut *state;

        self.bars[i].set_style(ProgressStyle::with_template("{msg}").unwrap());
        self.bars[i].finish_with_message(line.clone());
        state.lines[i] = Some(line);

        // Move the done rows at the top out of the live area, in order
        while let Some(line) = state.lines.get_mut(state.printed).and_then(Option::take) {
            self.multi.remove(&self.bars[state.printed]);

            // Print it as a plain line above the live area, which is redrawn
            // below it. This also works when nothing is drawn, like when
            // stdout isn't a terminal.
            self.multi.suspend(|| println!("{}", line));

            state.printed += 1;
        }
    }
}
