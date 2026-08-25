use super::ShellKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionContext {
    command: Option<String>,
    current_token: String,
    replacement_start: usize,
}

impl CompletionContext {
    pub fn parse(_shell: ShellKind, before_cursor: &str) -> Self {
        let mut tokens = Vec::<(String, usize)>::new();
        let mut token = String::new();
        let mut token_start = None;
        let mut quote = None;
        let mut escaped = false;

        for (index, character) in before_cursor.char_indices() {
            if escaped {
                token_start.get_or_insert(index);
                token.push(character);
                escaped = false;
                continue;
            }
            if character == '\\' && quote != Some('\'') {
                token_start.get_or_insert(index + character.len_utf8());
                escaped = true;
                continue;
            }
            if let Some(active_quote) = quote {
                if character == active_quote {
                    quote = None;
                } else {
                    token.push(character);
                }
                continue;
            }
            if matches!(character, '\'' | '"') {
                token_start.get_or_insert(index + character.len_utf8());
                quote = Some(character);
            } else if character.is_whitespace() {
                if let Some(start) = token_start.take() {
                    tokens.push((std::mem::take(&mut token), start));
                }
            } else {
                token_start.get_or_insert(index);
                token.push(character);
            }
        }
        if escaped {
            token.push('\\');
        }
        let ended_with_whitespace = before_cursor
            .chars()
            .last()
            .is_some_and(char::is_whitespace)
            && quote.is_none();
        if let Some(start) = token_start {
            tokens.push((token, start));
        }

        let command = tokens.first().map(|(value, _)| value.clone());
        let (current_token, replacement_start) = if ended_with_whitespace {
            (String::new(), before_cursor.len())
        } else {
            tokens
                .last()
                .cloned()
                .unwrap_or_else(|| (String::new(), before_cursor.len()))
        };
        Self {
            command,
            current_token,
            replacement_start,
        }
    }

    pub fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }

    pub fn current_token(&self) -> &str {
        &self.current_token
    }

    pub fn replacement_start(&self) -> usize {
        self.replacement_start
    }

    pub fn is_option(&self) -> bool {
        self.current_token.starts_with('-')
    }
}
