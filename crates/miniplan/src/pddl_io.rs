//! PDDL file loading utilities.
//!
//! This module provides functions to load PDDL domain and problem definitions
//! from strings, files, or multiple files. It wraps the `pddl` crate's parser.
//!
//! # Examples
//!
//! ```
//! use miniplan::pddl_io::load_domain_str;
//!
//! let domain = load_domain_str(r#"
//! (define (domain test)
//!   (:requirements :strips)
//!   (:predicates (a))
//! )
//! "#).expect("domain parses");
//! assert_eq!(domain.name().to_string(), "test");
//! ```

use std::fs;
use std::path::Path;

use pddl::{Domain, Parser, Problem};

pub use pddl::PddlFile;

use crate::error::MiniplanError;

/// Parse a PDDL domain from a string.
///
/// # Examples
///
/// ```
/// use miniplan::pddl_io::load_domain_str;
///
/// let domain = load_domain_str(r#"
/// (define (domain my-domain)
///   (:requirements :strips)
///   (:predicates (x))
/// )
/// "#).expect("domain parses");
/// assert_eq!(domain.name().to_string(), "my-domain");
/// ```
pub fn load_domain_str(s: &str) -> Result<Domain, MiniplanError> {
    Domain::from_str(s).map_err(|e| MiniplanError::Parse(e.to_string()))
}

/// Parse a PDDL problem from a string.
///
/// # Examples
///
/// ```
/// use miniplan::pddl_io::load_problem_str;
///
/// let problem = load_problem_str(r#"
/// (define (problem my-problem)
///   (:domain test)
///   (:init)
///   (:goal (and)))
/// "#).expect("problem parses");
/// assert_eq!(problem.name().to_string(), "my-problem");
/// ```
pub fn load_problem_str(s: &str) -> Result<Problem, MiniplanError> {
    Problem::from_str(s).map_err(|e| MiniplanError::Parse(e.to_string()))
}

/// Load a PDDL domain from a file path.
pub fn load_domain_path(path: &Path) -> Result<Domain, MiniplanError> {
    let s = fs::read_to_string(path).map_err(MiniplanError::Io)?;
    load_domain_str(&s)
}

/// Load a PDDL problem from a file path.
pub fn load_problem_path(path: &Path) -> Result<Problem, MiniplanError> {
    let s = fs::read_to_string(path).map_err(MiniplanError::Io)?;
    load_problem_str(&s)
}

pub(crate) fn load_pddl_file_str(s: &str) -> Result<PddlFile, MiniplanError> {
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

/// Load a combined PDDL file (domain + problem) from a string.
///
/// Errors if there isn't exactly one domain and one problem definition.
///
/// # Examples
///
/// ```
/// use miniplan::pddl_io::load_combined_str;
///
/// let (domain, problem) = load_combined_str(r#"
/// (define (domain test)
///   (:requirements :strips)
///   (:predicates (a))
/// )
/// (define (problem test-1)
///   (:domain test)
///   (:init)
///   (:goal (a)))
/// "#).expect("parses");
/// assert_eq!(domain.name().to_string(), "test");
/// ```
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

/// Load a combined PDDL file, selecting by name if multiple definitions exist.
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
            && *d.name() != name
        {
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
            && *p.name() != name
        {
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

/// Load a domain and problem from one or two file paths.
pub fn load_files(paths: &[impl AsRef<Path>]) -> Result<(Domain, Problem), MiniplanError> {
    load_files_named(paths, None, None)
}

/// Load from files with optional name selection.
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

/// A collection of loaded PDDL domains and problems with their source paths.
pub struct PddlBundle {
    /// Domains paired with their source file paths.
    pub domains: Vec<(std::path::PathBuf, Domain)>,
    /// Problems paired with their source file paths.
    pub problems: Vec<(std::path::PathBuf, Problem)>,
}

/// Load a bundle of PDDL files, extracting all domain and problem definitions.
pub fn load_pddl_bundle(paths: &[impl AsRef<Path>]) -> Result<PddlBundle, MiniplanError> {
    let mut domains = Vec::new();
    let mut problems = Vec::new();

    for path in paths {
        let path = path.as_ref();
        let s = fs::read_to_string(path).map_err(MiniplanError::Io)?;
        let parsed = load_pddl_file_str(&s)?;
        for domain in parsed.domains {
            domains.push((path.to_path_buf(), domain));
        }
        for problem in parsed.problems {
            problems.push((path.to_path_buf(), problem));
        }
    }

    Ok(PddlBundle { domains, problems })
}

#[cfg(test)]
mod tests_bundle {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_pddl(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn test_single_file_with_both() {
        let content = r#"
(define (domain test-domain)
  (:requirements :strips)
  (:predicates (at ?x))
)
(define (problem test-problem)
  (:domain test-domain)
  (:objects)
  (:init)
  (:goal (at obj1))
)
"#;
        let f = write_pddl(content);
        let bundle = load_pddl_bundle(&[f.path()]).unwrap();
        assert_eq!(bundle.domains.len(), 1);
        assert_eq!(bundle.problems.len(), 1);
        assert_eq!(bundle.domains[0].1.name().to_string(), "test-domain");
        assert_eq!(bundle.problems[0].1.name().to_string(), "test-problem");
    }

    #[test]
    fn test_two_files_separate() {
        let domain_content = r#"
(define (domain test-domain)
  (:requirements :strips)
  (:predicates (at ?x))
)
"#;
        let problem_content = r#"
(define (problem test-problem)
  (:domain test-domain)
  (:objects)
  (:init)
  (:goal (at obj1))
)
"#;
        let f1 = write_pddl(domain_content);
        let f2 = write_pddl(problem_content);
        let bundle = load_pddl_bundle(&[f1.path(), f2.path()]).unwrap();
        assert_eq!(bundle.domains.len(), 1);
        assert_eq!(bundle.problems.len(), 1);
    }

    #[test]
    fn test_three_files_mixed() {
        let domain1 = r#"
(define (domain domain-a)
  (:requirements :strips)
  (:predicates (p ?x))
)
"#;
        let domain2 = r#"
(define (domain domain-b)
  (:requirements :strips)
  (:predicates (q ?x))
)
"#;
        let problem = r#"
(define (problem prob-1)
  (:domain domain-a)
  (:objects)
  (:init)
  (:goal (p obj1))
)
"#;
        let f1 = write_pddl(domain1);
        let f2 = write_pddl(domain2);
        let f3 = write_pddl(problem);
        let bundle = load_pddl_bundle(&[f1.path(), f2.path(), f3.path()]).unwrap();
        assert_eq!(bundle.domains.len(), 2);
        assert_eq!(bundle.problems.len(), 1);
        let names: Vec<_> = bundle
            .domains
            .iter()
            .map(|(_, d)| d.name().to_string())
            .collect();
        assert!(names.contains(&"domain-a".to_string()));
        assert!(names.contains(&"domain-b".to_string()));
    }

    #[test]
    fn test_empty_bundle() {
        let content = "; just a comment";
        let f = write_pddl(content);
        let bundle = load_pddl_bundle(&[f.path()]).unwrap();
        assert!(bundle.domains.is_empty());
        assert!(bundle.problems.is_empty());
    }
}
