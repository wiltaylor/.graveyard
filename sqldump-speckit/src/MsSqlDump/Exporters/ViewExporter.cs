using MsSqlDump.Models;
using MsSqlDump.Utils;
using System.Text;

namespace MsSqlDump.Exporters;

/// <summary>
/// Exports database views to SQL CREATE VIEW scripts with version-aware syntax.
/// </summary>
public class ViewExporter
{
    private readonly int _sqlVersion;

    public ViewExporter(int sqlVersion)
    {
        _sqlVersion = sqlVersion;
    }

    /// <summary>
    /// Generates a CREATE VIEW script with version-aware DROP IF EXISTS pattern.
    /// </summary>
    /// <param name="view">The programmable object representing the view.</param>
    /// <returns>A SQL script to create the view.</returns>
    public string GenerateCreateViewScript(ProgrammableObject view)
    {
        if (view.ObjectType != ProgrammableObjectType.View)
        {
            throw new ArgumentException("Object must be a view", nameof(view));
        }

        var sb = new StringBuilder();

        // Add DROP statement (version-aware)
        sb.AppendLine(GenerateDropScript(view));
        sb.AppendLine("GO");
        sb.AppendLine();

        // Add CREATE VIEW from definition
        // The definition from sys.sql_modules already includes CREATE VIEW
        sb.AppendLine(view.Definition);
        sb.AppendLine("GO");

        return sb.ToString();
    }

    /// <summary>
    /// Generates version-aware DROP VIEW script
    /// </summary>
    private string GenerateDropScript(ProgrammableObject view)
    {
        // SQL 2016+ supports DROP VIEW IF EXISTS
        if (_sqlVersion >= 13)
        {
            return $"DROP VIEW IF EXISTS {SqlQuoter.QuoteIdentifier(view.SchemaName)}.{SqlQuoter.QuoteIdentifier(view.ObjectName)};";
        }

        // SQL 2008-2014: Use IF EXISTS pattern
        var sb = new StringBuilder();
        sb.AppendLine($"IF OBJECT_ID('{view.SchemaName}.{view.ObjectName}', 'V') IS NOT NULL");
        sb.Append($"    DROP VIEW {SqlQuoter.QuoteIdentifier(view.SchemaName)}.{SqlQuoter.QuoteIdentifier(view.ObjectName)};");
        return sb.ToString();
    }
}
