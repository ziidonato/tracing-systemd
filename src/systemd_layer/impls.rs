use crate::SystemdLayer;

// Setter methods
impl SystemdLayer {
    pub fn new() -> Self {
        Self {
            log_thread_id: false,
            filter_crate: false,
            span_separator: "::",
            message_separator: ": ",
            log_target: false,
            function_bracket_left: "(",
            function_bracket_right: ")",
            arguments_equality: ": ",
            arguments_separator: ", ",

            #[cfg(feature = "sd-journal")]
            use_sd_journal: true,
            #[cfg(feature = "colored")]
            use_color: true,

            use_level_prefix: true,
        }
    }

    pub fn log_thread_id(mut self, log_thread_id: bool) -> Self {
        Self {
            log_thread_id,
            ..self
        }
    }

    pub fn filter_crate(self, filter_crate: bool) -> Self {
        Self {
            filter_crate,
            ..self
        }
    }

    pub fn separate_spans_with(self, span_separator: &'static str) -> Self {
        Self {
            span_separator,
            ..self
        }
    }

    pub fn separate_message_with(self, message_separator: &'static str) -> Self {
        Self {
            message_separator,
            ..self
        }
    }

    pub fn with_target(self, display_target: bool) -> Self {
        Self {
            log_target: display_target,
            ..self
        }
    }

    pub fn function_bracket_left(self, function_bracket_left: &'static str) -> Self {
        Self {
            function_bracket_left,
            ..self
        }
    }

    pub fn function_bracket_right(self, function_bracket_right: &'static str) -> Self {
        Self {
            function_bracket_right,
            ..self
        }
    }

    pub fn arguments_equality(self, arguments_equality: &'static str) -> Self {
        Self {
            arguments_equality,
            ..self
        }
    }

    pub fn arguments_separator(self, arguments_separator: &'static str) -> Self {
        Self {
            arguments_separator,
            ..self
        }
    }

    #[cfg(feature = "sd-journal")]
    pub fn use_sd_journal(self, use_sd_journal: bool) -> Self {
        Self {
            use_sd_journal,
            ..self
        }
    }

    pub fn use_level_prefix(self, use_level_prefix: bool) -> Self {
        Self {
            use_level_prefix,
            ..self
        }
    }

    pub fn use_color(self, use_color: bool) -> Self {
        Self { use_color, ..self }
    }
}

// Setter methods
impl SystemdLayer {
    pub fn get_filter_crate(&self) -> bool {
        self.filter_crate
    }

    pub fn get_log_thread_id(&self) -> bool {
        self.log_thread_id
    }

    pub fn get_span_separator(&self) -> &'static str {
        self.span_separator
    }

    pub fn get_message_separator(&self) -> &'static str {
        self.message_separator
    }

    pub fn get_log_target(&self) -> bool {
        self.log_target
    }

    pub fn get_function_bracket_left(&self) -> &'static str {
        self.function_bracket_left
    }

    pub fn get_function_bracket_right(&self) -> &'static str {
        self.function_bracket_right
    }

    pub fn get_arguments_equality(&self) -> &'static str {
        self.arguments_equality
    }

    pub fn get_arguments_separator(&self) -> &'static str {
        self.arguments_separator
    }

    #[cfg(feature = "sd-journal")]
    pub fn get_use_sd_journal(&self) -> bool {
        self.use_sd_journal
    }

    pub fn get_use_level_prefix(&self) -> bool {
        self.use_level_prefix
    }

    #[cfg(feature = "colored")]
    pub fn get_use_color(&self) -> bool {
        self.use_color
    }
}
