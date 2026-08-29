pub fn is_sensitive_command(command: &str, deny_patterns: &[String]) -> bool {
    if command.is_empty()
        || command.len() > 65_536
        || command.chars().any(|character| character.is_control())
    {
        return true;
    }
    let normalized = command.to_ascii_lowercase();
    let built_in = [
        "password=",
        "password:",
        "passwd=",
        "passwd:",
        "token=",
        "token:",
        "api_token=",
        "api-key=",
        "api_key=",
        "secret=",
        "client_secret",
        "access_token",
        "private_key",
        "private-key",
        "aws_secret_access_key",
        "aws_access_key_id=",
        "azure_client_secret",
        "google_application_credentials=",
        "github_token",
        "npm_token",
        "_authtoken",
        "authorization:",
        "cookie:",
        "set-cookie:",
        "bearer ",
        "-----begin private key-----",
        "sshpass ",
        "curl -u ",
        "curl --user ",
    ];
    let sensitive_flags = [
        "--password",
        "--passwd",
        "--secret",
        "--token",
        "--api-key",
        "--api_key",
        "--private-key",
        "--private_key",
        "--authorization",
        "--cookie",
    ];
    built_in.iter().any(|pattern| normalized.contains(pattern))
        || sensitive_flags
            .iter()
            .any(|flag| contains_flag(&normalized, flag))
        || contains_uri_user_info(&normalized)
        || deny_patterns
            .iter()
            .any(|pattern| normalized.contains(&pattern.to_ascii_lowercase()))
}

fn contains_flag(command: &str, flag: &str) -> bool {
    command.match_indices(flag).any(|(start, _)| {
        let before = command[..start].chars().next_back();
        let after = command[start + flag.len()..].chars().next();
        before.is_none_or(char::is_whitespace)
            && after.is_none_or(|value| value.is_whitespace() || value == '=')
    })
}

fn contains_uri_user_info(command: &str) -> bool {
    let mut remainder = command;
    while let Some(scheme) = remainder.find("://") {
        let authority = &remainder[scheme + 3..];
        let end = authority
            .find(['/', ' ', '\t', '\r', '\n'])
            .unwrap_or(authority.len());
        if authority[..end].contains('@') && authority[..end].contains(':') {
            return true;
        }
        remainder = &authority[end..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catches_common_secret_forms_without_blocking_normal_commands() {
        for command in [
            "login --password hunter2",
            "curl -H 'Authorization: Bearer abc' example.test",
            "export AWS_SECRET_ACCESS_KEY=abc",
            "npm config set //registry/:_authToken abc",
            "curl -u admin:secret example.test",
            "tool --ToKeN 'abc'",
            "tool --api-key=abc",
            "export GOOGLE_APPLICATION_CREDENTIALS=/tmp/key.json",
            "curl -H 'Cookie: sid=abc' example.test",
            "git clone https://user:password@example.test/repo",
        ] {
            assert!(is_sensitive_command(command, &[]), "{command}");
        }
        assert!(!is_sensitive_command("cargo test --package api", &[]));
        assert!(!is_sensitive_command("tokenizer --input words", &[]));
        assert!(!is_sensitive_command("tool --tokenize source", &[]));
        assert!(is_sensitive_command(
            "deploy production",
            &["deploy".to_string()]
        ));
    }
}
