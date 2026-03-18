using MsSqlDump.Models;
using MsSqlDump.Utils;
using System.Text;

namespace MsSqlDump.Exporters;

/// <summary>
/// Exports stored procedures to SQL CREATE PROCEDURE scripts with version-aware syntax.
/// </summary>
public class ProcedureExporter
{
    private readonly int _sqlVersion;

    public ProcedureExporter(int sqlVersion)
    {
        _sqlVersion = sqlVersion;
    }

    /// <summary>
    /// Generates a CREATE PROCEDURE script with version-aware DROP IF EXISTS pattern.
    /// </summary>
    /// <param name="procedure">The programmable object representing the stored procedure.</param>
    /// <returns>A SQL script to create the stored procedure.</returns>
    public string GenerateCreateProcedureScript(ProgrammableObject procedure)
    {
        if (procedure.ObjectType != ProgrammableObjectType.StoredProcedure)
        {
            throw new ArgumentException("Object must be a stored procedure", nameof(procedure));
        }

        var sb = new StringBuilder();

        // Add DROP statement (version-aware)
        sb.AppendLine(GenerateDropScript(procedure));
        sb.AppendLine("GO");
        sb.AppendLine();

        // Add CREATE PROCEDURE from definition
        // The definition from sys.sql_modules already includes CREATE PROCEDURE
        sb.AppendLine(procedure.Definition);
        sb.AppendLine("GO");

        return sb.ToString();
    }

    /// <summary>
    /// Generates version-aware DROP PROCEDURE script
    /// </summary>
    private string GenerateDropScript(ProgrammableObject procedure)
    {
        // SQL 2016+ supports DROP PROCEDURE IF EXISTS
        if (_sqlVersion >= 13)
        {
            return $"DROP PROCEDURE IF EXISTS {SqlQuoter.QuoteIdentifier(procedure.SchemaName)}.{SqlQuoter.QuoteIdentifier(procedure.ObjectName)};";
        }

        // SQL 2008-2014: Use IF EXISTS pattern
        var sb = new StringBuilder();
        sb.AppendLine($"IF OBJECT_ID('{procedure.SchemaName}.{procedure.ObjectName}', 'P') IS NOT NULL");
        sb.Append($"    DROP PROCEDURE {SqlQuoter.QuoteIdentifier(procedure.SchemaName)}.{SqlQuoter.QuoteIdentifier(procedure.ObjectName)};");
        return sb.ToString();
    }
}
