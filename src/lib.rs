use std::{collections::BTreeMap, env};

use tracing::span;

mod formatting {
    use crate::SystemdLayer;

    pub fn build_span_name(span: &serde_json::Value) -> String {
        let name = span["name"].as_str().unwrap();

        let mut arguments_string = String::from("(");

        let arguments_and_fields = span["fields"].as_object().unwrap();
        for (key, value) in arguments_and_fields {
            arguments_string.push_str(&format!("{}: {}", key, value.as_str().unwrap()));
            if key != arguments_and_fields.keys().last().unwrap() {
                arguments_string.push_str(", ");
            }
        }

        arguments_string.push_str(")");

        return format!("{}{}", name, arguments_string);
    }

    pub fn prefix_from_level(level: &tracing::Level) -> u8 {
        match level {
            &tracing::Level::TRACE => 7,
            &tracing::Level::DEBUG => 6,
            &tracing::Level::INFO => 5,
            &tracing::Level::WARN => 4,
            &tracing::Level::ERROR => 3,
        }
    }

    impl SystemdLayer {
        fn build_span_chain(&self, output: &serde_json::Value) -> String {
            let target = output["target"].as_str().unwrap();
            let mut span_chain = String::new();

            let mut spans = output["spans"].as_array().unwrap().clone();
            spans.remove(spans.len() - 1);

            for span in spans {
                let span_name = build_span_name(&span);
                span_chain.push_str(&span_name);
                span_chain.push_str(self.span_separator);
            }

            return span_chain;
        }

        pub fn build_full_string(&self, output: &serde_json::Value) -> String {
            let span_chain = self.build_span_chain(&output);

            // Getting the function name from the last span
            let spans = output["spans"].as_array().unwrap();
            let function_name = build_span_name(spans.last().unwrap());

            let message = match output["fields"]["message"].as_str() {
                Some(message) => message,
                None => "",
            };

            let mut full_string = format!("{}{}", span_chain, function_name);

            if message.len() > 0 {
                full_string.push_str(self.message_separator);
                full_string.push_str(message);
            }

            let mut fields = output["fields"].as_object().unwrap().clone();
            fields.remove("message");
            if fields.len() > 0 {
                full_string.push_str(self.message_separator);
                full_string.push_str(&serde_json::to_string(&fields).unwrap());
            }

            return full_string;
        }
    }
}

use formatting::*;

struct SystemdFieldStorage(BTreeMap<String, serde_json::Value>);
struct SystemdVisitor<'a>(&'a mut BTreeMap<String, serde_json::Value>);
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

pub struct SystemdLayer {
    log_thread_id: bool,
    filter_crate: bool,
    span_separator: &'static str,
    message_separator: &'static str,
}
impl SystemdLayer {
    pub fn new() -> Self {
        Self {
            log_thread_id: false,
            filter_crate: false,
            span_separator: "::",
            message_separator: ": ",
        }
    }

    pub fn log_thread_id(mut self, log_thread_id: bool) -> Self {
        self.log_thread_id = log_thread_id;
        self
    }

    pub fn filter_crate(mut self, filter_crate: bool) -> Self {
        self.filter_crate = filter_crate;
        self
    }

    pub fn separate_spans_with(mut self, span_separator: &'static str) -> Self {
        self.span_separator = span_separator;
        self
    }

    pub fn separate_message_with(mut self, message_separator: &'static str) -> Self {
        self.message_separator = message_separator;
        self
    }
}

fn run_systemd_cat_command(level: &tracing::Level, target: &'static str, message: String) {
    let prefix = prefix_from_level(level);
    let command = std::process::Command::new("echo")
        .arg(message)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    std::process::Command::new("systemd-cat")
        .stdin(command.stdout.unwrap())
        .arg("-t")
        .arg(target)
        .arg("-p")
        .arg(format!("{}", prefix))
        .spawn()
        .unwrap();
}
impl<S> tracing_subscriber::Layer<S> for SystemdLayer
where
    S: tracing::Subscriber,
    S: for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // Filter out logs from other crates if the filter_crate option is set
        let current_crate = std::env::current_exe().unwrap();
        let current_crate = current_crate.file_stem().unwrap().to_str().unwrap();
        if self.filter_crate && event.metadata().target() != current_crate {
            return;
        }

        let scope = ctx.event_scope(event).unwrap();
        let mut spans = vec![];

        for span in scope.from_root() {
            let extensions = span.extensions();
            let storage = extensions.get::<SystemdFieldStorage>().unwrap();
            let data = &storage.0;
            spans.push(serde_json::json!({
                "target": event.metadata().target(),
                "name": span.name(),
                "level": event.metadata().level().to_string(),
                "fields": data,
            }));
        }

        let mut fields = BTreeMap::new();
        let mut visitor = SystemdVisitor(&mut fields);
        event.record(&mut visitor);

        let output = serde_json::json!({
            "target": event.metadata().target(),
            "name": event.metadata().name(),
            "level": event.metadata().level().to_string(),
            "fields": fields,
            "spans": spans,
        });

        run_systemd_cat_command(
            &event.metadata().level(),
            &event.metadata().target(),
            self.build_full_string(&output),
        );
    }

    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = BTreeMap::new();
        let mut visitor = SystemdVisitor(&mut fields);
        attrs.record(&mut visitor);

        let storage = SystemdFieldStorage(fields);
        let span = ctx.span(id).unwrap();
        let mut extensions = span.extensions_mut();
        extensions.insert(storage);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
