namespace MsSqlDump.Models;

/// <summary>
/// Represents a column within an index.
/// </summary>
public class IndexColumnMetadata
{
    public required string ColumnName { get; init; }
    public required int OrdinalPosition { get; init; }
    public bool IsDescending { get; init; }
}

/// <summary>
/// Represents metadata for a database index.
/// </summary>
public class IndexMetadata
{
    public required string IndexName { get; init; }
    public required string TableSchema { get; set; }
    public required string TableName { get; init; }
    public required bool IsClustered { get; init; }
    public required bool IsUnique { get; init; }
    public required bool IsPrimaryKey { get; init; }
    public required List<IndexColumnMetadata> Columns { get; init; } = new();
    public List<string> IncludedColumns { get; init; } = new();
    public string? FilterExpression { get; init; }

    /// <summary>
    /// Gets the full qualified table name.
    /// </summary>
    public string FullTableName => $"[{TableSchema}].[{TableName}]";

    /// <summary>
    /// Generates CREATE INDEX script.
    /// </summary>
    public string GenerateCreateScript()
    {
        // Skip indexes that are part of primary keys (created with constraint)
        if (IsPrimaryKey)
        {
            return string.Empty;
        }

        var indexType = IsUnique ? "UNIQUE " : "";
        var clusterType = IsClustered ? "CLUSTERED" : "NONCLUSTERED";
        
        var columns = Columns
            .OrderBy(c => c.OrdinalPosition)
            .Select(c => $"[{c.ColumnName}] {(c.IsDescending ? "DESC" : "ASC")}")
            .ToList();

        var columnList = string.Join(", ", columns);

        var script = $"CREATE {indexType}{clusterType} INDEX [{IndexName}] ON {FullTableName} ({columnList})";

        // Add included columns if any
        if (IncludedColumns.Any())
        {
            var includedList = string.Join(", ", IncludedColumns.Select(c => $"[{c}]"));
            script += $" INCLUDE ({includedList})";
        }

        // Add filter expression if any (SQL 2008+)
        if (!string.IsNullOrWhiteSpace(FilterExpression))
        {
            script += $" WHERE {FilterExpression}";
        }

        return script + ";";
    }

    /// <summary>
    /// Generates DROP INDEX script.
    /// </summary>
    public string GenerateDropScript(bool supportsDropIfExists)
    {
        if (IsPrimaryKey)
        {
            return string.Empty;
        }

        if (supportsDropIfExists)
        {
            return $"DROP INDEX IF EXISTS [{IndexName}] ON {FullTableName};";
        }
        else
        {
            return $@"IF EXISTS (SELECT * FROM sys.indexes WHERE object_id = OBJECT_ID(N'{FullTableName}') AND name = N'{IndexName}')
    DROP INDEX [{IndexName}] ON {FullTableName};";
        }
    }
}
