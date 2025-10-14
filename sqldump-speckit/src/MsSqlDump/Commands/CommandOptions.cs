using System.Collections.Generic;

namespace MsSqlDump.Commands;

/// <summary>
/// Command-line options for the dump command
/// </summary>
public class CommandOptions
{
    // Required options
    public required string Server { get; init; }
    public required string Database { get; init; }

    // Authentication (one of these is required)
    public bool WindowsAuth { get; init; }
    public string? User { get; init; }
    public string? Password { get; init; }

    // Output options
    public string Output { get; init; } = "./output";
    public bool SchemaOnly { get; init; } = false;

    // Object type filters (all default to true)
    public bool Tables { get; init; } = true;
    public bool Views { get; init; } = true;
    public bool Procedures { get; init; } = true;
    public bool Functions { get; init; } = true;
    public bool Triggers { get; init; } = true;

    // Pattern filters
    public List<string> Include { get; init; } = new();
    public List<string> Exclude { get; init; } = new();

    // Advanced options
    public int BatchSize { get; init; } = 1000;
    public int Timeout { get; init; } = 30;
    public int Retries { get; init; } = 3;
    public bool Verbose { get; init; } = false;

    /// <summary>
    /// Validates that authentication options are properly configured
    /// </summary>
    public bool IsAuthenticationValid()
    {
        if (WindowsAuth)
            return true;

        return !string.IsNullOrEmpty(User) && !string.IsNullOrEmpty(Password);
    }

    /// <summary>
    /// Validates that at least one object type is enabled
    /// </summary>
    public bool IsObjectTypeFilterValid()
    {
        return Tables || Views || Procedures || Functions || Triggers;
    }
}
