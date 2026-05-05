//! Field visitor and value storage used by [`SystemdLayer`](crate::SystemdLayer).
//!
//! `tracing` events and spans expose typed fields through the
//! [`Visit`](tracing::field::Visit) trait. We capture each value into a
//! [`FieldValue`] and accumulate them in a [`FieldMap`] keyed by field name.

use std::collections::BTreeMap;
use std::fmt;

use tracing::field::{Field, Visit};

/// Captured value of a `tracing` field.
///
/// Strings, debug-formatted, and error-formatted values are owned because
/// the visit callbacks borrow them with limited lifetimes.
#[derive(Debug, Clone)]
pub(crate) enum FieldValue {
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    Str(String),
    Debug(String),
    Error(String),
}

impl fmt::Display for FieldValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(v) => fmt::Display::fmt(v, f),
            Self::I64(v) => fmt::Display::fmt(v, f),
            Self::U64(v) => fmt::Display::fmt(v, f),
            Self::F64(v) => fmt::Display::fmt(v, f),
            Self::Str(v) | Self::Debug(v) | Self::Error(v) => f.write_str(v),
        }
    }
}

/// Map keyed by field name. `tracing` interns names as `&'static str`,
/// so we can store them by reference without allocation.
pub(crate) type FieldMap = BTreeMap<&'static str, FieldValue>;

/// Per-span storage attached to each [`Span`](tracing::Span)'s extensions
/// so its fields are available when an event fires inside it.
#[derive(Debug, Default)]
pub(crate) struct FieldStorage(pub(crate) FieldMap);

/// Visitor that fills a [`FieldMap`] from a tracing `Event` or `Attributes`.
pub(crate) struct FieldVisitor<'a> {
    fields: &'a mut FieldMap,
}

impl<'a> FieldVisitor<'a> {
    pub(crate) fn new(fields: &'a mut FieldMap) -> Self {
        Self { fields }
    }
}

impl Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.fields
            .insert(field.name(), FieldValue::Debug(format!("{value:?}")));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.insert(field.name(), FieldValue::Bool(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.insert(field.name(), FieldValue::F64(value));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(field.name(), FieldValue::I64(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(field.name(), FieldValue::U64(value));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name(), FieldValue::Str(value.to_owned()));
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.fields
            .insert(field.name(), FieldValue::Error(value.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_value_displays_each_variant() {
        assert_eq!(FieldValue::Bool(true).to_string(), "true");
        assert_eq!(FieldValue::I64(-3).to_string(), "-3");
        assert_eq!(FieldValue::U64(7).to_string(), "7");
        assert_eq!(FieldValue::F64(1.5).to_string(), "1.5");
        assert_eq!(FieldValue::Str("hi".to_owned()).to_string(), "hi");
        assert_eq!(FieldValue::Debug("Foo { x: 1 }".to_owned()).to_string(), "Foo { x: 1 }");
        assert_eq!(FieldValue::Error("oops".to_owned()).to_string(), "oops");
    }
}
