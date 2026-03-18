namespace MsSqlDump.Models;

/// <summary>
/// Types of programmable database objects.
/// </summary>
public enum ProgrammableObjectType
{
    View,
    StoredProcedure,
    ScalarFunction,
    InlineTableValuedFunction,
    TableValuedFunction,
    Trigger
}

/// <summary>
/// Represents a programmable database object (view, procedure, function, trigger).
/// </summary>
public class ProgrammableObject
{
    public required string SchemaName { get; init; }
    public required string ObjectName { get; init; }
    public required ProgrammableObjectType ObjectType { get; init; }
    public required string Definition { get; init; }
    
    // For triggers - the table they're attached to
    public string? ParentTableSchema { get; init; }
    public string? ParentTableName { get; init; }

    /// <summary>
    /// Gets the full qualified object name.
    /// </summary>
    public string FullName => $"[{SchemaName}].[{ObjectName}]";

    /// <summary>
    /// Gets the parent table name for triggers.
    /// </summary>
    public string? ParentTableFullName => 
        ParentTableSchema != null && ParentTableName != null 
            ? $"[{ParentTableSchema}].[{ParentTableName}]" 
            : null;
}
