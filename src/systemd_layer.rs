use std::collections::BTreeMap;

/// Contains the FieldStorage struct and the SystemdVisitor struct
mod helper_structs;

mod impls;

use crate::formatting::*;

use helper_structs::*;
#[cfg(feature = "sd-journal")]
use sd_journal::*;

///A configurable tracing-subscriber layer compatible with journald.
///
///The layer used to format and log events. Can be configured to log to stdout, or directly to journald using the `sd-journal` feature.
///```rust  
///let systemd_layer = SystemdLayer::new()
///    .with_target(true)
///    .use_level_prefix(false)
///    .use_color(true);
///```
pub struct SystemdLayer {
    #[cfg(feature = "colored")]
    use_color: bool,

    log_thread_id: bool,
    span_separator: &'static str,
    message_separator: &'static str,
    level_separator: &'static str,
    log_target: bool,
    #[cfg(feature = "sd-journal")]
    use_sd_journal: bool,
    function_bracket_left: &'static str,
    function_bracket_right: &'static str,
    arguments_equality: &'static str,
    arguments_separator: &'static str,
    use_level_prefix: bool,
    thread_id_prefix: &'static str,
    thread_id_suffix: &'static str,
}

// Implementation of the Layer trait
impl<S> tracing_subscriber::Layer<S> for SystemdLayer
where
    S: tracing::Subscriber,
    S: for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
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

        let full_string = self.build_full_string(&output);

        #[cfg(feature = "sd-journal")]
        {
            match self.get_use_sd_journal() {
                true => {
                    let level = journal_level_from_tracing_level(event.metadata().level());
                    Journal::log_message(level, full_string).unwrap();
                }

                false => match self.get_use_level_prefix() {
                    true => {
                        let prefix = prefix_from_tracing_level(event.metadata().level());
                        println!("{}{}", prefix, full_string);
                    }
                    false => {
                        println!("{}", full_string);
                    }
                },
            }
        }

        #[cfg(not(feature = "sd-journal"))]
        {
            match self.get_use_level_prefix() {
                true => {
                    let prefix = prefix_from_tracing_level(event.metadata().level());
                    println!("{}{}", prefix, full_string)
                }
                false => {
                    println!("{}", full_string);
                }
            }

            println!("{}", full_string);
        }
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
