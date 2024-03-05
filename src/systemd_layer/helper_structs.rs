use std::collections::BTreeMap;

pub struct SystemdFieldStorage(pub BTreeMap<String, serde_json::Value>);
pub struct SystemdVisitor<'a>(pub &'a mut BTreeMap<String, serde_json::Value>);
impl tracing::field::Visit for SystemdVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{:?}", value).into());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0
            .insert(field.name().to_string(), value.to_string().into());
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0
            .insert(field.name().to_string(), value.to_string().into());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0
            .insert(field.name().to_string(), value.to_string().into());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0
            .insert(field.name().to_string(), value.to_string().into());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0
            .insert(field.name().to_string(), value.to_string().into());
    }

    fn record_error(&mut self, field: &tracing::field::Field, value: &dyn std::error::Error) {
        self.0
            .insert(field.name().to_string(), value.to_string().into());
    }
}
