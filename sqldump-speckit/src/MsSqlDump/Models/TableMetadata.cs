namespace MsSqlDump.Models;

/// <summary>
/// Represents metadata for a single table including columns, constraints, and indexes.
/// </summary>
public class TableMetadata
{
    public required string SchemaName { get; init; }
    public required string TableName { get; init; }
    public required List<ColumnMetadata> Columns { get; set; } = new();
    public ConstraintMetadata? PrimaryKey { get; set; }
    public List<ConstraintMetadata> ForeignKeys { get; set; } = new();
    public List<ConstraintMetadata> UniqueConstraints { get; set; } = new();
    public List<ConstraintMetadata> CheckConstraints { get; set; } = new();
    public List<ConstraintMetadata> DefaultConstraints { get; set; } = new();
    public List<IndexMetadata> Indexes { get; set; } = new();
    public bool HasIdentityColumn { get; init; }
    public long RowCount { get; init; }

    /// <summary>
    /// Gets the full qualified table name.
    /// </summary>
    public string FullName => $"[{SchemaName}].[{TableName}]";

    /// <summary>
    /// Gets the list of tables this table depends on (via foreign keys).
    /// </summary>
    public List<string> GetDependencies()
    {
        return ForeignKeys
            .Where(fk => fk.ReferencedSchema != null && fk.ReferencedTable != null)
            .Select(fk => $"[{fk.ReferencedSchema}].[{fk.ReferencedTable}]")
            .Distinct()
            .Where(dep => dep != FullName) // Exclude self-references
            .ToList();
    }
}
