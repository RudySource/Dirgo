using System.Diagnostics;
using System.Management.Automation;
using System.Management.Automation.Subsystem;
using System.Management.Automation.Subsystem.Prediction;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Dirgo.Predictor;

public sealed class DirgoPredictor : ICommandPredictor, IDisposable
{
    private const int ResponseBudgetMilliseconds = 18;
    private readonly object _gate = new();
    private readonly string _executable;
    private Process? _worker;
    private Task<string?>? _ready;
    private bool _isReady;
    private long _requestId;

    public DirgoPredictor(string executable)
    {
        _executable = executable;
        StartWorker();
    }

    public Guid Id => new("531a90d6-192f-46c7-94cb-2c81413eed9b");
    public string Name => "Dirgo";
    public string Description => "Private, local directory and command suggestions from Dirgo.";

    public SuggestionPackage GetSuggestion(
        PredictionClient client,
        PredictionContext context,
        CancellationToken cancellationToken)
    {
        var input = context.InputAst.Extent.Text;
        if (string.IsNullOrWhiteSpace(input) || cancellationToken.IsCancellationRequested ||
            !Monitor.TryEnter(_gate))
        {
            return default;
        }

        try
        {
            var worker = EnsureWorker();
            if (worker is null) return default;

            var requestId = (ulong)Interlocked.Increment(ref _requestId);
            var request = new SuggestionRequest
            {
                RequestId = requestId,
                Cwd = Environment.GetEnvironmentVariable("DGO_PREDICTOR_CWD")
                    ?? Environment.CurrentDirectory,
                BeforeCursor = input,
            };
            worker.StandardInput.WriteLine(JsonSerializer.Serialize(request));
            worker.StandardInput.Flush();

            var responseTask = worker.StandardOutput.ReadLineAsync();
            if (!responseTask.Wait(ResponseBudgetMilliseconds, cancellationToken))
            {
                StopWorker();
                return default;
            }
            var line = responseTask.Result;
            if (line is null) {
                StopWorker();
                return default;
            }
            var response = JsonSerializer.Deserialize<SuggestionResponse>(line);
            if (response is null || response.RequestId != requestId || response.Error is not null)
                return default;

            var suggestions = response.Suggestions
                .Where(item => item.Edit.ExpectedBefore == input && item.Edit.Replacement != input)
                .Select(item => new PredictiveSuggestion(
                    item.Edit.Replacement,
                    item.Description ?? item.Source.ToUpperInvariant()))
                .ToList();
            return suggestions.Count == 0 ? default : new SuggestionPackage(suggestions);
        }
        catch (OperationCanceledException)
        {
            return default;
        }
        catch (Exception)
        {
            StopWorker();
            return default;
        }
        finally
        {
            Monitor.Exit(_gate);
        }
    }

    public bool CanAcceptFeedback(PredictionClient client, PredictorFeedbackKind feedback) =>
        feedback == PredictorFeedbackKind.CommandLineAccepted;

    public void OnCommandLineAccepted(PredictionClient client, IReadOnlyList<string> history)
    {
        if (history.Count == 0) return;
        var command = history[^1];
        _ = Task.Run(() => RecordCommand(command));
    }

    public void OnSuggestionDisplayed(PredictionClient client, uint session, int countOrIndex) { }
    public void OnSuggestionAccepted(PredictionClient client, uint session, string acceptedSuggestion) { }
    public void OnCommandLineExecuted(PredictionClient client, string commandLine, bool success) { }

    public void Dispose()
    {
        lock (_gate) StopWorker();
    }

    private Process? EnsureWorker()
    {
        if (_worker is null || _worker.HasExited) StartWorker();
        if (_worker is not null && _isReady) return _worker;
        if (_worker is null || _ready is null || !_ready.IsCompleted) return null;
        try
        {
            if (_ready.Result != "READY 2") {
                StopWorker();
                return null;
            }
        }
        catch
        {
            StopWorker();
            return null;
        }
        _ready = null;
        _isReady = true;
        return _worker;
    }

    private void StartWorker()
    {
        StopWorker();
        try
        {
            var start = CreateStartInfo("__suggest-worker", "--ready");
            start.RedirectStandardOutput = true;
            start.RedirectStandardError = true;
            _worker = Process.Start(start);
            _isReady = false;
            _ready = _worker?.StandardOutput.ReadLineAsync();
            if (_worker is not null)
                _worker.ErrorDataReceived += (_, _) => { };
            _worker?.BeginErrorReadLine();
        }
        catch
        {
            _worker = null;
        }
    }

    private void StopWorker()
    {
        var worker = _worker;
        _worker = null;
        _ready = null;
        _isReady = false;
        if (worker is null) return;
        try
        {
            worker.StandardInput.Close();
            if (!worker.WaitForExit(50)) worker.Kill(entireProcessTree: true);
        }
        catch { }
        worker.Dispose();
    }

    private void RecordCommand(string command)
    {
        try
        {
            using var process = Process.Start(CreateStartInfo("__suggest-record"));
            if (process is null) return;
            process.StandardInput.Write(command);
            process.StandardInput.Close();
            if (!process.WaitForExit(250)) process.Kill(entireProcessTree: true);
        }
        catch { }
    }

    private ProcessStartInfo CreateStartInfo(params string[] arguments)
    {
        var start = new ProcessStartInfo(_executable)
        {
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardInput = true,
        };
        foreach (var argument in arguments) start.ArgumentList.Add(argument);
        return start;
    }

    private sealed class SuggestionRequest
    {
        [JsonPropertyName("protocol_version")] public ushort ProtocolVersion { get; init; } = 2;
        [JsonPropertyName("request_id")] public ulong RequestId { get; init; }
        [JsonPropertyName("shell")] public string Shell { get; init; } = "power_shell";
        [JsonPropertyName("cwd")] public string Cwd { get; init; } = "";
        [JsonPropertyName("before_cursor")] public string BeforeCursor { get; init; } = "";
        [JsonPropertyName("after_cursor")] public string AfterCursor { get; init; } = "";
        [JsonPropertyName("max_results")] public int MaxResults { get; init; } = 8;
        [JsonPropertyName("terminal_rows")] public ushort? TerminalRows { get; init; }
        [JsonPropertyName("terminal_columns")] public ushort? TerminalColumns { get; init; }
        [JsonPropertyName("presentation")] public string Presentation { get; init; } = "list";
    }

    private sealed class SuggestionResponse
    {
        [JsonPropertyName("request_id")] public ulong RequestId { get; init; }
        [JsonPropertyName("suggestions")] public List<Suggestion> Suggestions { get; init; } = [];
        [JsonPropertyName("error")] public string? Error { get; init; }
    }

    private sealed class Suggestion
    {
        [JsonPropertyName("edit")] public TextEdit Edit { get; init; } = new();
        [JsonPropertyName("description")] public string? Description { get; init; }
        [JsonPropertyName("source")] public string Source { get; init; } = "DIR";
    }

    private sealed class TextEdit
    {
        [JsonPropertyName("expected_before")] public string ExpectedBefore { get; init; } = "";
        [JsonPropertyName("replacement")] public string Replacement { get; init; } = "";
    }
}

public sealed class Init : IModuleAssemblyInitializer, IModuleAssemblyCleanup
{
    private static readonly Guid PredictorId = new("531a90d6-192f-46c7-94cb-2c81413eed9b");

    public void OnImport()
    {
        var executable = Environment.GetEnvironmentVariable("DGO_PREDICTOR_EXECUTABLE");
        if (string.IsNullOrWhiteSpace(executable)) return;
        SubsystemManager.RegisterSubsystem<ICommandPredictor, DirgoPredictor>(
            new DirgoPredictor(executable));
    }

    public void OnRemove(PSModuleInfo psModuleInfo)
    {
        SubsystemManager.UnregisterSubsystem<ICommandPredictor>(PredictorId);
    }
}
