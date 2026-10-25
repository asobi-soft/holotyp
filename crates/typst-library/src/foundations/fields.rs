//! Fields on values.

use ecow::{EcoString, eco_format};

use crate::diag::StrResult;
use crate::foundations::{CastInfo, IntoValue, Reflect, Smart, Type, Value, Version};
use crate::layout::{Alignment, Length, Ratio, Rel};
use crate::visualize::{DashPattern, LineCap, LineJoin, Paint, Stroke};

/// A named field on a value.
///
/// Unlike a type's scope, these are read directly off the value and cannot be
/// set.
#[derive(Debug, Clone, Copy)]
pub struct FieldInfo {
    /// The field's name.
    pub name: &'static str,
    /// What the field's value can be cast into.
    pub output: fn() -> CastInfo,
}

/// Try to access a field on a value.
///
/// This function is exclusively for types which have predefined fields, such as
/// stroke and length.
pub(crate) fn field(value: &Value, field: &str) -> StrResult<Value> {
    let ty = value.ty();
    let nope = || Err(no_fields(ty));
    let missing = || Err(missing_field(ty, field));

    // Special cases, such as module and dict, are handled by Value itself
    let result = match value {
        Value::Version(version) => match version.component(field) {
            Ok(i) => i.into_value(),
            Err(_) => return missing(),
        },
        Value::Length(length) => match field {
            "em" => length.em.get().into_value(),
            "abs" => length.abs.into_value(),
            _ => return missing(),
        },
        Value::Relative(rel) => match field {
            "ratio" => rel.rel.into_value(),
            "length" => rel.abs.into_value(),
            _ => return missing(),
        },
        Value::Dyn(dynamic) => {
            if let Some(stroke) = dynamic.downcast::<Stroke>() {
                match field {
                    "paint" => stroke.paint.clone().into_value(),
                    "thickness" => stroke.thickness.into_value(),
                    "cap" => stroke.cap.into_value(),
                    "join" => stroke.join.into_value(),
                    "dash" => stroke.dash.clone().into_value(),
                    "miter-limit" => {
                        stroke.miter_limit.map(|limit| limit.get()).into_value()
                    }
                    _ => return missing(),
                }
            } else if let Some(align) = dynamic.downcast::<Alignment>() {
                match field {
                    "x" => align.x().into_value(),
                    "y" => align.y().into_value(),
                    _ => return missing(),
                }
            } else {
                return nope();
            }
        }
        _ => return nope(),
    };

    Ok(result)
}

/// The error message for a type not supporting field access.
#[cold]
fn no_fields(ty: Type) -> EcoString {
    eco_format!("cannot access fields on type {ty}")
}

/// The missing field error message.
#[cold]
fn missing_field(ty: Type, field: &str) -> EcoString {
    eco_format!("{ty} does not contain field \"{field}\"")
}

/// List the available fields for a type.
pub fn fields_on(ty: Type) -> &'static [FieldInfo] {
    fn output<T: Reflect + ?Sized>() -> CastInfo {
        T::output()
    }

    if ty == Type::of::<Version>() {
        &[
            FieldInfo { name: "major", output: output::<i64> },
            FieldInfo { name: "minor", output: output::<i64> },
            FieldInfo { name: "patch", output: output::<i64> },
        ]
    } else if ty == Type::of::<Length>() {
        &[
            FieldInfo { name: "em", output: output::<f64> },
            FieldInfo { name: "abs", output: output::<Length> },
        ]
    } else if ty == Type::of::<Rel>() {
        &[
            FieldInfo { name: "ratio", output: output::<Ratio> },
            FieldInfo { name: "length", output: output::<Length> },
        ]
    } else if ty == Type::of::<Stroke>() {
        &[
            FieldInfo { name: "paint", output: output::<Smart<Paint>> },
            FieldInfo { name: "thickness", output: output::<Smart<Length>> },
            FieldInfo { name: "cap", output: output::<Smart<LineCap>> },
            FieldInfo { name: "join", output: output::<Smart<LineJoin>> },
            FieldInfo {
                name: "dash",
                output: output::<Smart<Option<DashPattern>>>,
            },
            FieldInfo { name: "miter-limit", output: output::<Smart<f64>> },
        ]
    } else if ty == Type::of::<Alignment>() {
        &[
            FieldInfo { name: "x", output: output::<Option<Alignment>> },
            FieldInfo { name: "y", output: output::<Option<Alignment>> },
        ]
    } else {
        &[]
    }
}
