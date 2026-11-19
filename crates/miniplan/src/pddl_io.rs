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
/// # Errors
///
/// Returns [`MiniplanError::Parse`] when `s` is not a valid PDDL domain.
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
/// # Errors
///
/// Returns [`MiniplanError::Parse`] when `s` is not a valid PDDL problem.
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
///
/// # Errors
///
/// Returns [`MiniplanError::Io`] when the file cannot be read as UTF-8 text,
/// and [`MiniplanError::Parse`] when it is not a valid PDDL domain.
pub fn load_domain_path(path: &Path) -> Result<Domain, MiniplanError> {
    load_domain_str(&read_source(path)?)
}

/// Load a PDDL problem from a file path.
///
/// # Errors
///
/// Returns [`MiniplanError::Io`] when the file cannot be read as UTF-8 text,
/// and [`MiniplanError::Parse`] when it is not a valid PDDL problem.
pub fn load_problem_path(path: &Path) -> Result<Problem, MiniplanError> {
    load_problem_str(&read_source(path)?)
}

/// Reads a PDDL source file into a string.
///
/// # Errors
///
/// Returns [`MiniplanError::Io`] when the file cannot be read as UTF-8 text.
fn read_source(path: &Path) -> Result<String, MiniplanError> {
    fs::read_to_string(path).map_err(MiniplanError::Io)
}

/// Parses a source that may hold any number of domain and problem
/// definitions.
///
/// # Errors
///
/// Returns [`MiniplanError::Parse`] when `s` is not valid PDDL.
pub(crate) fn load_pddl_file_str(s: &str) -> Result<PddlFile, MiniplanError> {
    PddlFile::from_str(s).map_err(|e| MiniplanError::Parse(e.to_string()))
}

/// A domain or problem definition, abstracted for name-based selection and
/// for error messages.
trait Definition: Clone {
    /// Lower-case kind used in messages and CLI flags: `domain` or `problem`.
    const KIND: &'static str;

    /// Whether the definition is declared with `name`.
    fn has_name(&self, name: &str) -> bool;

    /// The declared name, for listings.
    fn display_name(&self) -> String;
}

impl Definition for Domain {
    const KIND: &'static str = "domain";

    fn has_name(&self, name: &str) -> bool {
        *self.name() == name
    }

    fn display_name(&self) -> String {
        self.name().to_string()
    }
}

impl Definition for Problem {
    const KIND: &'static str = "problem";

    fn has_name(&self, name: &str) -> bool {
        *self.name() == name
    }

    fn display_name(&self) -> String {
        self.name().to_string()
    }
}

/// Comma-separated names of `defs`, in order, for error messages.
fn names<T: Definition>(defs: &[T]) -> String {
    defs.iter()
        .map(Definition::display_name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Error for a source without any definition of kind `T`.
fn missing<T: Definition>() -> MiniplanError {
    MiniplanError::Parse(format!("no {} definition found", T::KIND))
}

/// Error for several definitions of kind `T` where one is needed; `hint`
/// tells the user how to pick one.
fn ambiguous<T: Definition>(defs: &[T], hint: &str) -> MiniplanError {
    MiniplanError::Parse(format!(
        "multiple {} definitions found ({}); {hint}: {}",
        T::KIND,
        defs.len(),
        names(defs)
    ))
}

/// Error for several definitions of kind `T` and no name to pick one.
fn name_required<T: Definition>(defs: &[T]) -> MiniplanError {
    ambiguous(defs, &format!("--{} is required", T::KIND))
}

/// Returns the definition called `name`.
///
/// # Errors
///
/// Returns [`MiniplanError::Parse`] listing the available names when no
/// definition is called `name`.
fn find_named<T: Definition>(defs: &[T], name: &str) -> Result<T, MiniplanError> {
    defs.iter()
        .find(|d| d.has_name(name))
        .cloned()
        .ok_or_else(|| not_found(defs, name))
}

/// Error for a `name` that none of `defs` is called; lists the available
/// names.
fn not_found<T: Definition>(defs: &[T], name: &str) -> MiniplanError {
    MiniplanError::Parse(format!(
        "{} '{name}' not found; available: {}",
        T::KIND,
        names(defs)
    ))
}

/// Picks one definition: the only one (which must match `name` if given), or
/// the one called `name` among several.
///
/// # Errors
///
/// Returns [`MiniplanError::Parse`] when there is no definition, when there
/// are several and `name` is `None`, or when no definition matches `name`.
fn select_definition<T: Definition>(defs: &[T], name: Option<&str>) -> Result<T, MiniplanError> {
    match (defs, name) {
        ([], _) => Err(missing::<T>()),
        ([only], Some(name)) if !only.has_name(name) => Err(not_found(defs, name)),
        ([only], _) => Ok(only.clone()),
        (_, None) => Err(name_required(defs)),
        (_, Some(name)) => find_named(defs, name),
    }
}

/// Fails unless `file` holds at least one domain and one problem, checking
/// domains first.
///
/// # Errors
///
/// Returns [`MiniplanError::Parse`] for the first missing kind.
fn require_both(file: &PddlFile) -> Result<(), MiniplanError> {
    if file.domains.is_empty() {
        return Err(missing::<Domain>());
    }

    if file.problems.is_empty() {
        return Err(missing::<Problem>());
    }

    Ok(())
}

/// Load a combined PDDL file (domain + problem) from a string.
///
/// Errors if there isn't exactly one domain and one problem definition.
///
/// # Errors
///
/// Returns [`MiniplanError::Parse`] when `s` is not valid PDDL, when it lacks
/// a domain or a problem definition, or when it holds more than one of either
/// (use [`load_combined_str_named`] to pick one).
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
    require_both(&parsed)?;

    if parsed.domains.len() > 1 {
        return Err(ambiguous(&parsed.domains, "use --domain to select one"));
    }

    if parsed.problems.len() > 1 {
        return Err(ambiguous(&parsed.problems, "use --problem to select one"));
    }

    Ok((parsed.domains[0].clone(), parsed.problems[0].clone()))
}

/// Load a combined PDDL file, selecting by name if multiple definitions exist.
///
/// A given name must match even when there is only one definition of its
/// kind. The domain is resolved completely before the problem.
///
/// # Errors
///
/// Returns [`MiniplanError::Parse`] when `s` is not valid PDDL, or, for the
/// domain and then the problem: when there is no definition, when there are
/// several and no name is given, or when no definition has the given name.
pub fn load_combined_str_named(
    s: &str,
    domain_name: Option<&str>,
    problem_name: Option<&str>,
) -> Result<(Domain, Problem), MiniplanError> {
    let parsed = load_pddl_file_str(s)?;

    let domain = select_definition(&parsed.domains, domain_name)?;
    let problem = select_definition(&parsed.problems, problem_name)?;

    Ok((domain, problem))
}

/// Concatenates the definitions of several parsed files, in file order.
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
///
/// # Errors
///
/// Same as [`load_files_named`] without names: in particular,
/// [`MiniplanError::Parse`] when the files hold several domains or problems.
pub fn load_files(paths: &[impl AsRef<Path>]) -> Result<(Domain, Problem), MiniplanError> {
    load_files_named(paths, None, None)
}

/// Load from files with optional name selection.
///
/// One file is handled like [`load_combined_str_named`]. With two files, their
/// definitions are merged; a name is then only needed (and only checked) for
/// a kind that has more than one definition.
///
/// # Errors
///
/// - [`MiniplanError::Parse`] when `paths` is empty or has more than two
///   entries.
/// - [`MiniplanError::Io`] when a file cannot be read.
/// - [`MiniplanError::Parse`] when a file is not valid PDDL, a domain or
///   problem definition is missing, several exist and no name is given, or no
///   definition has the given name.
pub fn load_files_named(
    paths: &[impl AsRef<Path>],
    domain_name: Option<&str>,
    problem_name: Option<&str>,
) -> Result<(Domain, Problem), MiniplanError> {
    match paths {
        [] => Err(MiniplanError::Parse("no input files provided".into())),
        [path] => {
            let source = read_source(path.as_ref())?;
            load_combined_str_named(&source, domain_name, problem_name)
        }
        [first, second] => {
            let first = read_source(first.as_ref())?;
            let second = read_source(second.as_ref())?;
            let merged =
                merge_pddl_files(&[load_pddl_file_str(&first)?, load_pddl_file_str(&second)?]);
            select_from_merged(&merged, domain_name, problem_name)
        }
        _ => Err(MiniplanError::Parse("expected 1 or 2 input files".into())),
    }
}

/// Picks the domain and problem out of the merged definitions of two files.
///
/// All presence and ambiguity checks run before any name lookup.
///
/// # Errors
///
/// Returns [`MiniplanError::Parse`] when a kind is missing, when a kind has
/// several definitions and no name, or when no definition has the given name.
fn select_from_merged(
    merged: &PddlFile,
    domain_name: Option<&str>,
    problem_name: Option<&str>,
) -> Result<(Domain, Problem), MiniplanError> {
    require_both(merged)?;

    if merged.domains.len() > 1 && domain_name.is_none() {
        return Err(name_required(&merged.domains));
    }

    if merged.problems.len() > 1 && problem_name.is_none() {
        return Err(name_required(&merged.problems));
    }

    let domain = pick_unique_or_named(&merged.domains, domain_name)?;
    let problem = pick_unique_or_named(&merged.problems, problem_name)?;

    Ok((domain, problem))
}

/// Returns the only definition (ignoring `name`), or the one called `name`
/// among several.
///
/// `defs` must not be empty, and `name` must be `Some` when there are several
/// definitions; [`select_from_merged`] checks both beforehand.
///
/// # Errors
///
/// Returns [`MiniplanError::Parse`] when no definition has the given name,
/// or (unreachable after those checks) when `defs` is empty.
fn pick_unique_or_named<T: Definition>(defs: &[T], name: Option<&str>) -> Result<T, MiniplanError> {
    match name {
        Some(name) if defs.len() > 1 => find_named(defs, name),
        _ => defs.first().cloned().ok_or_else(missing::<T>),
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
///
/// Definitions keep file order and, within a file, declaration order. Files
/// without any definition are fine.
///
/// # Errors
///
/// Returns [`MiniplanError::Io`] when a file cannot be read, and
/// [`MiniplanError::Parse`] when a file is not valid PDDL. Loading stops at
/// the first failing file.
pub fn load_pddl_bundle(paths: &[impl AsRef<Path>]) -> Result<PddlBundle, MiniplanError> {
    let mut domains = Vec::new();
    let mut problems = Vec::new();

    for path in paths {
        let path = path.as_ref();
        let parsed = load_pddl_file_str(&read_source(path)?)?;
        for domain in parsed.domains {
            domains.push((path.to_path_buf(), domain));
        }

        for problem in parsed.problems {
            problems.push((path.to_path_buf(), problem));
        }
    }

    Ok(PddlBundle { domains, problems })
}

/// Tests for [`load_pddl_bundle`].
#[cfg(test)]
mod tests_bundle {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Writes `content` to a fresh temporary file that lives as long as the handle.
    fn write_pddl(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    /// One file holding a domain and a problem yields one of each.
    #[test]
    fn test_single_file_with_both() {
        let content = r"
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
";
        let f = write_pddl(content);
        let bundle = load_pddl_bundle(&[f.path()]).unwrap();
        assert_eq!(bundle.domains.len(), 1);
        assert_eq!(bundle.problems.len(), 1);
        assert_eq!(bundle.domains[0].1.name().to_string(), "test-domain");
        assert_eq!(bundle.problems[0].1.name().to_string(), "test-problem");
    }

    /// A domain file and a problem file are bundled together.
    #[test]
    fn test_two_files_separate() {
        let domain_content = r"
(define (domain test-domain)
  (:requirements :strips)
  (:predicates (at ?x))
)
";
        let problem_content = r"
(define (problem test-problem)
  (:domain test-domain)
  (:objects)
  (:init)
  (:goal (at obj1))
)
";
        let f1 = write_pddl(domain_content);
        let f2 = write_pddl(problem_content);
        let bundle = load_pddl_bundle(&[f1.path(), f2.path()]).unwrap();
        assert_eq!(bundle.domains.len(), 1);
        assert_eq!(bundle.problems.len(), 1);
    }

    /// Definitions from all files are collected, including several domains.
    #[test]
    fn test_three_files_mixed() {
        let domain1 = r"
(define (domain domain-a)
  (:requirements :strips)
  (:predicates (p ?x))
)
";
        let domain2 = r"
(define (domain domain-b)
  (:requirements :strips)
  (:predicates (q ?x))
)
";
        let problem = r"
(define (problem prob-1)
  (:domain domain-a)
  (:objects)
  (:init)
  (:goal (p obj1))
)
";
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

    /// A file without definitions yields an empty bundle, not an error.
    #[test]
    fn test_empty_bundle() {
        let content = "; just a comment";
        let f = write_pddl(content);
        let bundle = load_pddl_bundle(&[f.path()]).unwrap();
        assert_eq!(
            bundle.domains,
            [] as [(std::path::PathBuf, pddl::Domain); 0]
        );
        assert_eq!(
            bundle.problems,
            [] as [(std::path::PathBuf, pddl::Problem); 0]
        );
    }
}
