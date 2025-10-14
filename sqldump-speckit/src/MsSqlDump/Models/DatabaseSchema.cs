namespace MsSqlDump.Models;

/// <summary>
/// Represents the complete database schema with all metadata.
/// </summary>
public class DatabaseSchema
{
    public required string DatabaseName { get; init; }
    public required string ServerVersion { get; init; }
    public required int ServerMajorVersion { get; init; }
    public string? Collation { get; init; }
    
    public List<TableMetadata> Tables { get; init; } = new();
    public List<ProgrammableObject> Views { get; init; } = new();
    public List<ProgrammableObject> Procedures { get; init; } = new();
    public List<ProgrammableObject> Functions { get; init; } = new();
    public List<ProgrammableObject> Triggers { get; init; } = new();
    
    public Core.DependencyGraph? DependencyGraph { get; set; }

    /// <summary>
    /// Gets the creation order for all tables based on dependencies.
    /// </summary>
    public List<string> GetTableCreationOrder()
    {
        if (DependencyGraph == null)
            throw new InvalidOperationException("Dependency graph not built. Call BuildDependencyGraph first.");

        return DependencyGraph.GetTopologicalOrder();
    }

    /// <summary>
    /// Gets total object count.
    /// </summary>
    public int TotalObjectCount => 
        Tables.Count + Views.Count + Procedures.Count + Functions.Count + Triggers.Count;
}
