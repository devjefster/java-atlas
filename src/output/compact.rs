//! Shared compaction rules for human- and AI-oriented output formats.

use crate::java_ast::{JavaConstructor, JavaField, JavaMethod, JavaType};

pub(super) fn field_accessor_labels(ty: &JavaType, field: &JavaField) -> Vec<&'static str> {
    let mut getter = false;
    let mut setter = false;
    let mut add = false;
    for method in &ty.methods {
        match classify_accessor(method, field, &ty.name) {
            Some(AccessorKind::Getter) => getter = true,
            Some(AccessorKind::Setter) => setter = true,
            Some(AccessorKind::Add) => add = true,
            None => {}
        }
    }
    let mut labels = Vec::new();
    if getter {
        labels.push("getter");
    }
    if setter {
        labels.push("setter");
    }
    if add {
        labels.push("add");
    }
    labels
}

pub(super) fn is_compacted_accessor(method: &JavaMethod, ty: &JavaType) -> bool {
    ty.fields
        .iter()
        .any(|field| classify_accessor(method, field, &ty.name).is_some())
}

pub(super) fn is_boring_no_arg_constructor(cons: &JavaConstructor) -> bool {
    cons.args.is_empty()
        && cons.documentation.is_none()
        && cons.annotations.is_empty()
        && cons.type_parameters.is_empty()
        && cons.throws.is_empty()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessorKind {
    Getter,
    Setter,
    Add,
}

fn classify_accessor(
    method: &JavaMethod,
    field: &JavaField,
    declaring_type: &str,
) -> Option<AccessorKind> {
    if !is_plain_method(method) {
        return None;
    }

    let property = property_suffix(&field.name);
    let field_ty = field.ty.to_string();

    if method.name == format!("get{property}")
        && method.args.is_empty()
        && method
            .return_type
            .as_ref()
            .is_some_and(|return_type| return_type.to_string() == field_ty)
    {
        return Some(AccessorKind::Getter);
    }

    if method.name == format!("is{property}")
        && method.args.is_empty()
        && is_boolean_type(&field_ty)
        && method
            .return_type
            .as_ref()
            .is_some_and(|return_type| is_boolean_type(&return_type.to_string()))
    {
        return Some(AccessorKind::Getter);
    }

    if method.name == format!("set{property}")
        && method.args.len() == 1
        && method.args[0].ty.to_string() == field_ty
        && method.return_type.as_ref().is_some_and(|return_type| {
            is_void_or_declaring_type(return_type.to_string(), declaring_type)
        })
    {
        return Some(AccessorKind::Setter);
    }

    if method.name == format!("add{property}")
        && method.args.len() == 1
        && method.args[0].ty.to_string() == field_ty
        && method.return_type.as_ref().is_some_and(|return_type| {
            is_void_or_declaring_type(return_type.to_string(), declaring_type)
        })
    {
        return Some(AccessorKind::Add);
    }

    None
}

fn is_plain_method(method: &JavaMethod) -> bool {
    method.documentation.is_none()
        && method.annotations.is_empty()
        && method.type_parameters.is_empty()
        && method.throws.is_empty()
}

fn property_suffix(field_name: &str) -> String {
    let mut chars = field_name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}

fn is_boolean_type(ty: &str) -> bool {
    matches!(ty, "boolean" | "Boolean")
}

fn is_void_or_declaring_type(return_type: String, declaring_type: &str) -> bool {
    return_type == "void" || return_type == declaring_type
}
