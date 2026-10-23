//! Accessors for DynamicMessage fields by (obfuscated) name, so modules need no generated code.
use prost_reflect::{DynamicMessage, Kind, ReflectMessage, Value};

pub fn as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::I32(x) => Some(*x as i64),
        Value::I64(x) => Some(*x),
        Value::U32(x) => Some(*x as i64),
        Value::U64(x) => Some(*x as i64),
        Value::EnumNumber(x) => Some(*x as i64),
        Value::Bool(b) => Some(*b as i64),
        _ => None,
    }
}

/// Integer field by name (0 when unset or not an integer, like protobuf defaults).
pub fn int(m: &DynamicMessage, name: &str) -> i64 {
    m.get_field_by_name(name).and_then(|v| as_i64(&v)).unwrap_or(0)
}

/// Every single (not repeated) integer field of the message's type with its value (0 when unset): for messages whose
/// integer fields have no reliable names and are told apart by their values.
pub fn scalar_ints(m: &DynamicMessage) -> Vec<(String, i64)> {
    m.descriptor()
        .fields()
        .filter(|f| !f.is_list() && matches!(f.kind(), Kind::Int32 | Kind::Int64 | Kind::Uint32 | Kind::Uint64 | Kind::Sint32 | Kind::Sint64))
        .map(|f| (f.name().to_string(), int(m, f.name())))
        .collect()
}

pub fn ints(m: &DynamicMessage, names: &[&str]) -> Vec<i64> {
    names.iter().map(|n| int(m, n)).collect()
}

pub fn int_list(m: &DynamicMessage, name: &str) -> Vec<i64> {
    match m.get_field_by_name(name).as_deref() {
        Some(Value::List(l)) => l.iter().filter_map(as_i64).collect(),
        _ => vec![],
    }
}

pub fn messages(m: &DynamicMessage, name: &str) -> Vec<DynamicMessage> {
    match m.get_field_by_name(name).as_deref() {
        Some(Value::List(l)) => l.iter().filter_map(|v| v.as_message().cloned()).collect(),
        _ => vec![],
    }
}

/// The set member of a oneof (field name, value), if any.
pub fn oneof_value(m: &DynamicMessage, oneof: &str) -> Option<(String, Value, Kind)> {
    let o = m.descriptor().oneofs().find(|o| o.name() == oneof)?;
    let set = o.fields().find(|f| m.has_field(f));
    set.map(|f| (f.name().to_string(), m.get_field(&f).into_owned(), f.kind()))
}
