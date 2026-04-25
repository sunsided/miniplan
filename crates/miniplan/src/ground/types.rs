use indexmap::IndexMap;
use pddl::{Domain, Problem, Type};

use crate::MiniplanError;
use crate::task::{Object, TypeHierarchy};

pub fn extract_types(domain: &Domain) -> Result<TypeHierarchy, MiniplanError> {
    let mut hierarchy = TypeHierarchy::new();
    hierarchy.add_type("object".to_owned(), None);

    for typed_name in domain.types().iter() {
        let name = typed_name.value().to_string();
        let supertype = type_to_string(typed_name.type_());
        hierarchy.add_type(name, Some(supertype));
    }

    Ok(hierarchy)
}

pub fn extract_objects(
    domain: &Domain,
    problem: &Problem,
    _types: &TypeHierarchy,
) -> Result<Vec<Object>, MiniplanError> {
    let mut objects: IndexMap<String, String> = IndexMap::new();

    // Domain constants
    for typed_name in domain.constants().iter() {
        let name = typed_name.value().to_string();
        let type_name = type_to_string(typed_name.type_());
        objects.entry(name).or_insert(type_name);
    }

    // Problem objects
    for typed_name in problem.objects().iter() {
        let name = typed_name.value().to_string();
        let type_name = type_to_string(typed_name.type_());
        objects.entry(name).or_insert(type_name);
    }

    let result = objects
        .into_iter()
        .map(|(name, type_name)| Object { name, type_name })
        .collect();

    Ok(result)
}

pub fn objects_of_type<'a>(
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

fn type_to_string(t: &Type) -> String {
    match t {
        Type::Exactly(pt) => pt.to_string(),
        Type::EitherOf(pts) => pts
            .first()
            .map(|p| p.to_string())
            .unwrap_or_else(|| "object".to_owned()),
    }
}
