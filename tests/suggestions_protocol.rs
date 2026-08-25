use dirgo::suggestions::{
    MAX_REQUEST_BYTES, PROTOCOL_VERSION, ShellKind, Suggestion, SuggestionRequest,
    SuggestionResponse, SuggestionSource, TextEdit, apply_text_edit, decode_request_line,
    encode_response_line, read_bounded_frame, sanitize_suggestion,
};

#[test]
fn request_round_trips_unicode_without_a_numeric_cursor_offset() {
    let request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 42,
        shell: ShellKind::Zsh,
        cwd: "/tmp/проект 🚀".into(),
        before_cursor: "cd про".into(),
        after_cursor: " --print".into(),
        max_results: 8,
    };

    let encoded = serde_json::to_string(&request).expect("serialize request");
    let decoded: SuggestionRequest = serde_json::from_str(&encoded).expect("deserialize request");

    assert_eq!(decoded, request);
    assert!(!encoded.contains("cursor_offset"));
}

#[test]
fn text_edit_applies_only_to_the_buffer_suffix_it_was_ranked_for() {
    let edit = TextEdit {
        expected_before: "cd pro".into(),
        replacement: "cd Projects/Dirgo".into(),
    };

    assert_eq!(
        apply_text_edit("cd pro", " --print", &edit),
        Some("cd Projects/Dirgo --print".into())
    );
    assert_eq!(apply_text_edit("cd prod", "", &edit), None);
}

#[test]
fn decoder_rejects_oversized_and_wrong_version_requests() {
    let oversized = vec![b'x'; MAX_REQUEST_BYTES + 1];
    assert_eq!(
        decode_request_line(&oversized)
            .expect_err("oversized request must fail")
            .to_string(),
        "suggestion request exceeds 65536 bytes"
    );

    let mut request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION + 1,
        request_id: 9,
        shell: ShellKind::Fish,
        cwd: "/tmp".into(),
        before_cursor: "dgo".into(),
        after_cursor: String::new(),
        max_results: 4,
    };
    let encoded = serde_json::to_vec(&request).expect("serialize request");
    assert!(
        decode_request_line(&encoded)
            .expect_err("wrong version must fail")
            .to_string()
            .contains("unsupported suggestion protocol version")
    );

    request.protocol_version = PROTOCOL_VERSION;
    request.max_results = 21;
    let encoded = serde_json::to_vec(&request).expect("serialize request");
    assert_eq!(
        decode_request_line(&encoded)
            .expect_err("large result limit must fail")
            .to_string(),
        "max_results must be between 1 and 20"
    );
}

#[test]
fn response_is_one_json_line_and_unsafe_edits_are_dropped() {
    let safe = Suggestion {
        id: "directory:projects".into(),
        edit: TextEdit {
            expected_before: "cd pro".into(),
            replacement: "cd Projects".into(),
        },
        display: "Projects".into(),
        description: Some("DIR".into()),
        source: SuggestionSource::Directory,
        score: 10.0,
    };
    assert_eq!(sanitize_suggestion(safe.clone()), Some(safe.clone()));

    let mut unsafe_edit = safe.clone();
    unsafe_edit.edit.replacement.push('\n');
    assert_eq!(sanitize_suggestion(unsafe_edit), None);

    let response = SuggestionResponse::success(42, vec![safe]);
    let encoded = encode_response_line(&response).expect("encode response");
    assert!(encoded.ends_with('\n'));
    assert_eq!(encoded.matches('\n').count(), 1);
    let decoded: SuggestionResponse =
        serde_json::from_str(encoded.trim_end()).expect("decode response");
    assert_eq!(decoded, response);
}

#[test]
fn bounded_frame_reader_drains_an_oversized_frame_before_the_next_request() {
    let valid = serde_json::to_vec(&SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 55,
        shell: ShellKind::Bash,
        cwd: "/tmp".into(),
        before_cursor: "git".into(),
        after_cursor: String::new(),
        max_results: 4,
    })
    .expect("request json");
    let mut input = vec![b'x'; MAX_REQUEST_BYTES + 10];
    input.push(b'\n');
    input.extend(valid);
    input.push(b'\n');
    let mut reader = std::io::BufReader::new(std::io::Cursor::new(input));

    assert!(
        read_bounded_frame(&mut reader)
            .expect_err("oversized frame")
            .to_string()
            .contains("exceeds")
    );
    let next = read_bounded_frame(&mut reader)
        .expect("next frame")
        .expect("frame present");
    assert_eq!(
        decode_request_line(&next)
            .expect("valid request")
            .request_id,
        55
    );
    assert!(read_bounded_frame(&mut reader).expect("eof").is_none());
}
