//! Cross-file type resolution. Populates `JavaReferenceType::resolved_fqn` on
//! every reachable reference whose target is also in the parsed file set.
//!
//! Types outside the parsed set (JDK, third-party libraries) stay unresolved;
//! we never trust an import as proof a type exists.

use std::collections::{HashMap, HashSet};

use super::model::{
    JavaAnnotationValue, JavaFile, JavaReferenceType, JavaType, JavaTypeArgument, JavaTypeRef,
    JavaWildcardBound,
};

/// Resolves type references across the given files in place.
pub fn resolve_files(files: &mut [JavaFile]) {
    let index = GlobalIndex::build(files);
    for file in files.iter_mut() {
        let scope = FileScope::build(file, &index);
        for ty in file.types.iter_mut() {
            resolve_type(ty, &scope, &index);
        }
    }
}

/// Set of known FQNs across all parsed files, plus a package → simple-name map
/// for wildcard imports.
struct GlobalIndex {
    all_fqns: HashSet<String>,
    by_package: HashMap<String, HashMap<String, String>>,
}

impl GlobalIndex {
    fn build(files: &[JavaFile]) -> Self {
        let mut all_fqns = HashSet::new();
        let mut by_package: HashMap<String, HashMap<String, String>> = HashMap::new();
        for file in files {
            let pkg = file.package.as_deref().unwrap_or("");
            for ty in &file.types {
                let fqn = join_fqn(pkg, &ty.name);
                all_fqns.insert(fqn.clone());
                by_package
                    .entry(pkg.to_string())
                    .or_default()
                    .insert(ty.name.clone(), fqn.clone());
                collect_nested_fqns(ty, &fqn, &mut all_fqns);
            }
        }
        Self {
            all_fqns,
            by_package,
        }
    }
}

fn collect_nested_fqns(ty: &JavaType, parent_fqn: &str, out: &mut HashSet<String>) {
    for nested in &ty.nested_types {
        let fqn = format!("{parent_fqn}.{}", nested.name);
        out.insert(fqn.clone());
        collect_nested_fqns(nested, &fqn, out);
    }
}

fn join_fqn(pkg: &str, name: &str) -> String {
    if pkg.is_empty() {
        name.to_string()
    } else {
        format!("{pkg}.{name}")
    }
}

/// Simple-name → FQN map for one file's lexical type scope.
struct FileScope {
    by_simple_name: HashMap<String, String>,
}

impl FileScope {
    fn build(file: &JavaFile, index: &GlobalIndex) -> Self {
        let mut by_simple_name = HashMap::new();
        let (wildcards, explicit) = split_imports(&file.imports);

        // Lowest precedence: wildcard imports.
        for pkg in &wildcards {
            if let Some(pkg_types) = index.by_package.get(pkg.as_str()) {
                for (simple, fqn) in pkg_types {
                    by_simple_name.insert(simple.clone(), fqn.clone());
                }
            }
        }

        // Same-package types (covers this file's own top-level types too,
        // since they were added to `by_package` during index build).
        let own_pkg = file.package.as_deref().unwrap_or("");
        if let Some(pkg_types) = index.by_package.get(own_pkg) {
            for (simple, fqn) in pkg_types {
                by_simple_name.insert(simple.clone(), fqn.clone());
            }
        }

        // Highest precedence: explicit single-type imports — only honored when
        // we actually parsed the target file.
        for fqn in &explicit {
            if index.all_fqns.contains(fqn.as_str())
                && let Some(simple) = fqn.rsplit('.').next()
            {
                by_simple_name.insert(simple.to_string(), fqn.clone());
            }
        }

        Self { by_simple_name }
    }
}

/// Walks `file.imports`, splitting `[com.x, *, com.y.Z]`-style raw entries
/// into wildcard packages and explicit single-type imports.
///
/// The parser captures the `scoped_identifier` and the asterisk as separate
/// raw entries, so we reconstruct the grouping with one-step lookahead.
fn split_imports(imports: &[String]) -> (Vec<String>, Vec<String>) {
    let mut wildcards = Vec::new();
    let mut explicit = Vec::new();
    let mut iter = imports.iter().peekable();
    while let Some(entry) = iter.next() {
        if entry == "*" {
            continue;
        }
        if iter.peek().map(|s| s.as_str()) == Some("*") {
            wildcards.push(entry.clone());
            iter.next();
        } else {
            explicit.push(entry.clone());
        }
    }
    (wildcards, explicit)
}

fn resolve_type(ty: &mut JavaType, scope: &FileScope, index: &GlobalIndex) {
    for r in ty.extends.iter_mut() {
        resolve_type_ref(r, scope, index);
    }
    for r in ty.implements.iter_mut() {
        resolve_type_ref(r, scope, index);
    }
    for tp in ty.type_parameters.iter_mut() {
        for b in tp.bounds.iter_mut() {
            resolve_type_ref(b, scope, index);
        }
    }
    for f in ty.fields.iter_mut() {
        resolve_type_ref(&mut f.ty, scope, index);
    }
    for c in ty.constructors.iter_mut() {
        for a in c.args.iter_mut() {
            resolve_type_ref(&mut a.ty, scope, index);
        }
        for t in c.throws.iter_mut() {
            resolve_type_ref(t, scope, index);
        }
    }
    for m in ty.methods.iter_mut() {
        if let Some(rt) = m.return_type.as_mut() {
            resolve_type_ref(rt, scope, index);
        }
        for a in m.args.iter_mut() {
            resolve_type_ref(&mut a.ty, scope, index);
        }
        for t in m.throws.iter_mut() {
            resolve_type_ref(t, scope, index);
        }
    }
    for rc in ty.record_components.iter_mut() {
        resolve_type_ref(&mut rc.ty, scope, index);
    }
    for ae in ty.annotation_elements.iter_mut() {
        resolve_type_ref(&mut ae.return_type, scope, index);
        if let Some(dv) = ae.default_value.as_mut() {
            resolve_annotation_value(dv, scope, index);
        }
    }
    for nt in ty.nested_types.iter_mut() {
        resolve_type(nt, scope, index);
    }
}

fn resolve_type_ref(tref: &mut JavaTypeRef, scope: &FileScope, index: &GlobalIndex) {
    match tref {
        JavaTypeRef::Reference(r) => {
            r.resolved_fqn = resolve_reference(r, scope, index);
            for arg in r.args.iter_mut() {
                match arg {
                    JavaTypeArgument::Type(t) | JavaTypeArgument::Wildcard(t) => {
                        resolve_type_ref(t, scope, index);
                    }
                }
            }
        }
        JavaTypeRef::Array { element, .. } => resolve_type_ref(element, scope, index),
        JavaTypeRef::Wildcard { bound, .. } => {
            if let Some(bound) = bound {
                match bound {
                    JavaWildcardBound::Extends(t) | JavaWildcardBound::Super(t) => {
                        resolve_type_ref(t, scope, index);
                    }
                }
            }
        }
        JavaTypeRef::Annotated { inner, .. } => resolve_type_ref(inner, scope, index),
        JavaTypeRef::Primitive(_) | JavaTypeRef::Void | JavaTypeRef::Unsupported { .. } => {}
    }
}

fn resolve_annotation_value(
    value: &mut JavaAnnotationValue,
    scope: &FileScope,
    index: &GlobalIndex,
) {
    match value {
        JavaAnnotationValue::ClassLiteral(t) => resolve_type_ref(t, scope, index),
        JavaAnnotationValue::Array(values) => {
            for v in values.iter_mut() {
                resolve_annotation_value(v, scope, index);
            }
        }
        _ => {}
    }
}

fn resolve_reference(
    reference: &JavaReferenceType,
    scope: &FileScope,
    index: &GlobalIndex,
) -> Option<String> {
    let segments = collect_segments(reference);
    if segments.is_empty() {
        return None;
    }
    let joined = segments.join(".");
    if index.all_fqns.contains(&joined) {
        return Some(joined);
    }
    let first = &segments[0];
    let anchor = scope.by_simple_name.get(first)?;
    if segments.len() == 1 {
        return Some(anchor.clone());
    }
    let candidate = format!("{anchor}.{}", segments[1..].join("."));
    if index.all_fqns.contains(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn collect_segments(reference: &JavaReferenceType) -> Vec<String> {
    let mut segments = Vec::new();
    if let Some(qualifier) = &reference.qualifier {
        collect_qualifier_segments(qualifier, &mut segments);
    }
    segments.push(reference.name.clone());
    segments
}

fn collect_qualifier_segments(qualifier: &JavaTypeRef, out: &mut Vec<String>) {
    match qualifier {
        JavaTypeRef::Reference(r) => {
            if let Some(inner) = &r.qualifier {
                collect_qualifier_segments(inner, out);
            }
            out.push(r.name.clone());
        }
        JavaTypeRef::Annotated { inner, .. } => collect_qualifier_segments(inner, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::java_ast::parse_java_file;

    fn parse_all(sources: &[&str]) -> Vec<JavaFile> {
        sources
            .iter()
            .map(|s| parse_java_file(s).expect("parse"))
            .collect()
    }

    fn resolved(tref: &JavaTypeRef) -> Option<&str> {
        match tref {
            JavaTypeRef::Reference(r) => r.resolved_fqn.as_deref(),
            JavaTypeRef::Annotated { inner, .. } => resolved(inner),
            _ => None,
        }
    }

    #[test]
    fn resolves_explicit_import() {
        let mut files = parse_all(&[
            "package com.example.model; public class User {}",
            "package com.example.service; import com.example.model.User; \
             public class Service { private User user; }",
        ]);
        resolve_files(&mut files);
        let service = &files[1].types[0];
        assert_eq!(
            resolved(&service.fields[0].ty),
            Some("com.example.model.User")
        );
    }

    #[test]
    fn resolves_same_package_without_import() {
        let mut files = parse_all(&[
            "package x; public class A {}",
            "package x; public class B { A a; }",
        ]);
        resolve_files(&mut files);
        let b = &files[1].types[0];
        assert_eq!(resolved(&b.fields[0].ty), Some("x.A"));
    }

    #[test]
    fn resolves_wildcard_import() {
        let mut files = parse_all(&[
            "package com.foo; public class Foo {}",
            "package other; import com.foo.*; public class Bar { Foo f; }",
        ]);
        resolve_files(&mut files);
        let bar = &files[1].types[0];
        assert_eq!(resolved(&bar.fields[0].ty), Some("com.foo.Foo"));
    }

    #[test]
    fn resolves_nested_type_via_outer() {
        let mut files = parse_all(&[
            "package x; public class Outer { public static class Inner {} }",
            "package x; public class B { Outer.Inner field; }",
        ]);
        resolve_files(&mut files);
        let b = &files[1].types[0];
        assert_eq!(resolved(&b.fields[0].ty), Some("x.Outer.Inner"));
    }

    #[test]
    fn resolves_fully_qualified_inline_use() {
        let mut files = parse_all(&[
            "package com.example; public class Thing {}",
            "package other; public class B { com.example.Thing t; }",
        ]);
        resolve_files(&mut files);
        let b = &files[1].types[0];
        assert_eq!(resolved(&b.fields[0].ty), Some("com.example.Thing"));
    }

    #[test]
    fn external_types_stay_unresolved() {
        let mut files = parse_all(&["package a; public class A { String s; }"]);
        resolve_files(&mut files);
        let a = &files[0].types[0];
        assert_eq!(resolved(&a.fields[0].ty), None);
    }

    #[test]
    fn generic_args_resolve_independently_of_raw_type() {
        let mut files = parse_all(&[
            "package a; public class User {}",
            "package b; import a.User; import java.util.List; \
             public class Holder { List<User> users; }",
        ]);
        resolve_files(&mut files);
        let holder = &files[1].types[0];
        let field_ty = &holder.fields[0].ty;
        // Outer List is JDK, not in parsed set.
        assert_eq!(resolved(field_ty), None);
        let JavaTypeRef::Reference(reference) = field_ty else {
            panic!("expected reference type");
        };
        let JavaTypeArgument::Type(arg) = &reference.args[0] else {
            panic!("expected type argument");
        };
        assert_eq!(resolved(arg), Some("a.User"));
    }

    #[test]
    fn array_element_is_resolved() {
        let mut files = parse_all(&[
            "package a; public class User {}",
            "package b; import a.User; public class B { User[] users; }",
        ]);
        resolve_files(&mut files);
        let b = &files[1].types[0];
        let JavaTypeRef::Array { element, .. } = &b.fields[0].ty else {
            panic!("expected array type");
        };
        assert_eq!(resolved(element), Some("a.User"));
    }

    #[test]
    fn wildcard_bound_is_resolved() {
        let mut files = parse_all(&[
            "package a; public class Animal {}",
            "package b; import a.Animal; import java.util.List; \
             public class B { List<? extends Animal> list; }",
        ]);
        resolve_files(&mut files);
        let b = &files[1].types[0];
        let JavaTypeRef::Reference(list_ref) = &b.fields[0].ty else {
            panic!("expected reference");
        };
        let JavaTypeArgument::Wildcard(JavaTypeRef::Wildcard { bound, .. }) = &list_ref.args[0]
        else {
            panic!("expected wildcard argument");
        };
        let bound = bound.as_ref().expect("wildcard should have bound");
        let JavaWildcardBound::Extends(inner) = bound else {
            panic!("expected extends bound");
        };
        assert_eq!(resolved(inner), Some("a.Animal"));
    }

    #[test]
    fn same_file_reference_resolves() {
        let mut files = parse_all(&["package x; public class Foo { Bar b; } class Bar {}"]);
        resolve_files(&mut files);
        let foo = &files[0].types[0];
        assert_eq!(resolved(&foo.fields[0].ty), Some("x.Bar"));
    }
}
