use dirgo::suggestions::{
    CommandHistoryAggregateV2, CommandHistoryEventV2, CommandHistoryRecordFrame, CommandOutcome,
    DecodedHistoryRecord, HISTORY_RECORD_PROTOCOL_VERSION, MAX_REQUEST_BYTES, PROTOCOL_VERSION,
    ShellKind, Suggestion, SuggestionPresentation, SuggestionRequest, SuggestionResponse,
    SuggestionSource, TextEdit, apply_text_edit, decode_history_record_frame, decode_request_line,
    encode_response_line, read_bounded_frame, sanitize_suggestion, visible_result_limit,
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
        terminal_rows: Some(24),
        terminal_columns: Some(120),
        presentation: SuggestionPresentation::List,
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
        protocol_version: 1,
        request_id: 9,
        shell: ShellKind::Fish,
        cwd: "/tmp".into(),
        before_cursor: "dgo".into(),
        after_cursor: String::new(),
        max_results: 4,
        terminal_rows: None,
        terminal_columns: None,
        presentation: SuggestionPresentation::Inline,
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
fn adaptive_visible_limits_are_five_to_twelve_and_respect_one_third_height() {
    assert_eq!(visible_result_limit(Some(15), 20), 5);
    assert_eq!(visible_result_limit(Some(24), 20), 8);
    assert_eq!(visible_result_limit(Some(36), 20), 12);
    assert_eq!(visible_result_limit(Some(60), 8), 8);
    assert_eq!(visible_result_limit(None, 20), 12);
}

#[test]
fn decoder_rejects_unusable_or_unbounded_terminal_dimensions() {
    let request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 10,
        shell: ShellKind::Zsh,
        cwd: "/tmp".into(),
        before_cursor: "d".into(),
        after_cursor: String::new(),
        max_results: 8,
        terminal_rows: Some(14),
        terminal_columns: Some(120),
        presentation: SuggestionPresentation::List,
    };
    let encoded = serde_json::to_vec(&request).expect("serialize request");
    assert!(decode_request_line(&encoded).is_err());

    let mut too_wide = request;
    too_wide.terminal_rows = Some(24);
    too_wide.terminal_columns = Some(4_097);
    let encoded = serde_json::to_vec(&too_wide).expect("serialize request");
    assert!(decode_request_line(&encoded).is_err());
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
        terminal_rows: None,
        terminal_columns: None,
        presentation: SuggestionPresentation::Explicit,
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

#[test]
fn history_record_v2_round_trips_context_and_accepts_legacy_command_frames() {
    let frame = CommandHistoryRecordFrame {
        protocol_version: HISTORY_RECORD_PROTOCOL_VERSION,
        command: "cargo test --workspace".into(),
        cwd: "/tmp/project/src".into(),
        exit_code: Some(0),
        duration_ms: Some(1_234),
        session_id: Some("zsh-session-42".into()),
        shell: ShellKind::Zsh,
        started_at: 1_787_900_000,
    };
    let encoded = serde_json::to_vec(&frame).expect("record json");

    assert_eq!(
        decode_history_record_frame(&encoded).expect("v2 record"),
        DecodedHistoryRecord::V2(frame)
    );
    assert_eq!(
        decode_history_record_frame(b"cargo test --workspace\n").expect("legacy record"),
        DecodedHistoryRecord::LegacyCommand("cargo test --workspace".into())
    );
}

#[test]
fn history_record_v2_accepts_shell_safe_nul_framing() {
    let input =
        b"DGOH2\x00cargo test\x00/tmp/project\x001\x00345\x00zsh-42\x00zsh\x001800000000\x00";
    let DecodedHistoryRecord::V2(frame) = decode_history_record_frame(input).expect("nul frame")
    else {
        panic!("expected v2 frame");
    };
    assert_eq!(frame.command, "cargo test");
    assert_eq!(frame.exit_code, Some(1));
    assert_eq!(frame.duration_ms, Some(345));
    assert_eq!(frame.session_id.as_deref(), Some("zsh-42"));
    assert_eq!(frame.shell, ShellKind::Zsh);
}

#[test]
fn history_record_decoder_rejects_invalid_metadata_and_bounds() {
    let valid = CommandHistoryRecordFrame {
        protocol_version: HISTORY_RECORD_PROTOCOL_VERSION,
        command: "cargo test".into(),
        cwd: "/tmp/project".into(),
        exit_code: Some(1),
        duration_ms: None,
        session_id: Some("fish-session-7".into()),
        shell: ShellKind::Fish,
        started_at: 1_787_900_000,
    };

    for invalid in [
        CommandHistoryRecordFrame {
            command: "cargo test\nrm -rf ignored".into(),
            ..valid.clone()
        },
        CommandHistoryRecordFrame {
            cwd: "/tmp/project\nother".into(),
            ..valid.clone()
        },
        CommandHistoryRecordFrame {
            session_id: Some("session\u{1b}".into()),
            ..valid.clone()
        },
        CommandHistoryRecordFrame {
            started_at: 0,
            ..valid.clone()
        },
        CommandHistoryRecordFrame {
            protocol_version: 99,
            ..valid.clone()
        },
    ] {
        let encoded = serde_json::to_vec(&invalid).expect("invalid record json");
        assert!(
            decode_history_record_frame(&encoded).is_err(),
            "invalid frame unexpectedly decoded: {invalid:?}"
        );
    }

    assert!(decode_history_record_frame(b"\n").is_err());
    assert!(decode_history_record_frame(&vec![b'x'; MAX_REQUEST_BYTES + 1]).is_err());
}

#[test]
fn context_engine_domain_types_preserve_unknowns_and_outcome_semantics() {
    assert_eq!(
        CommandOutcome::from_exit_code(Some(0)),
        CommandOutcome::Success
    );
    assert_eq!(
        CommandOutcome::from_exit_code(Some(17)),
        CommandOutcome::Failure
    );
    assert_eq!(
        CommandOutcome::from_exit_code(None),
        CommandOutcome::Unknown
    );

    let event = CommandHistoryEventV2 {
        id: 7,
        command: "cargo test".into(),
        started_at: 1_787_900_000,
        duration_ms: Some(2_500),
        cwd: "/tmp/project".into(),
        project_root: Some("/tmp/project".into()),
        exit_code: Some(0),
        outcome: CommandOutcome::Success,
        session_id: Some("zsh-session-42".into()),
    };
    let event_json = serde_json::to_string(&event).expect("event json");
    assert_eq!(
        serde_json::from_str::<CommandHistoryEventV2>(&event_json).expect("event decode"),
        event
    );

    let legacy = CommandHistoryAggregateV2 {
        scope_key: "global".into(),
        command: "git status".into(),
        use_count: 9,
        success_count: 0,
        failure_count: 0,
        unknown_count: 9,
        last_used: 1_787_800_000,
        last_success: None,
        last_failure: None,
        total_duration_ms: 0,
        measured_duration_count: 0,
    };
    let aggregate_json = serde_json::to_string(&legacy).expect("aggregate json");
    assert_eq!(
        serde_json::from_str::<CommandHistoryAggregateV2>(&aggregate_json)
            .expect("aggregate decode"),
        legacy
    );
}
