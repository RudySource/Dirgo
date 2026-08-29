use super::{
    CommandHistoryRecordFrame, DecodedHistoryRecord, HISTORY_RECORD_PROTOCOL_VERSION,
    PROTOCOL_VERSION, SuggestionRequest, SuggestionResponse,
};
use std::io::BufRead;

pub const MAX_REQUEST_BYTES: usize = 65_536;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("suggestion request exceeds {MAX_REQUEST_BYTES} bytes")]
    RequestTooLarge,
    #[error("suggestion protocol frame is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("unsupported suggestion protocol version {found}; expected {PROTOCOL_VERSION}")]
    UnsupportedVersion { found: u16 },
    #[error("max_results must be between 1 and 20")]
    InvalidResultLimit,
    #[error("terminal_rows must be between 15 and 4096 when present")]
    InvalidTerminalRows,
    #[error("terminal_columns must be between 20 and 4096 when present")]
    InvalidTerminalColumns,
    #[error("suggestion protocol I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid command-history record: {0}")]
    InvalidHistoryRecord(String),
}

pub fn decode_history_record_frame(input: &[u8]) -> Result<DecodedHistoryRecord, ProtocolError> {
    if input.len() > MAX_REQUEST_BYTES {
        return Err(ProtocolError::RequestTooLarge);
    }
    if input.starts_with(b"DGOH2\0") {
        return decode_nul_history_record(input);
    }
    let input = input.strip_suffix(b"\n").unwrap_or(input);
    let input = input.strip_suffix(b"\r").unwrap_or(input);
    if input.is_empty() {
        return Err(ProtocolError::InvalidHistoryRecord(
            "command must not be empty".into(),
        ));
    }
    if input.first() != Some(&b'{') {
        let command = std::str::from_utf8(input).map_err(|_| {
            ProtocolError::InvalidHistoryRecord("command is not valid UTF-8".into())
        })?;
        validate_record_scalar(command, "command", MAX_REQUEST_BYTES - 1)?;
        return Ok(DecodedHistoryRecord::LegacyCommand(command.to_owned()));
    }

    let frame: CommandHistoryRecordFrame = serde_json::from_slice(input)?;
    if frame.protocol_version != HISTORY_RECORD_PROTOCOL_VERSION {
        return Err(ProtocolError::InvalidHistoryRecord(format!(
            "unsupported version {}; expected {HISTORY_RECORD_PROTOCOL_VERSION}",
            frame.protocol_version
        )));
    }
    validate_record_scalar(&frame.command, "command", MAX_REQUEST_BYTES - 1)?;
    let cwd = frame
        .cwd
        .to_str()
        .ok_or_else(|| ProtocolError::InvalidHistoryRecord("cwd is not valid UTF-8".into()))?;
    validate_record_scalar(cwd, "cwd", 16_384)?;
    if let Some(session_id) = frame.session_id.as_deref() {
        validate_record_scalar(session_id, "session", 256)?;
    }
    if frame.started_at == 0 {
        return Err(ProtocolError::InvalidHistoryRecord(
            "started_at must be a positive Unix timestamp".into(),
        ));
    }
    Ok(DecodedHistoryRecord::V2(frame))
}

fn decode_nul_history_record(input: &[u8]) -> Result<DecodedHistoryRecord, ProtocolError> {
    let mut fields = input.split(|byte| *byte == 0);
    let magic = fields.next();
    let values = fields.by_ref().take(7).collect::<Vec<_>>();
    if magic != Some(b"DGOH2".as_slice())
        || values.len() != 7
        || fields.any(|field| !field.is_empty())
    {
        return Err(ProtocolError::InvalidHistoryRecord(
            "malformed v2 NUL frame".into(),
        ));
    }
    let scalar = |index: usize, name: &str| {
        std::str::from_utf8(values[index])
            .map_err(|_| ProtocolError::InvalidHistoryRecord(format!("{name} is not valid UTF-8")))
    };
    let command = scalar(0, "command")?.to_owned();
    let cwd = scalar(1, "cwd")?.into();
    let exit_code = parse_optional(values[2], "exit code")?;
    let duration_ms = parse_optional(values[3], "duration")?;
    let session = scalar(4, "session")?;
    let session_id = (!session.is_empty()).then(|| session.to_owned());
    let shell = match scalar(5, "shell")? {
        "zsh" => super::ShellKind::Zsh,
        "bash" => super::ShellKind::Bash,
        "fish" => super::ShellKind::Fish,
        "powershell" => super::ShellKind::PowerShell,
        _ => return Err(ProtocolError::InvalidHistoryRecord("unknown shell".into())),
    };
    let started_at = scalar(6, "started_at")?.parse().map_err(|_| {
        ProtocolError::InvalidHistoryRecord("started_at is not an unsigned integer".into())
    })?;
    let frame = CommandHistoryRecordFrame {
        protocol_version: HISTORY_RECORD_PROTOCOL_VERSION,
        command,
        cwd,
        exit_code,
        duration_ms,
        session_id,
        shell,
        started_at,
    };
    let encoded = serde_json::to_vec(&frame).map_err(ProtocolError::InvalidJson)?;
    decode_history_record_frame(&encoded)
}

fn parse_optional<T: std::str::FromStr>(
    value: &[u8],
    name: &str,
) -> Result<Option<T>, ProtocolError> {
    if value.is_empty() {
        return Ok(None);
    }
    let value = std::str::from_utf8(value)
        .map_err(|_| ProtocolError::InvalidHistoryRecord(format!("{name} is not valid UTF-8")))?;
    value
        .parse()
        .map(Some)
        .map_err(|_| ProtocolError::InvalidHistoryRecord(format!("{name} is invalid")))
}

fn validate_record_scalar(
    value: &str,
    name: &str,
    maximum_bytes: usize,
) -> Result<(), ProtocolError> {
    if value.is_empty() {
        return Err(ProtocolError::InvalidHistoryRecord(format!(
            "{name} must not be empty"
        )));
    }
    if value.len() > maximum_bytes {
        return Err(ProtocolError::InvalidHistoryRecord(format!(
            "{name} exceeds {maximum_bytes} bytes"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(ProtocolError::InvalidHistoryRecord(format!(
            "{name} contains a control character"
        )));
    }
    Ok(())
}

pub fn read_bounded_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, ProtocolError> {
    let mut frame = Vec::new();
    let mut oversized = false;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if frame.is_empty() && !oversized {
                return Ok(None);
            }
            break;
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        if !oversized && frame.len() + consumed <= MAX_REQUEST_BYTES {
            frame.extend_from_slice(&available[..consumed]);
        } else {
            oversized = true;
        }
        reader.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    if oversized {
        Err(ProtocolError::RequestTooLarge)
    } else {
        Ok(Some(frame))
    }
}

pub fn decode_request_line(input: &[u8]) -> Result<SuggestionRequest, ProtocolError> {
    if input.len() > MAX_REQUEST_BYTES {
        return Err(ProtocolError::RequestTooLarge);
    }
    let input = input.strip_suffix(b"\n").unwrap_or(input);
    let input = input.strip_suffix(b"\r").unwrap_or(input);
    let request: SuggestionRequest = serde_json::from_slice(input)?;
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion {
            found: request.protocol_version,
        });
    }
    if !(1..=20).contains(&request.max_results) {
        return Err(ProtocolError::InvalidResultLimit);
    }
    if request
        .terminal_rows
        .is_some_and(|rows| !(15..=4_096).contains(&rows))
    {
        return Err(ProtocolError::InvalidTerminalRows);
    }
    if request
        .terminal_columns
        .is_some_and(|columns| !(20..=4_096).contains(&columns))
    {
        return Err(ProtocolError::InvalidTerminalColumns);
    }
    Ok(request)
}

pub fn encode_response_line(response: &SuggestionResponse) -> Result<String, ProtocolError> {
    let mut encoded = serde_json::to_string(response)?;
    encoded.push('\n');
    Ok(encoded)
}
