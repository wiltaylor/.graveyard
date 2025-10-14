namespace MsSqlDump.Models;

/// <summary>
/// Configuration for database export operations.
/// </summary>
public class ExportConfiguration
{
    public required string Server { get; init; }
    public required string Database { get; init; }
    public string? UserId { get; init; }
    public string? Password { get; init; }
    public bool UseWindowsAuth { get; init; } = true;
    public required string OutputPath { get; init; }
    
    // Export options
    public bool SchemaOnly { get; init; } = false;
    public bool IncludeTables { get; init; } = true;
    public bool IncludeViews { get; init; } = true;
    public bool IncludeProcedures { get; init; } = true;
    public bool IncludeFunctions { get; init; } = true;
    public bool IncludeTriggers { get; init; } = true;
    
    // Data export options
    public int BatchSize { get; init; } = 1000;
    
    // Connection options
    public int ConnectionTimeout { get; init; } = 30;
    public int MaxRetries { get; init; } = 3;
    public int RetryDelaySeconds { get; init; } = 5;
    
    // Filtering options
    public List<string> IncludePatterns { get; init; } = new();
    public List<string> ExcludePatterns { get; init; } = new();
}
