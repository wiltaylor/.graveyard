using MsSqlDump.Models;
using MsSqlDump.Utils;
using System.Text;

namespace MsSqlDump.Exporters;

/// <summary>
/// Exports database triggers to SQL CREATE TRIGGER scripts with version-aware syntax and parent table association.
/// </summary>
public class TriggerExporter
{
    private readonly int _sqlVersion;

    public TriggerExporter(int sqlVersion)
    {
        _sqlVersion = sqlVersion;
    }

    /// <summary>
    /// Generates a CREATE TRIGGER script with version-aware DROP IF EXISTS pattern.
    /// </summary>
    /// <param name="trigger">The programmable object representing the trigger with parent table information.</param>
    /// <returns>A SQL script to create the trigger.</returns>
    public string GenerateCreateTriggerScript(ProgrammableObject trigger)
    {
        if (trigger.ObjectType != ProgrammableObjectType.Trigger)
        {
            throw new ArgumentException("Object must be a trigger", nameof(trigger));
        }

        var sb = new StringBuilder();

        // Add comment about parent table
        if (!string.IsNullOrEmpty(trigger.ParentTableFullName))
        {
            sb.AppendLine($"-- Trigger on table: {trigger.ParentTableFullName}");
        }

        // Add DROP statement (version-aware)
        sb.AppendLine(GenerateDropScript(trigger));
        sb.AppendLine("GO");
        sb.AppendLine();

        // Add CREATE TRIGGER from definition
        // The definition from sys.sql_modules already includes CREATE TRIGGER
        sb.AppendLine(trigger.Definition);
        sb.AppendLine("GO");

        return sb.ToString();
    }

    /// <summary>
    /// Generates version-aware DROP TRIGGER script
    /// </summary>
    private string GenerateDropScript(ProgrammableObject trigger)
    {
        // SQL 2016+ supports DROP TRIGGER IF EXISTS
        if (_sqlVersion >= 13)
        {
            return $"DROP TRIGGER IF EXISTS {SqlQuoter.QuoteIdentifier(trigger.SchemaName)}.{SqlQuoter.QuoteIdentifier(trigger.ObjectName)};";
        }

        // SQL 2008-2014: Use IF EXISTS pattern
        var sb = new StringBuilder();
        sb.AppendLine($"IF OBJECT_ID('{trigger.SchemaName}.{trigger.ObjectName}', 'TR') IS NOT NULL");
        sb.Append($"    DROP TRIGGER {SqlQuoter.QuoteIdentifier(trigger.SchemaName)}.{SqlQuoter.QuoteIdentifier(trigger.ObjectName)};");
        return sb.ToString();
    }
}
