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

pub fn load_combined_str(s: &str) -> Result<(Domain, Problem), MiniplanError> {
    let domain = load_domain_str(s)?;
    let problem = load_problem_str(s)?;
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
