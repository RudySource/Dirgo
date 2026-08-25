const PREDICTOR: &str = include_str!("../powershell/DirgoPredictor/DirgoPredictor.cs");

#[test]
fn predictor_sends_adaptive_protocol_v2_list_requests() {
    assert!(PREDICTOR.contains("ProtocolVersion { get; init; } = 2"));
    assert!(PREDICTOR.contains("MaxResults = 12"));
    assert!(PREDICTOR.contains("TerminalRows = GetTerminalRows()"));
    assert!(PREDICTOR.contains("TerminalColumns = GetTerminalColumns()"));
    assert!(PREDICTOR.contains("Presentation { get; init; } = \"list\""));
}

#[test]
fn predictor_maps_semantic_sources_and_observes_cancellation_around_io() {
    for mapping in [
        "\"command\" => \"CMD\"",
        "\"subcommand\" => \"SUB\"",
        "\"option\" => \"OPT\"",
        "\"directory\" => \"DIR\"",
        "\"filesystem\" => \"FILE\"",
        "\"command_history\" => \"HIST\"",
        "\"navigation_history\" => \"NAV\"",
    ] {
        assert!(PREDICTOR.contains(mapping), "missing mapping {mapping}");
    }
    assert!(
        PREDICTOR
            .matches("cancellationToken.IsCancellationRequested")
            .count()
            >= 2
    );
    assert!(!PREDICTOR.contains("AcceptLine"));
}
