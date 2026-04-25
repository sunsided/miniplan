use anyhow::{Context, Result};
use miniplan::plan::Plan;

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Plain,
    Ipc,
    Json,
}

pub fn write_plan(plan: &Plan, format: &OutputFormat, output: &str) -> Result<()> {
    let text = match format {
        OutputFormat::Plain | OutputFormat::Ipc => {
            format!("{}", plan)
        }
        OutputFormat::Json => {
            let steps: Vec<&str> = plan.steps.iter().map(|s| s.op_name.as_str()).collect();
            serde_json::to_string_pretty(&serde_json::json!({
                "cost": plan.cost,
                "length": plan.len(),
                "steps": steps,
            }))
            .context("Failed to serialize plan to JSON")?
        }
    };

    if output == "-" {
        print!("{}", text);
    } else {
        std::fs::write(output, text).context("Failed to write plan file")?;
    }

    Ok(())
}
