use std::fs;
use std::path::Path;

use pddl::{Domain, Parser, Problem};

pub use pddl::PddlFile;

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

fn load_pddl_file_str(s: &str) -> Result<PddlFile, MiniplanError> {
    PddlFile::from_str(s).map_err(|e| MiniplanError::Parse(e.to_string()))
}

#[allow(dead_code)]
fn load_pddl_file_path(path: &Path) -> Result<PddlFile, MiniplanError> {
    let s = fs::read_to_string(path).map_err(MiniplanError::Io)?;
    load_pddl_file_str(&s)
}

fn domain_names(domains: &[Domain]) -> String {
    domains
        .iter()
        .map(|d| d.name().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn problem_names(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(|p| p.name().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn load_combined_str(s: &str) -> Result<(Domain, Problem), MiniplanError> {
    let parsed = load_pddl_file_str(s)?;

    if parsed.domains.is_empty() {
        return Err(MiniplanError::Parse("no domain definition found".into()));
    }
    if parsed.problems.is_empty() {
        return Err(MiniplanError::Parse("no problem definition found".into()));
    }
    if parsed.domains.len() > 1 {
        return Err(MiniplanError::Parse(format!(
            "multiple domain definitions found ({}); use --domain to select one: {}",
            parsed.domains.len(),
            domain_names(&parsed.domains)
        )));
    }
    if parsed.problems.len() > 1 {
        return Err(MiniplanError::Parse(format!(
            "multiple problem definitions found ({}); use --problem to select one: {}",
            parsed.problems.len(),
            problem_names(&parsed.problems)
        )));
    }

    Ok((parsed.domains[0].clone(), parsed.problems[0].clone()))
}

pub fn load_combined_str_named(
    s: &str,
    domain_name: Option<&str>,
    problem_name: Option<&str>,
) -> Result<(Domain, Problem), MiniplanError> {
    let parsed = load_pddl_file_str(s)?;

    let domain = if parsed.domains.is_empty() {
        return Err(MiniplanError::Parse("no domain definition found".into()));
    } else if parsed.domains.len() == 1 {
        let d = &parsed.domains[0];
        if let Some(name) = domain_name
            && *d.name() != name {
                return Err(MiniplanError::Parse(format!(
                    "domain '{}' not found; available: {}",
                    name,
                    domain_names(&parsed.domains)
                )));
            }
        d.clone()
    } else {
        let name = domain_name.ok_or_else(|| {
            MiniplanError::Parse(format!(
                "multiple domain definitions found ({}); --domain is required: {}",
                parsed.domains.len(),
                domain_names(&parsed.domains)
            ))
        })?;
        parsed
            .domains
            .iter()
            .find(|d| *d.name() == name)
            .cloned()
            .ok_or_else(|| {
                MiniplanError::Parse(format!(
                    "domain '{}' not found; available: {}",
                    name,
                    domain_names(&parsed.domains)
                ))
            })?
    };

    let problem = if parsed.problems.is_empty() {
        return Err(MiniplanError::Parse("no problem definition found".into()));
    } else if parsed.problems.len() == 1 {
        let p = &parsed.problems[0];
        if let Some(name) = problem_name
            && *p.name() != name {
                return Err(MiniplanError::Parse(format!(
                    "problem '{}' not found; available: {}",
                    name,
                    problem_names(&parsed.problems)
                )));
            }
        p.clone()
    } else {
        let name = problem_name.ok_or_else(|| {
            MiniplanError::Parse(format!(
                "multiple problem definitions found ({}); --problem is required: {}",
                parsed.problems.len(),
                problem_names(&parsed.problems)
            ))
        })?;
        parsed
            .problems
            .iter()
            .find(|p| *p.name() == name)
            .cloned()
            .ok_or_else(|| {
                MiniplanError::Parse(format!(
                    "problem '{}' not found; available: {}",
                    name,
                    problem_names(&parsed.problems)
                ))
            })?
    };

    Ok((domain, problem))
}

fn merge_pddl_files(files: &[PddlFile]) -> PddlFile {
    let mut domains = Vec::new();
    let mut problems = Vec::new();
    for f in files {
        domains.extend(f.domains.iter().cloned());
        problems.extend(f.problems.iter().cloned());
    }
    PddlFile { domains, problems }
}

pub fn load_files(paths: &[impl AsRef<Path>]) -> Result<(Domain, Problem), MiniplanError> {
    load_files_named(paths, None, None)
}

pub fn load_files_named(
    paths: &[impl AsRef<Path>],
    domain_name: Option<&str>,
    problem_name: Option<&str>,
) -> Result<(Domain, Problem), MiniplanError> {
    match paths.len() {
        0 => Err(MiniplanError::Parse("no input files provided".into())),
        1 => {
            let s = fs::read_to_string(paths[0].as_ref()).map_err(MiniplanError::Io)?;
            load_combined_str_named(&s, domain_name, problem_name)
        }
        2 => {
            let s0 = fs::read_to_string(paths[0].as_ref()).map_err(MiniplanError::Io)?;
            let s1 = fs::read_to_string(paths[1].as_ref()).map_err(MiniplanError::Io)?;
            let p0 = load_pddl_file_str(&s0)?;
            let p1 = load_pddl_file_str(&s1)?;
            let merged = merge_pddl_files(&[p0, p1]);

            if merged.domains.is_empty() {
                return Err(MiniplanError::Parse("no domain definition found".into()));
            }
            if merged.problems.is_empty() {
                return Err(MiniplanError::Parse("no problem definition found".into()));
            }
            if merged.domains.len() > 1 && domain_name.is_none() {
                return Err(MiniplanError::Parse(format!(
                    "multiple domain definitions found ({}); --domain is required: {}",
                    merged.domains.len(),
                    domain_names(&merged.domains)
                )));
            }
            if merged.problems.len() > 1 && problem_name.is_none() {
                return Err(MiniplanError::Parse(format!(
                    "multiple problem definitions found ({}); --problem is required: {}",
                    merged.problems.len(),
                    problem_names(&merged.problems)
                )));
            }

            let domain = if merged.domains.len() == 1 {
                merged.domains[0].clone()
            } else {
                merged
                    .domains
                    .iter()
                    .find(|d| *d.name() == domain_name.unwrap())
                    .cloned()
                    .ok_or_else(|| {
                        MiniplanError::Parse(format!(
                            "domain '{}' not found; available: {}",
                            domain_name.unwrap(),
                            domain_names(&merged.domains)
                        ))
                    })?
            };

            let problem = if merged.problems.len() == 1 {
                merged.problems[0].clone()
            } else {
                merged
                    .problems
                    .iter()
                    .find(|p| *p.name() == problem_name.unwrap())
                    .cloned()
                    .ok_or_else(|| {
                        MiniplanError::Parse(format!(
                            "problem '{}' not found; available: {}",
                            problem_name.unwrap(),
                            problem_names(&merged.problems)
                        ))
                    })?
            };

            Ok((domain, problem))
        }
        _ => Err(MiniplanError::Parse("expected 1 or 2 input files".into())),
    }
}
