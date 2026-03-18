using System.Text;

namespace MsSqlDump.Writers;

/// <summary>
/// Writes SQL scripts to files with UTF-8 encoding.
/// </summary>
public class ScriptWriter
{
    /// <summary>
    /// Writes a SQL script to a file.
    /// </summary>
    /// <param name="filePath">Full path to the output file</param>
    /// <param name="content">SQL script content</param>
    public async Task WriteScriptAsync(string filePath, string content)
    {
        if (string.IsNullOrWhiteSpace(filePath))
            throw new ArgumentException("File path cannot be empty", nameof(filePath));

        // Ensure directory exists
        var directory = Path.GetDirectoryName(filePath);
        if (!string.IsNullOrEmpty(directory))
        {
            Directory.CreateDirectory(directory);
        }

        // Write with UTF-8 encoding (with BOM for better compatibility)
        await File.WriteAllTextAsync(filePath, content, Encoding.UTF8);
    }

    /// <summary>
    /// Writes multiple SQL scripts in a batch.
    /// </summary>
    public async Task WriteScriptsAsync(Dictionary<string, string> scripts)
    {
        var tasks = scripts.Select(kvp => WriteScriptAsync(kvp.Key, kvp.Value));
        await Task.WhenAll(tasks);
    }

    /// <summary>
    /// Appends content to an existing file or creates it if it doesn't exist.
    /// </summary>
    public async Task AppendScriptAsync(string filePath, string content)
    {
        if (string.IsNullOrWhiteSpace(filePath))
            throw new ArgumentException("File path cannot be empty", nameof(filePath));

        // Ensure directory exists
        var directory = Path.GetDirectoryName(filePath);
        if (!string.IsNullOrEmpty(directory))
        {
            Directory.CreateDirectory(directory);
        }

        await File.AppendAllTextAsync(filePath, content, Encoding.UTF8);
    }
}
