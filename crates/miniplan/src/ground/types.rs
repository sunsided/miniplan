//! Type hierarchy, object extraction and parameter binding enumeration.
//!
//! Everything here works on PDDL type *names*: an `(either a b)` type is
//! collapsed to its first member, and an untyped item has type `object`.

use indexmap::IndexMap;
use pddl::{Domain, Problem, Type};

use crate::task::{Object, TypeHierarchy};

/// A variable binding: `(variable name, object name)`, e.g. `("?x", "a")`.
///
/// Variable names keep their leading `?` so they match the `Display` form of
/// `pddl::Variable`.
pub(super) type Binding = (String, String);

/// Builds the [`TypeHierarchy`] declared in the domain's `:types` section.
///
/// The root type `object` is always registered. A type declared as its own
/// supertype is treated as having no supertype (and so implicitly derives from
/// `object`): some PDDL parsers produce such self-loops for declarations like
/// `(:types location disc peg - location)`, where `location` itself ends up
/// with supertype `location`.
pub(super) fn extract_types(domain: &Domain) -> TypeHierarchy {
    let mut hierarchy = TypeHierarchy::new();
    hierarchy.add_type("object".to_owned(), None);

    for typed_name in domain.types().iter() {
        let name = typed_name.value().to_string();
        let supertype = type_to_string(typed_name.type_());
        let supertype = if supertype == name {
            None
        } else {
            Some(supertype)
        };
        hierarchy.add_type(name, supertype);
    }

    hierarchy
}

/// Collects all objects: the domain's `:constants` followed by the problem's
/// `:objects`.
///
/// Order is first-declaration order and duplicates are dropped; when a name is
/// declared twice, the type of its first declaration wins. The order matters
/// because it fixes the order of grounded facts and operators.
pub(super) fn extract_objects(domain: &Domain, problem: &Problem) -> Vec<Object> {
    let mut objects: IndexMap<String, String> = IndexMap::new();
    let declarations = domain.constants().iter().chain(problem.objects().iter());

    for typed_name in declarations {
        let name = typed_name.value().to_string();
        let type_name = type_to_string(typed_name.type_());
        objects.entry(name).or_insert(type_name);
    }

    objects
        .into_iter()
        .map(|(name, type_name)| Object { name, type_name })
        .collect()
}

/// Returns the names of all objects whose type is `type_name` or one of its
/// subtypes, in object order.
pub(super) fn objects_of_type<'a>(
    objects: &'a [Object],
    type_name: &str,
    types: &'a TypeHierarchy,
) -> Vec<&'a str> {
    objects
        .iter()
        .filter(|o| types.is_subtype_of(&o.type_name, type_name))
        .map(|o| o.name.as_str())
        .collect()
}

/// Enumerates every assignment of objects to the typed `params`.
///
/// `params` holds `(variable, type name)` pairs. The result is the cartesian
/// product of the matching objects in lexicographic order, with the last
/// parameter varying fastest. No parameters yield one empty binding; a
/// parameter without matching objects yields no bindings at all.
pub(super) fn generate_bindings(
    params: &[(String, String)],
    objects: &[Object],
    types: &TypeHierarchy,
) -> Vec<Vec<Binding>> {
    if params.is_empty() {
        return vec![vec![]];
    }

    let param_objects: Vec<Vec<&str>> = params
        .iter()
        .map(|(_, sort)| objects_of_type(objects, sort, types))
        .collect();
    let counts: Vec<usize> = param_objects.iter().map(Vec::len).collect();

    let mut results = Vec::new();
    if counts.contains(&0) {
        return results;
    }

    let mut indices = vec![0usize; params.len()];
    loop {
        let binding: Vec<Binding> = params
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (name.clone(), param_objects[i][indices[i]].to_owned()))
            .collect();
        results.push(binding);

        if !advance_odometer(&mut indices, &counts) {
            return results;
        }
    }
}

/// Advances `indices` to the next combination, like an odometer whose digit
/// `i` counts up to `limits[i]` (exclusive) with the last digit fastest.
///
/// Returns `false` once every combination has been visited; `indices` is then
/// all zeros again. `indices` must not be empty and every limit must be
/// positive.
pub(super) fn advance_odometer(indices: &mut [usize], limits: &[usize]) -> bool {
    for idx in (0..indices.len()).rev() {
        indices[idx] += 1;
        if indices[idx] < limits[idx] {
            return true;
        }

        indices[idx] = 0;
    }

    false
}

/// Converts a PDDL type to its name.
///
/// `(either a b ...)` collapses to its first member `a`; an empty `either`
/// becomes `object`.
pub(super) fn type_to_string(t: &Type) -> String {
    match t {
        Type::Exactly(pt) => pt.to_string(),
        Type::EitherOf(pts) => pts
            .first()
            .map_or_else(|| "object".to_owned(), std::string::ToString::to_string),
    }
}
