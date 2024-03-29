use crate::SystemdLayer;

impl SystemdLayer {
    ///Creates a new `SystemdLayer` with default configuration.
    pub fn new() -> Self {
        Self {
            log_thread_id: false,
            span_separator: "::",
            message_separator: ": ",
            log_target: false,
            function_bracket_left: "(",
            function_bracket_right: ")",
            arguments_equality: ": ",
            arguments_separator: ", ",
            use_level_prefix: true,
            level_separator: " ",
            thread_id_prefix: "[",
            thread_id_suffix: "] ",

            #[cfg(feature = "sd-journal")]
            use_sd_journal: true,
            #[cfg(feature = "colored")]
            use_color: true,
        }
    }

    ///Sets whether or not to include thread IDs (default: false)
    pub fn with_thread_ids(self, log_thread_id: bool) -> Self {
        Self {
            log_thread_id,
            ..self
        }
    }

    ///Sets the span separator (default: "::")
    pub fn separate_spans_with(self, span_separator: &'static str) -> Self {
        Self {
            span_separator,
            ..self
        }
    }

    ///Sets the message separator (default: ": ")
    pub fn separate_message_with(self, message_separator: &'static str) -> Self {
        Self {
            message_separator,
            ..self
        }
    }

    ///Sets whether or not to include the target (default: false)
    pub fn with_target(self, display_target: bool) -> Self {
        Self {
            log_target: display_target,
            ..self
        }
    }

    ///Sets the left bracket for function names (default: "(")
    pub fn function_bracket_left(self, function_bracket_left: &'static str) -> Self {
        Self {
            function_bracket_left,
            ..self
        }
    }

    ///Sets the right bracket for function names (default: ")")
    pub fn function_bracket_right(self, function_bracket_right: &'static str) -> Self {
        Self {
            function_bracket_right,
            ..self
        }
    }

    ///Sets the equality sign for arguments (default: ": ")
    pub fn arguments_equality(self, arguments_equality: &'static str) -> Self {
        Self {
            arguments_equality,
            ..self
        }
    }

    ///Sets the separator for arguments (default: ", ")
    pub fn arguments_separator(self, arguments_separator: &'static str) -> Self {
        Self {
            arguments_separator,
            ..self
        }
    }

    ///Sets the separator for levels (default: " ")
    pub fn level_separator(self, level_separator: &'static str) -> Self {
        Self {
            level_separator,
            ..self
        }
    }

    ///Sets the prefix for thread IDs (default: "[")
    pub fn thread_id_prefix(self, thread_id_prefix: &'static str) -> Self {
        Self {
            thread_id_prefix,
            ..self
        }
    }

    ///Sets the suffix for thread IDs (default: "] ")
    pub fn thread_id_suffix(self, thread_id_suffix: &'static str) -> Self {
        Self {
            thread_id_suffix,
            ..self
        }
    }

    ///Sets whether or not to use sd_journal to write logs (default: true)
    #[cfg(feature = "sd-journal")]
    pub fn use_sd_journal(self, use_sd_journal: bool) -> Self {
        Self {
            use_sd_journal,
            ..self
        }
    }

    ///Sets whether or not to use level prefixes (default: true)
    pub fn use_level_prefix(self, use_level_prefix: bool) -> Self {
        Self {
            use_level_prefix,
            ..self
        }
    }

    ///Sets whether or not to use color
    #[cfg(feature = "colored")]
    pub fn use_color(self, use_color: bool) -> Self {
        Self { use_color, ..self }
    }
}

impl SystemdLayer {
    pub(crate) fn get_log_thread_id(&self) -> bool {
        self.log_thread_id
    }

    pub(crate) fn get_span_separator(&self) -> &'static str {
        self.span_separator
    }

    pub(crate) fn get_message_separator(&self) -> &'static str {
        self.message_separator
    }

    pub(crate) fn get_log_target(&self) -> bool {
        self.log_target
    }

    pub(crate) fn get_function_bracket_left(&self) -> &'static str {
        self.function_bracket_left
    }

    pub(crate) fn get_function_bracket_right(&self) -> &'static str {
        self.function_bracket_right
    }

    pub(crate) fn get_arguments_equality(&self) -> &'static str {
        self.arguments_equality
    }

    pub(crate) fn get_arguments_separator(&self) -> &'static str {
        self.arguments_separator
    }

    #[cfg(feature = "sd-journal")]
    pub(crate) fn get_use_sd_journal(&self) -> bool {
        self.use_sd_journal
    }

    pub(crate) fn get_use_level_prefix(&self) -> bool {
        self.use_level_prefix
    }

    pub(crate) fn get_level_separator(&self) -> &'static str {
        self.level_separator
    }

    #[cfg(feature = "colored")]
    pub(crate) fn get_use_color(&self) -> bool {
        self.use_color
    }

    pub(crate) fn get_thread_id_prefix(&self) -> &'static str {
        self.thread_id_prefix
    }

    pub(crate) fn get_thread_id_suffix(&self) -> &'static str {
        self.thread_id_suffix
    }
}
