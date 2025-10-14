namespace MsSqlDump.Writers;

/// <summary>
/// Manages the output directory structure for database export.
/// </summary>
public class DirectoryOrganizer
{
    private readonly string _outputPath;

    public DirectoryOrganizer(string outputPath)
    {
        if (string.IsNullOrWhiteSpace(outputPath))
            throw new ArgumentException("Output path cannot be empty", nameof(outputPath));

        _outputPath = outputPath;
    }

    /// <summary>
    /// Creates the complete output directory structure.
    /// </summary>
    public void CreateOutputStructure(bool includeData = true)
    {
        // Create main output directory
        Directory.CreateDirectory(_outputPath);

        // Create subdirectories for each object type
        Directory.CreateDirectory(TablesDirectory);
        Directory.CreateDirectory(ViewsDirectory);
        Directory.CreateDirectory(ProceduresDirectory);
        Directory.CreateDirectory(FunctionsDirectory);
        Directory.CreateDirectory(TriggersDirectory);
        
        if (includeData)
        {
            Directory.CreateDirectory(DataDirectory);
        }
    }

    /// <summary>
    /// Gets the tables directory path
    /// </summary>
    public string TablesDirectory => Path.Combine(_outputPath, "tables");

    /// <summary>
    /// Gets the views directory path
    /// </summary>
    public string ViewsDirectory => Path.Combine(_outputPath, "views");

    /// <summary>
    /// Gets the procedures directory path
    /// </summary>
    public string ProceduresDirectory => Path.Combine(_outputPath, "procedures");

    /// <summary>
    /// Gets the functions directory path
    /// </summary>
    public string FunctionsDirectory => Path.Combine(_outputPath, "functions");

    /// <summary>
    /// Gets the triggers directory path
    /// </summary>
    public string TriggersDirectory => Path.Combine(_outputPath, "triggers");

    /// <summary>
    /// Gets the data directory path
    /// </summary>
    public string DataDirectory => Path.Combine(_outputPath, "data");

    /// <summary>
    /// Gets the path for a specific object type directory.
    /// </summary>
    public string GetDirectoryPath(string objectType)
    {
        return Path.Combine(_outputPath, objectType.ToLowerInvariant());
    }

    /// <summary>
    /// Gets the full file path for an object script.
    /// </summary>
    /// <param name="objectType">Type of object (tables, views, etc.)</param>
    /// <param name="schema">Schema name</param>
    /// <param name="objectName">Object name</param>
    /// <returns>Full file path</returns>
    public string GetFilePath(string objectType, string schema, string objectName)
    {
        var directory = GetDirectoryPath(objectType);
        var fileName = $"{schema}.{objectName}.sql";
        return Path.Combine(directory, fileName);
    }

    /// <summary>
    /// Writes the execution order file.
    /// </summary>
    public void WriteExecutionOrder(IEnumerable<string> orderedScripts)
    {
        var executionOrderPath = Path.Combine(_outputPath, "_execution_order.txt");
        File.WriteAllLines(executionOrderPath, orderedScripts);
    }

    /// <summary>
    /// Cleans the output directory (removes all files).
    /// </summary>
    public void CleanOutput()
    {
        if (Directory.Exists(_outputPath))
        {
            Directory.Delete(_outputPath, recursive: true);
        }
    }

    /// <summary>
    /// Gets the main output directory path.
    /// </summary>
    public string OutputPath => _outputPath;
}
