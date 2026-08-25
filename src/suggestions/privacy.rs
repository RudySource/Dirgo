pub fn is_sensitive_command(command: &str, deny_patterns: &[String]) -> bool {
    if command.is_empty()
        || command.len() > 65_536
        || command.chars().any(|character| character.is_control())
    {
        return true;
    }
    let normalized = command.to_ascii_lowercase();
    let built_in = [
        "--password",
        "--passwd",
        "--token",
        "--api-key",
        "--api_key",
        "--secret",
        "password=",
        "password:",
        "passwd=",
        "token=",
        "token:",
        "api_token=",
        "api-key=",
        "api_key=",
        "secret=",
        "client_secret",
        "access_token",
        "aws_secret_access_key",
        "github_token",
        "npm_token",
        "_authtoken",
        "authorization:",
        "bearer ",
        "-----begin private key-----",
        "sshpass ",
        "curl -u ",
        "curl --user ",
    ];
    built_in.iter().any(|pattern| normalized.contains(pattern))
        || deny_patterns
            .iter()
            .any(|pattern| normalized.contains(&pattern.to_ascii_lowercase()))
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
        ] {
            assert!(is_sensitive_command(command, &[]), "{command}");
        }
        assert!(!is_sensitive_command("cargo test --package api", &[]));
        assert!(is_sensitive_command(
            "deploy production",
            &["deploy".to_string()]
        ));
    }
}
