use std::path::PathBuf;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use dirgo::{
    model::DirectoryRecord,
    suggestions::{
        CommandCatalog, PROTOCOL_VERSION, ShellKind, SuggestionData, SuggestionEngine,
        SuggestionRequest,
    },
};

fn record(index: usize) -> DirectoryRecord {
    let basename = if index == 99_999 {
        "PunkProject".to_string()
    } else {
        format!("project-{index:06}")
    };
    let path = PathBuf::from("/benchmark").join(&basename);
    DirectoryRecord {
        display_path: path.display().to_string(),
        parent: PathBuf::from("/benchmark"),
        depth: 2,
        path,
        basename,
        is_project_root: true,
        project_kind: None,
        last_seen: 1,
    }
}

fn suggestions(c: &mut Criterion) {
    let records = (0..100_000).map(record).collect();
    let engine = SuggestionEngine::new_indexed(SuggestionData {
        records,
        catalog: CommandCatalog::default(),
        ..SuggestionData::default()
    });
    let request = SuggestionRequest {
        protocol_version: PROTOCOL_VERSION,
        request_id: 1,
        shell: ShellKind::Zsh,
        cwd: PathBuf::from("/benchmark"),
        before_cursor: "cd punk".into(),
        after_cursor: String::new(),
        max_results: 8,
        terminal_rows: Some(24),
        terminal_columns: Some(120),
        presentation: dirgo::suggestions::SuggestionPresentation::List,
    };
    c.bench_with_input(
        BenchmarkId::new("warm_prefix", "100k_directories"),
        &request,
        |bencher, request| bencher.iter(|| engine.suggest(request)),
    );

    let executables = (0..8_192).map(|index| {
        if index == 8_191 {
            "slash-tool".to_owned()
        } else {
            format!("command-{index:04}")
        }
    });
    let command_engine = SuggestionEngine::new_indexed(SuggestionData {
        catalog: CommandCatalog::from_executable_names(executables),
        ..SuggestionData::default()
    });
    let command_request = SuggestionRequest {
        before_cursor: "slas".into(),
        ..request.clone()
    };
    c.bench_with_input(
        BenchmarkId::new("warm_prefix", "8192_path_commands"),
        &command_request,
        |bencher, request| bencher.iter(|| command_engine.suggest(request)),
    );

    let catalog_request = SuggestionRequest {
        before_cursor: "command-".into(),
        ..request.clone()
    };
    c.bench_with_input(
        BenchmarkId::new("catalog_page", "8192_path_commands"),
        &catalog_request,
        |bencher, request| bencher.iter(|| command_engine.suggest_page(request, 192, 96)),
    );

    let nested_request = SuggestionRequest {
        before_cursor: "docker compose u".into(),
        ..request
    };
    c.bench_with_input(
        BenchmarkId::new("warm_prefix", "nested_command_spec"),
        &nested_request,
        |bencher, request| bencher.iter(|| command_engine.suggest(request)),
    );
}

criterion_group!(benches, suggestions);
criterion_main!(benches);
