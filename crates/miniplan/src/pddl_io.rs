use std::fs;
use std::path::Path;

use pddl::{Domain, Parser, Problem};

use crate::error::MiniplanError;

pub fn load_domain_str(s: &str) -> Result<Domain, MiniplanError> {
    Domain::from_str(s).map_err(|e| MiniplanError::Parse(e.to_string()))
}

pub fn load_problem_str(s: &str) -> Result<Problem, MiniplanError> {
    Problem::from_str(s).map_err(|e| MiniplanError::Parse(e.to_string()))
}

pub fn load_domain_path(path: &Path) -> Result<Domain, MiniplanError> {
    let s = fs::read_to_string(path).map_err(MiniplanError::Io)?;
    load_domain_str(&s)
}

pub fn load_problem_path(path: &Path) -> Result<Problem, MiniplanError> {
    let s = fs::read_to_string(path).map_err(MiniplanError::Io)?;
    load_problem_str(&s)
}

fn extract_define_blocks(input: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut depth = 0;
    let mut start: Option<usize> = None;
    let mut in_comment = false;

    for (i, c) in input.char_indices() {
        if in_comment {
            if c == '\n' {
                in_comment = false;
            }
            continue;
        }

        match c {
            ';' => {
                in_comment = true;
                continue;
            }
            '(' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        blocks.push(input[s..=i].to_string());
                        start = None;
                    }
                }
            }
            _ => {}
        }
    }

    blocks
}

pub fn load_combined_str(s: &str) -> Result<(Domain, Problem), MiniplanError> {
    let blocks = extract_define_blocks(s);
    if blocks.is_empty() {
        return Err(MiniplanError::Parse("no (define ...) blocks found".into()));
    }

    let mut domain: Option<Domain> = None;
    let mut problem: Option<Problem> = None;

    for block in &blocks {
        if domain.is_none() {
            if let Ok(d) = Domain::from_str(block) {
                domain = Some(d);
                continue;
            }
        }
        if problem.is_none() {
            if let Ok(p) = Problem::from_str(block) {
                problem = Some(p);
                continue;
            }
        }
    }

    let domain = domain.ok_or_else(|| MiniplanError::Parse("no domain definition found".into()))?;
    let problem =
        problem.ok_or_else(|| MiniplanError::Parse("no problem definition found".into()))?;

    Ok((domain, problem))
}

pub fn load_files(paths: &[impl AsRef<Path>]) -> Result<(Domain, Problem), MiniplanError> {
    match paths.len() {
        0 => Err(MiniplanError::Parse("no input files provided".into())),
        1 => {
            let s = fs::read_to_string(paths[0].as_ref()).map_err(MiniplanError::Io)?;
            load_combined_str(&s)
        }
        2 => {
            let s0 = fs::read_to_string(paths[0].as_ref()).map_err(MiniplanError::Io)?;
            let s1 = fs::read_to_string(paths[1].as_ref()).map_err(MiniplanError::Io)?;
            let d0 = Domain::from_str(&s0);
            let p0 = Problem::from_str(&s0);

            match (d0, p0) {
                (Ok(d), Err(_)) => {
                    let p =
                        Problem::from_str(&s1).map_err(|e| MiniplanError::Parse(e.to_string()))?;
                    Ok((d, p))
                }
                (Err(_), Ok(p)) => {
                    let d =
                        Domain::from_str(&s1).map_err(|e| MiniplanError::Parse(e.to_string()))?;
                    Ok((d, p))
                }
                _ => {
                    let d =
                        Domain::from_str(&s1).map_err(|e| MiniplanError::Parse(e.to_string()))?;
                    let p =
                        Problem::from_str(&s0).map_err(|e| MiniplanError::Parse(e.to_string()))?;
                    Ok((d, p))
                }
            }
        }
        _ => Err(MiniplanError::Parse("expected 1 or 2 input files".into())),
    }
}
