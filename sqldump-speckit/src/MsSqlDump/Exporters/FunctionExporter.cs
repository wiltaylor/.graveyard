using MsSqlDump.Models;
using MsSqlDump.Utils;
using System.Text;

namespace MsSqlDump.Exporters;

/// <summary>
/// Exports user-defined functions (scalar, inline table-valued, and multi-statement table-valued) to SQL CREATE FUNCTION scripts with version-aware syntax.
/// </summary>
public class FunctionExporter
{
    private readonly int _sqlVersion;

    public FunctionExporter(int sqlVersion)
    {
        _sqlVersion = sqlVersion;
    }

    /// <summary>
    /// Generates a CREATE FUNCTION script with version-aware DROP IF EXISTS pattern.
    /// </summary>
    /// <param name="function">The programmable object representing the function (scalar, inline table-valued, or multi-statement table-valued).</param>
    /// <returns>A SQL script to create the function.</returns>
    public string GenerateCreateFunctionScript(ProgrammableObject function)
    {
        if (function.ObjectType is not (ProgrammableObjectType.ScalarFunction 
            or ProgrammableObjectType.InlineTableValuedFunction 
            or ProgrammableObjectType.TableValuedFunction))
        {
            throw new ArgumentException("Object must be a function", nameof(function));
        }

        var sb = new StringBuilder();

        // Add DROP statement (version-aware)
        sb.AppendLine(GenerateDropScript(function));
        sb.AppendLine("GO");
        sb.AppendLine();

        // Add CREATE FUNCTION from definition
        // The definition from sys.sql_modules already includes CREATE FUNCTION
        sb.AppendLine(function.Definition);
        sb.AppendLine("GO");

        return sb.ToString();
    }

    /// <summary>
    /// Generates version-aware DROP FUNCTION script
    /// </summary>
    private string GenerateDropScript(ProgrammableObject function)
    {
        // SQL 2016+ supports DROP FUNCTION IF EXISTS
        if (_sqlVersion >= 13)
        {
            return $"DROP FUNCTION IF EXISTS {SqlQuoter.QuoteIdentifier(function.SchemaName)}.{SqlQuoter.QuoteIdentifier(function.ObjectName)};";
        }

        // SQL 2008-2014: Use IF EXISTS pattern
        var sb = new StringBuilder();
        sb.AppendLine($"IF OBJECT_ID('{function.SchemaName}.{function.ObjectName}', 'FN') IS NOT NULL");
        sb.Append($"    DROP FUNCTION {SqlQuoter.QuoteIdentifier(function.SchemaName)}.{SqlQuoter.QuoteIdentifier(function.ObjectName)};");
        return sb.ToString();
    }
}
