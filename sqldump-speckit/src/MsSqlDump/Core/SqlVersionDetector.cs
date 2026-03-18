using Microsoft.Data.SqlClient;

namespace MsSqlDump.Core;

/// <summary>
/// Detects SQL Server version from the database connection.
/// </summary>
public class SqlVersionDetector
{
    /// <summary>
    /// Detects the SQL Server version and returns the major version number.
    /// </summary>
    /// <param name="connection">Open SQL connection</param>
    /// <returns>Major version number (e.g., 10 for SQL 2008, 11 for SQL 2012, 15 for SQL 2019)</returns>
    public async Task<int> DetectMajorVersionAsync(SqlConnection connection)
    {
        if (connection.State != System.Data.ConnectionState.Open)
            throw new InvalidOperationException("Connection must be open");

        const string query = "SELECT SERVERPROPERTY('ProductVersion') AS Version";
        
        using var command = new SqlCommand(query, connection);
        var result = await command.ExecuteScalarAsync();
        
        if (result == null || result == DBNull.Value)
            throw new InvalidOperationException("Failed to detect SQL Server version");

        var versionString = result.ToString()!;
        var parts = versionString.Split('.');
        
        if (parts.Length == 0 || !int.TryParse(parts[0], out int majorVersion))
            throw new InvalidOperationException($"Invalid version format: {versionString}");

        return majorVersion;
    }

    /// <summary>
    /// Gets the full SQL Server version string.
    /// </summary>
    public async Task<string> GetFullVersionAsync(SqlConnection connection)
    {
        if (connection.State != System.Data.ConnectionState.Open)
            throw new InvalidOperationException("Connection must be open");

        const string query = "SELECT @@VERSION AS VersionString";
        
        using var command = new SqlCommand(query, connection);
        var result = await command.ExecuteScalarAsync();
        
        return result?.ToString() ?? "Unknown";
    }

    /// <summary>
    /// Gets a friendly version name (e.g., "SQL Server 2019").
    /// </summary>
    public string GetVersionName(int majorVersion)
    {
        return majorVersion switch
        {
            16 => "SQL Server 2022",
            15 => "SQL Server 2019",
            14 => "SQL Server 2017",
            13 => "SQL Server 2016",
            12 => "SQL Server 2014",
            11 => "SQL Server 2012",
            10 => "SQL Server 2008/2008 R2",
            9 => "SQL Server 2005",
            _ => $"SQL Server (Version {majorVersion})"
        };
    }

    /// <summary>
    /// Checks if the version supports DROP IF EXISTS syntax.
    /// </summary>
    public bool SupportsDropIfExists(int majorVersion)
    {
        // DROP IF EXISTS was introduced in SQL Server 2016 (version 13)
        return majorVersion >= 13;
    }
}
