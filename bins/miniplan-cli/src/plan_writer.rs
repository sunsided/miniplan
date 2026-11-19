//! Serialization of a found [`Plan`] to stdout or a file, shared by `solve` and the TUI.

use anyhow::{Context, Result};
use clap::builder::PossibleValue;
use miniplan::plan::Plan;

/// The text format a plan is written in.
#[derive(Clone, Debug)]
pub enum OutputFormat {
    /// The plan's `Display` form: `; cost` and `; length` comment lines, then one `(step)`
    /// per line.
    Plain,
    /// IPC-style output; currently the same text as [`OutputFormat::Plain`].
    Ipc,
    /// A pretty-printed JSON object with `cost`, `length` and the list of step names.
    Json,
}

/// Implemented by hand rather than derived: the derive would turn the variant docs above into
/// `--help` text.
impl clap::ValueEnum for OutputFormat {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Plain, Self::Ipc, Self::Json]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(match self {
            Self::Plain => "plain",
            Self::Ipc => "ipc",
            Self::Json => "json",
        }))
    }
}

/// Writes `plan` in `format` to `output`, where `"-"` means stdout and anything else a path.
///
/// An existing file at `output` is overwritten.
///
/// # Errors
///
/// Returns an error when JSON serialization fails or the file cannot be written.
pub fn write_plan(plan: &Plan, format: &OutputFormat, output: &str) -> Result<()> {
    let text = render_plan(plan, format)?;

    if output == "-" {
        print!("{text}");
    } else {
        std::fs::write(output, text).context("Failed to write plan file")?;
    }

    Ok(())
}

/// Renders `plan` as the text of `format`.
///
/// # Errors
///
/// Returns an error when JSON serialization fails.
fn render_plan(plan: &Plan, format: &OutputFormat) -> Result<String> {
    match format {
        OutputFormat::Plain | OutputFormat::Ipc => Ok(format!("{plan}")),
        OutputFormat::Json => {
            let steps: Vec<&str> = plan.steps.iter().map(|s| s.op_name.as_str()).collect();
            serde_json::to_string_pretty(&serde_json::json!({
                "cost": plan.cost,
                "length": plan.len(),
                "steps": steps,
            }))
            .context("Failed to serialize plan to JSON")
        }
    }
}
