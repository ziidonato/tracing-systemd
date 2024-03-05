use crate::SystemdLayer;

#[cfg(feature = "colored")]
use colored::Colorize;

#[cfg(feature = "sd-journal")]
use sd_journal::Level;

///Converts a tracing level to a sd-journal level<br>
///Only available if the feature "sd-journal" is enabled
#[cfg(feature = "sd-journal")]
pub(crate) fn journal_level_from_tracing_level(level: &tracing::Level) -> Level {
    match level {
        &tracing::Level::TRACE => Level::Debug,
        &tracing::Level::DEBUG => Level::Info,
        &tracing::Level::INFO => Level::Notice,
        &tracing::Level::WARN => Level::Warning,
        &tracing::Level::ERROR => Level::Error,
        _ => Level::Info,
    }
}

///Generates the journald prefix from a tracing level<br>
pub(crate) fn prefix_from_tracing_level(level: &tracing::Level) -> String {
    match level {
        &tracing::Level::TRACE => "<7>".to_string(),
        &tracing::Level::DEBUG => "<6>".to_string(),
        &tracing::Level::INFO => "<5>".to_string(),
        &tracing::Level::WARN => "<4>".to_string(),
        &tracing::Level::ERROR => "<3>".to_string(),
    }
}

impl SystemdLayer {
    ///Builds the span name<br/>
    ///includes arguments, fields, and the function name
    pub(crate) fn build_span_name(&self, span: &serde_json::Value) -> String {
        let name = span["name"].as_str().unwrap();

        let mut arguments_string = String::new();
        arguments_string.push_str(self.get_function_bracket_left());

        let arguments_and_fields = span["fields"].as_object().unwrap();
        for (key, value) in arguments_and_fields {
            arguments_string.push_str(&format!(
                "{}{}{}",
                key,
                self.get_arguments_equality(),
                value.as_str().unwrap()
            ));
            if key != arguments_and_fields.keys().last().unwrap() {
                arguments_string.push_str(self.get_arguments_separator());
            }
        }

        arguments_string.push_str(self.get_function_bracket_right());

        return format!("{}{}", name, arguments_string);
    }

    /// Builds the span chain
    /// Responsible for putting the spans together
    fn build_span_chain(&self, output: &serde_json::Value) -> String {
        let target = output["target"].as_str().unwrap();
        let mut span_chain = String::new();

        let mut spans = output["spans"].as_array().unwrap().clone();
        spans.remove(spans.len() - 1);

        for span in spans {
            let span_name = self.build_span_name(&span);
            span_chain.push_str(&span_name);
            span_chain.push_str(self.get_span_separator());
        }

        if self.get_log_target() {
            span_chain = format!("{}{}{}", target, self.get_span_separator(), span_chain);
        }

        return span_chain;
    }

    /// Builds the full string to be logged
    pub(crate) fn build_full_string(&self, output: &serde_json::Value) -> String {
        let span_chain = self.build_span_chain(&output);

        let spans = output["spans"].as_array().unwrap();
        let function_name = self.build_span_name(spans.last().unwrap());

        let level = output["level"].as_str().unwrap().to_string();

        #[cfg(feature = "colored")]
        let colored_level = match level.as_str() {
            "TRACE" => level.magenta(),
            "DEBUG" => level.blue(),
            "INFO" => level.green(),
            "WARN" => level.yellow(),
            "ERROR" => level.red(),
            _ => level.normal(),
        };

        let thread_id = std::thread::current().id();
        let thread_id_str = format!("{:?}", thread_id);
        let thread_id_int = thread_id_str.split("(").collect::<Vec<&str>>()[1]
            .split(")")
            .collect::<Vec<&str>>()[0];

        let mut full_string = format!(
            "{}{}{}{}",
            level,
            self.get_level_separator(),
            span_chain,
            function_name
        );

        if self.get_log_thread_id() {
            full_string = format!(
                "{}{}{}{}{}{}{}",
                level,
                self.get_level_separator(),
                self.get_thread_id_prefix(),
                thread_id_int,
                self.get_thread_id_suffix(),
                span_chain,
                function_name
            );
        }

        #[cfg(feature = "colored")]
        if self.get_use_color() {
            if self.get_log_thread_id() {
                full_string = format!(
                    "{}{}{}{}{}{}{}",
                    colored_level,
                    self.get_level_separator(),
                    self.get_thread_id_prefix(),
                    thread_id_int,
                    self.get_thread_id_suffix(),
                    span_chain,
                    function_name
                );
            } else {
                full_string = format!(
                    "{}{}{}{}",
                    colored_level,
                    self.get_level_separator(),
                    span_chain,
                    function_name
                );
            }
        }

        let message = match output["fields"]["message"].as_str() {
            Some(message) => message,
            None => "",
        };

        if message.len() > 0 {
            full_string.push_str(self.get_message_separator());
            full_string.push_str(message);
        }

        let mut fields = output["fields"].as_object().unwrap().clone();
        fields.remove("message");
        if fields.len() > 0 {
            full_string.push_str(self.get_message_separator());
            full_string.push_str(&serde_json::to_string(&fields).unwrap());
        }

        return full_string;
    }
}
