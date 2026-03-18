namespace MsSqlDump.Utils;

/// <summary>
/// Utility for quoting SQL Server identifiers with proper escaping.
/// </summary>
public static class SqlQuoter
{
    /// <summary>
    /// Quotes a SQL identifier using [...] brackets with proper escaping.
    /// </summary>
    /// <param name="identifier">The identifier to quote (table name, column name, etc.)</param>
    /// <returns>Quoted identifier with escaped brackets</returns>
    public static string Quote(string identifier)
    {
        if (string.IsNullOrWhiteSpace(identifier))
            throw new ArgumentException("Identifier cannot be null or whitespace", nameof(identifier));

        // Escape any existing ] characters by doubling them
        var escaped = identifier.Replace("]", "]]");
        
        return $"[{escaped}]";
    }

    /// <summary>
    /// Quotes a SQL identifier using [...] brackets with proper escaping.
    /// Alias for Quote method for compatibility.
    /// </summary>
    /// <param name="identifier">The identifier to quote (table name, column name, etc.)</param>
    /// <returns>Quoted identifier with escaped brackets</returns>
    public static string QuoteIdentifier(string identifier)
    {
        return Quote(identifier);
    }

    /// <summary>
    /// Quotes a schema and object name as a fully qualified identifier.
    /// </summary>
    /// <param name="schema">Schema name</param>
    /// <param name="objectName">Object name (table, view, etc.)</param>
    /// <returns>Fully qualified quoted identifier (e.g., [dbo].[MyTable])</returns>
    public static string QuoteFull(string schema, string objectName)
    {
        if (string.IsNullOrWhiteSpace(schema))
            throw new ArgumentException("Schema cannot be null or whitespace", nameof(schema));
        if (string.IsNullOrWhiteSpace(objectName))
            throw new ArgumentException("Object name cannot be null or whitespace", nameof(objectName));

        return $"{Quote(schema)}.{Quote(objectName)}";
    }

    /// <summary>
    /// Escapes a string value for use in SQL (single quote escaping).
    /// </summary>
    /// <param name="value">The string value to escape</param>
    /// <returns>Escaped string (single quotes doubled)</returns>
    public static string EscapeString(string value)
    {
        if (value == null)
            return "NULL";

        // Escape single quotes by doubling them
        return value.Replace("'", "''");
    }

    /// <summary>
    /// Wraps a string value in single quotes with proper escaping.
    /// </summary>
    /// <param name="value">The string value</param>
    /// <returns>Quoted and escaped string ready for SQL</returns>
    public static string QuoteString(string? value)
    {
        if (value == null)
            return "NULL";

        return $"'{EscapeString(value)}'";
    }
}
