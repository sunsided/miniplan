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
    /// A pretty-printed JSON object with `cost`, `length` and the list of step names, plus
    /// `orderings` (pairs of 0-based step indices) when the plan has a partial order.
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
            let mut json = serde_json::json!({
                "cost": plan.cost,
                "length": plan.len(),
                "steps": steps,
            });

            if let Some(order) = &plan.partial_order {
                json["orderings"] = serde_json::json!(order.before);
            }

            serde_json::to_string_pretty(&json).context("Failed to serialize plan to JSON")
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::ValueEnum;
    use miniplan::plan::{PartialOrder, PlanStep};
    use miniplan::task::OpId;

    use super::*;

    /// A two-step plan with cost 2, built by hand so the expected text is fixed.
    fn two_step_plan() -> Plan {
        Plan {
            steps: vec![
                PlanStep {
                    op_id: OpId(0),
                    op_name: "move(a,b)".to_owned(),
                },
                PlanStep {
                    op_id: OpId(1),
                    op_name: "pick(x,b)".to_owned(),
                },
            ],
            cost: 2.0,
            partial_order: None,
        }
    }

    /// A path in the system temp directory unique to this process and `tag`.
    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("miniplan-plan-writer-{}-{tag}", std::process::id()))
    }

    /// The plain format is the plan's `Display` text: cost and length comments, then one step
    /// per line.
    #[test]
    fn plain_renders_comments_and_steps() {
        let text = render_plan(&two_step_plan(), &OutputFormat::Plain).unwrap();

        assert_eq!(text, "; cost = 2\n; length = 2\n(move(a,b))\n(pick(x,b))\n");
    }

    /// The IPC format currently produces exactly the plain text.
    #[test]
    fn ipc_matches_plain() {
        let text = render_plan(&two_step_plan(), &OutputFormat::Ipc).unwrap();

        assert_eq!(text, "; cost = 2\n; length = 2\n(move(a,b))\n(pick(x,b))\n");
    }

    /// The JSON format is a pretty-printed object with cost, length and step names, without a
    /// trailing newline.
    #[test]
    fn json_renders_cost_length_and_steps() {
        let text = render_plan(&two_step_plan(), &OutputFormat::Json).unwrap();

        let expected = "{\n  \"cost\": 2.0,\n  \"length\": 2,\n  \"steps\": [\n    \"move(a,b)\",\n    \"pick(x,b)\"\n  ]\n}";
        assert_eq!(text, expected);
    }

    /// A partial order shows up as `; order` comment lines in plain text and as `orderings`
    /// in JSON.
    #[test]
    fn partial_order_renders_in_plain_and_json() {
        let mut plan = two_step_plan();
        plan.partial_order = Some(PartialOrder {
            before: vec![(0, 1)],
        });

        let text = render_plan(&plan, &OutputFormat::Plain).unwrap();
        assert!(text.ends_with("(pick(x,b))\n; order 0 < 1\n"), "{text}");

        let json: serde_json::Value =
            serde_json::from_str(&render_plan(&plan, &OutputFormat::Json).unwrap()).unwrap();
        assert_eq!(json["orderings"], serde_json::json!([[0, 1]]));
    }

    /// An empty plan renders with zero cost and length and no steps.
    #[test]
    fn empty_plan_renders_zero() {
        let plan = Plan::new();

        assert_eq!(
            render_plan(&plan, &OutputFormat::Plain).unwrap(),
            "; cost = 0\n; length = 0\n"
        );
        assert_eq!(
            render_plan(&plan, &OutputFormat::Json).unwrap(),
            "{\n  \"cost\": 0.0,\n  \"length\": 0,\n  \"steps\": []\n}"
        );
    }

    /// Writing to a path creates the file with the rendered text and replaces old content.
    #[test]
    fn write_to_file_overwrites() {
        let path = temp_path("overwrite.json");
        std::fs::write(&path, "old content that is longer than the plan itself").unwrap();
        let plan = two_step_plan();

        write_plan(&plan, &OutputFormat::Json, path.to_str().unwrap()).unwrap();

        let written = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(written, render_plan(&plan, &OutputFormat::Json).unwrap());
    }

    /// Writing into a directory that does not exist fails with a write error.
    #[test]
    fn write_to_missing_directory_fails() {
        let path = temp_path("missing-dir").join("plan.txt");

        let err = write_plan(
            &two_step_plan(),
            &OutputFormat::Plain,
            path.to_str().unwrap(),
        )
        .unwrap_err();

        assert_eq!(err.to_string(), "Failed to write plan file");
    }

    /// `-` writes to stdout and succeeds.
    #[test]
    fn write_to_stdout_succeeds() {
        write_plan(&two_step_plan(), &OutputFormat::Plain, "-").unwrap();
    }

    /// Each format maps to its command-line name, and names parse back to the format.
    #[test]
    fn value_enum_names() {
        let names: Vec<String> = OutputFormat::value_variants()
            .iter()
            .map(|f| f.to_possible_value().unwrap().get_name().to_owned())
            .collect();

        assert_eq!(names, ["plain", "ipc", "json"]);
        assert!(matches!(
            OutputFormat::from_str("json", false),
            Ok(OutputFormat::Json)
        ));
    }
}
