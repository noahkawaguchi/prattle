pub enum Command<'a> {
    Empty,
    Quit,
    Help,
    Who,
    Action(&'a str),
    Msg(&'a str),
}

pub const COMMAND_HELP: &[u8] = b"
/quit             Leave the server
/help             Show this message
/who              List online users
/action <action>  Broadcast an action, e.g. /action waves

[anything else]   Send a regular message

";

impl<'a> Command<'a> {
    pub fn parse(input: &'a str) -> Self {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            Self::Empty
        } else if trimmed == "/quit" {
            Self::Quit
        } else if trimmed == "/help" {
            Self::Help
        } else if trimmed == "/who" {
            Self::Who
        } else if let Some(action) = trimmed.strip_prefix("/action ") {
            Self::Action(action)
        } else {
            Self::Msg(trimmed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_strings() {
        for input in ["", " ", "   ", "\t", "\n", " \t \n "] {
            assert!(
                matches!(Command::parse(input), Command::Empty),
                "expected Empty command for {input}"
            );
        }
    }

    #[test]
    fn parses_quit_command() {
        for input in ["/quit", "  /quit  ", "/quit\n"] {
            assert!(
                matches!(Command::parse(input), Command::Quit),
                "expected Quit command for {input}"
            );
        }
    }

    #[test]
    fn parses_who_command() {
        for input in ["/who", "  /who  ", "/who\n"] {
            assert!(
                matches!(Command::parse(input), Command::Who),
                "expected Who command for {input}"
            );
        }
    }

    #[test]
    fn parses_action_command() {
        for (input, expected_action) in [
            ("/action waves hello", "waves hello"),
            // Leading/trailing whitespace on the input is trimmed
            ("  /action does something  ", "does something"),
            ("/action jumps", "jumps"),
            // Internal spaces in the action text are preserved
            ("/action does   something", "does   something"),
        ] {
            assert!(
                matches!(
                    Command::parse(input),
                    Command::Action(action) if action == expected_action
                ),
                "expected Action(\"{expected_action}\") for {input}"
            );
        }
    }

    #[test]
    fn parses_action_without_text_as_message() {
        // "/action" without trailing space and text is treated as a regular message
        // "/action " with trailing space gets trimmed to "/action", also a message
        for input in ["/action", "/action "] {
            assert!(
                matches!(Command::parse(input), Command::Msg(msg) if msg == "/action"),
                "expected Msg(\"/action\") for {input}"
            );
        }
    }

    #[test]
    fn parses_regular_messages() {
        for (input, expected_msg) in [
            ("Hello, world!", "Hello, world!"),
            // Leading/trailing whitespace is trimmed
            ("  This is a message  ", "This is a message"),
            ("Multi-word message here", "Multi-word message here"),
        ] {
            assert!(
                matches!(Command::parse(input), Command::Msg(msg) if msg == expected_msg),
                "expected Msg(\"{expected_msg}\") for {input}"
            );
        }
    }

    #[test]
    fn parses_unknown_commands_as_messages() {
        // Commands that don't exist should be treated as regular messages
        for input in ["/unknown", "/sleep", "/quit_now"] {
            assert!(
                matches!(Command::parse(input), Command::Msg(msg) if msg == input),
                "expected Msg(\"{input}\") for {input}"
            );
        }
    }

    #[test]
    fn parses_messages_resembling_commands() {
        // Messages that have / but aren't valid commands
        for input in ["/", "//", "This /action is in the middle"] {
            assert!(
                matches!(Command::parse(input), Command::Msg(msg) if msg == input),
                "expected Msg(\"{input}\") for {input}"
            );
        }
    }
}
