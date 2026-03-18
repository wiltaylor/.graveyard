namespace MsSqlDump.Models;

/// <summary>
/// Types of database constraints.
/// </summary>
public enum ConstraintType
{
    PrimaryKey,
    ForeignKey,
    Unique,
    Check,
    Default
}

/// <summary>
/// Represents metadata for a database constraint.
/// </summary>
public class ConstraintMetadata
{
    public required string ConstraintName { get; init; }
    public required ConstraintType ConstraintType { get; init; }
    public required string TableSchema { get; init; }
    public required string TableName { get; init; }
    public required List<string> Columns { get; init; } = new();
    
    // Foreign key properties
    public string? ReferencedSchema { get; init; }
    public string? ReferencedTable { get; init; }
    public List<string>? ReferencedColumns { get; init; }
    public string? OnDeleteAction { get; init; }
    public string? OnUpdateAction { get; init; }
    
    // Check constraint properties
    public string? CheckExpression { get; init; }
    
    // Default constraint properties
    public string? DefaultExpression { get; init; }
    public string? DefaultColumn { get; init; }

    /// <summary>
    /// Gets the full qualified table name.
    /// </summary>
    public string FullTableName => $"[{TableSchema}].[{TableName}]";

    /// <summary>
    /// Generates the ALTER TABLE ADD CONSTRAINT script.
    /// </summary>
    public string GenerateCreateScript()
    {
        return ConstraintType switch
        {
            ConstraintType.PrimaryKey => GeneratePrimaryKeyScript(),
            ConstraintType.ForeignKey => GenerateForeignKeyScript(),
            ConstraintType.Unique => GenerateUniqueScript(),
            ConstraintType.Check => GenerateCheckScript(),
            ConstraintType.Default => GenerateDefaultScript(),
            _ => throw new NotSupportedException($"Constraint type {ConstraintType} not supported")
        };
    }

    private string GeneratePrimaryKeyScript()
    {
        var columnList = string.Join(", ", Columns.Select(c => $"[{c}]"));
        return $"ALTER TABLE {FullTableName} ADD CONSTRAINT [{ConstraintName}] PRIMARY KEY CLUSTERED ({columnList});";
    }

    private string GenerateForeignKeyScript()
    {
        if (string.IsNullOrWhiteSpace(ReferencedSchema) || 
            string.IsNullOrWhiteSpace(ReferencedTable) ||
            ReferencedColumns == null || ReferencedColumns.Count == 0)
        {
            throw new InvalidOperationException("Foreign key constraint missing referenced table or columns");
        }

        var columnList = string.Join(", ", Columns.Select(c => $"[{c}]"));
        var refColumnList = string.Join(", ", ReferencedColumns.Select(c => $"[{c}]"));
        var refTable = $"[{ReferencedSchema}].[{ReferencedTable}]";

        var script = $"ALTER TABLE {FullTableName} ADD CONSTRAINT [{ConstraintName}] " +
                     $"FOREIGN KEY ({columnList}) REFERENCES {refTable} ({refColumnList})";

        if (!string.IsNullOrWhiteSpace(OnDeleteAction))
        {
            script += $" ON DELETE {OnDeleteAction}";
        }

        if (!string.IsNullOrWhiteSpace(OnUpdateAction))
        {
            script += $" ON UPDATE {OnUpdateAction}";
        }

        return script + ";";
    }

    private string GenerateUniqueScript()
    {
        var columnList = string.Join(", ", Columns.Select(c => $"[{c}]"));
        return $"ALTER TABLE {FullTableName} ADD CONSTRAINT [{ConstraintName}] UNIQUE ({columnList});";
    }

    private string GenerateCheckScript()
    {
        if (string.IsNullOrWhiteSpace(CheckExpression))
        {
            throw new InvalidOperationException("Check constraint missing expression");
        }

        return $"ALTER TABLE {FullTableName} ADD CONSTRAINT [{ConstraintName}] CHECK {CheckExpression};";
    }

    private string GenerateDefaultScript()
    {
        if (string.IsNullOrWhiteSpace(DefaultExpression) || string.IsNullOrWhiteSpace(DefaultColumn))
        {
            throw new InvalidOperationException("Default constraint missing expression or column");
        }

        return $"ALTER TABLE {FullTableName} ADD CONSTRAINT [{ConstraintName}] DEFAULT {DefaultExpression} FOR [{DefaultColumn}];";
    }

    /// <summary>
    /// Generates DROP CONSTRAINT script.
    /// </summary>
    public string GenerateDropScript(bool supportsDropIfExists)
    {
        if (supportsDropIfExists)
        {
            return $"ALTER TABLE {FullTableName} DROP CONSTRAINT IF EXISTS [{ConstraintName}];";
        }
        else
        {
            return $@"IF EXISTS (SELECT * FROM sys.objects WHERE object_id = OBJECT_ID(N'{FullTableName}') AND type = 'U')
BEGIN
    IF EXISTS (SELECT * FROM sys.objects WHERE object_id = OBJECT_ID(N'[{ConstraintName}]') AND parent_object_id = OBJECT_ID(N'{FullTableName}'))
        ALTER TABLE {FullTableName} DROP CONSTRAINT [{ConstraintName}];
END;";
        }
    }
}
