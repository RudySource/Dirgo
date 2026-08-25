use super::{PROTOCOL_VERSION, SuggestionRequest, SuggestionResponse};
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
    #[error("suggestion protocol I/O failed: {0}")]
    Io(#[from] std::io::Error),
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
    Ok(request)
}

pub fn encode_response_line(response: &SuggestionResponse) -> Result<String, ProtocolError> {
    let mut encoded = serde_json::to_string(response)?;
    encoded.push('\n');
    Ok(encoded)
}
