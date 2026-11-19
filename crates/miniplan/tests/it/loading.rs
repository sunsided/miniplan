//! Tests for loading PDDL from files: path loaders, two-file bundles and name selection.

use std::path::{Path, PathBuf};

use miniplan::error::MiniplanError;
use miniplan::pddl_io::{load_domain_path, load_files, load_files_named, load_problem_path};
use tempfile::TempDir;

/// A domain named `d1`.
const DOMAIN_D1: &str = "(define (domain d1) (:requirements :strips) (:predicates (p)))";

/// A second domain, named `d2`.
const DOMAIN_D2: &str = "(define (domain d2) (:requirements :strips) (:predicates (q)))";

/// A problem named `p1` for `d1`.
const PROBLEM_P1: &str = "(define (problem p1) (:domain d1) (:init) (:goal (p)))";

/// A second problem, named `p2`, for `d1`.
const PROBLEM_P2: &str = "(define (problem p2) (:domain d1) (:init (p)) (:goal (p)))";

/// Writes `files` (name, contents) into a fresh temporary directory.
///
/// Returns the directory guard, which deletes it on drop, and the file paths in order.
fn write_files(files: &[(&str, &str)]) -> (TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = files
        .iter()
        .map(|(name, contents)| {
            let path = dir.path().join(name);
            std::fs::write(&path, contents).expect("write fixture");
            path
        })
        .collect();

    (dir, paths)
}

/// The parse error message of `result`.
///
/// # Panics
///
/// Panics when `result` is not a [`MiniplanError::Parse`].
fn parse_error<T>(result: Result<T, MiniplanError>) -> String {
    match result {
        Err(MiniplanError::Parse(message)) => message,
        Err(other) => panic!("expected a parse error, got {other}"),
        Ok(_) => panic!("expected a parse error, got success"),
    }
}

/// The separate domain and problem loaders read their files.
#[test]
fn path_loaders_read_domain_and_problem() {
    let (_dir, paths) = write_files(&[("d.pddl", DOMAIN_D1), ("p.pddl", PROBLEM_P1)]);

    let domain = load_domain_path(&paths[0]).expect("domain loads");
    let problem = load_problem_path(&paths[1]).expect("problem loads");

    assert_eq!(domain.name().to_string(), "d1");
    assert_eq!(problem.name().to_string(), "p1");
}

/// A missing file is an I/O error, not a parse error.
#[test]
fn missing_file_is_an_io_error() {
    let missing = Path::new("/nonexistent/miniplan/missing.pddl");

    assert!(matches!(
        load_domain_path(missing),
        Err(MiniplanError::Io(_))
    ));
    assert!(matches!(load_files(&[missing]), Err(MiniplanError::Io(_))));
}

/// Zero or more than two paths are rejected.
#[test]
fn wrong_number_of_files_is_rejected() {
    let none: [&Path; 0] = [];
    let (_dir, paths) = write_files(&[
        ("a.pddl", DOMAIN_D1),
        ("b.pddl", PROBLEM_P1),
        ("c.pddl", PROBLEM_P2),
    ]);

    assert!(parse_error(load_files(&none)).contains("no input files"));
    assert!(parse_error(load_files(&paths)).contains("1 or 2 input files"));
}

/// A domain file plus a problem file load as a pair, in either order.
#[test]
fn two_files_merge_into_domain_and_problem() {
    let (_dir, paths) = write_files(&[("d.pddl", DOMAIN_D1), ("p.pddl", PROBLEM_P1)]);
    let reversed = [paths[1].clone(), paths[0].clone()];

    for files in [paths.as_slice(), reversed.as_slice()] {
        let (domain, problem) = load_files(files).expect("pair loads");
        assert_eq!(domain.name().to_string(), "d1");
        assert_eq!(problem.name().to_string(), "p1");
    }
}

/// Two files without a problem, or without a domain, name the missing kind.
#[test]
fn two_files_need_both_kinds() {
    let (_dir, domains) = write_files(&[("d1.pddl", DOMAIN_D1), ("d2.pddl", DOMAIN_D2)]);
    let (_dir2, problems) = write_files(&[("p1.pddl", PROBLEM_P1), ("p2.pddl", PROBLEM_P2)]);

    assert!(parse_error(load_files(&domains)).contains("no problem definition"));
    assert!(parse_error(load_files(&problems)).contains("no domain definition"));
}

/// With several problems across two files, a name is required and selects one.
#[test]
fn two_files_select_problem_by_name() {
    let (_dir, paths) = write_files(&[
        ("d.pddl", DOMAIN_D1),
        ("ps.pddl", &format!("{PROBLEM_P1}\n{PROBLEM_P2}")),
    ]);

    assert!(
        parse_error(load_files(&paths))
            .contains("multiple problem definitions found (2); --problem is required")
    );

    let (_, problem) = load_files_named(&paths, None, Some("p2")).expect("p2 is selected");
    assert_eq!(problem.name().to_string(), "p2");

    assert!(
        parse_error(load_files_named(&paths, None, Some("p9")))
            .contains("problem 'p9' not found; available:")
    );
}

/// With several domains across two files, a name is required and selects one.
#[test]
fn two_files_select_domain_by_name() {
    let (_dir, paths) = write_files(&[
        ("ds.pddl", &format!("{DOMAIN_D1}\n{DOMAIN_D2}")),
        ("p.pddl", PROBLEM_P1),
    ]);

    assert!(
        parse_error(load_files(&paths))
            .contains("multiple domain definitions found (2); --domain is required")
    );

    let (domain, _) = load_files_named(&paths, Some("d2"), None).expect("d2 is selected");
    assert_eq!(domain.name().to_string(), "d2");
}

/// With one definition per kind across two files, a given name must still match it.
#[test]
fn two_files_check_given_names() {
    let (_dir, paths) = write_files(&[("d.pddl", DOMAIN_D1), ("p.pddl", PROBLEM_P1)]);

    assert!(load_files_named(&paths, Some("d1"), Some("p1")).is_ok());
    assert!(
        parse_error(load_files_named(&paths, Some("d9"), None)).contains("domain 'd9' not found")
    );
    assert!(
        parse_error(load_files_named(&paths, None, Some("p9"))).contains("problem 'p9' not found")
    );
}

/// A single file with one domain and one problem accepts their own names and reports a
/// name that matches neither.
#[test]
fn single_file_checks_given_names() {
    let (_dir, paths) = write_files(&[("both.pddl", &format!("{DOMAIN_D1}\n{PROBLEM_P1}"))]);

    assert!(load_files_named(&paths, Some("d1"), Some("p1")).is_ok());
    assert!(
        parse_error(load_files_named(&paths, Some("d9"), None)).contains("domain 'd9' not found")
    );
}

/// Invalid PDDL in a file is a parse error.
#[test]
fn invalid_pddl_is_a_parse_error() {
    let (_dir, paths) = write_files(&[("bad.pddl", "(define (domain")]);

    parse_error(load_files(&paths));
    assert!(matches!(
        load_domain_path(&paths[0]),
        Err(MiniplanError::Parse(_))
    ));
}
