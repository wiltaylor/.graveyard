using Microsoft.Data.SqlClient;
using MsSqlDump.Core;
using MsSqlDump.Models;
using MsSqlDump.Utils;
using System;
using System.Collections.Generic;
using System.Data;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace MsSqlDump.Exporters;

/// <summary>
/// Exports table data to SQL INSERT statements
/// </summary>
public class DataExporter
{
    private readonly DatabaseConnection _connection;
    private readonly int _batchSize;

    public DataExporter(DatabaseConnection connection, int batchSize = 1000)
    {
        _connection = connection ?? throw new ArgumentNullException(nameof(connection));
        _batchSize = batchSize;
    }

    /// <summary>
    /// Exports table data as batched INSERT statements with proper data type formatting and identity column handling.
    /// </summary>
    /// <param name="table">The table metadata containing schema and row count information.</param>
    /// <returns>A SQL script containing INSERT statements for all table data.</returns>
    public async Task<string> ExportTableDataAsync(TableMetadata table)
    {
        var sb = new StringBuilder();
        
        // Check if table has identity columns
        var hasIdentity = table.Columns.Any(c => c.IsIdentity);
        
        if (hasIdentity)
        {
            sb.AppendLine($"SET IDENTITY_INSERT {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} ON;");
            sb.AppendLine("GO");
            sb.AppendLine();
        }

        // Get non-computed columns only
        var dataColumns = table.Columns.Where(c => !c.IsComputed).OrderBy(c => c.OrdinalPosition).ToList();
        
        if (dataColumns.Count == 0 || table.RowCount == 0)
        {
            if (hasIdentity)
            {
                sb.AppendLine($"SET IDENTITY_INSERT {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} OFF;");
                sb.AppendLine("GO");
            }
            return sb.ToString();
        }

        // Build SELECT query
        var columnList = string.Join(", ", dataColumns.Select(c => SqlQuoter.QuoteIdentifier(c.ColumnName)));
        var query = $"SELECT {columnList} FROM {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)}";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        command.CommandTimeout = 300; // 5 minutes for large tables
        
        await using var reader = await command.ExecuteReaderAsync();
        
        var rowCount = 0;
        var currentBatch = new List<string>();

        while (await reader.ReadAsync())
        {
            var values = new List<string>();
            
            for (int i = 0; i < dataColumns.Count; i++)
            {
                values.Add(FormatValue(reader, i, dataColumns[i]));
            }
            
            currentBatch.Add($"({string.Join(", ", values)})");
            rowCount++;

            // Write batch when full
            if (currentBatch.Count >= _batchSize)
            {
                sb.AppendLine(GenerateInsertStatement(table, dataColumns, currentBatch));
                sb.AppendLine("GO");
                sb.AppendLine();
                currentBatch.Clear();
            }
        }

        // Write remaining rows
        if (currentBatch.Any())
        {
            sb.AppendLine(GenerateInsertStatement(table, dataColumns, currentBatch));
            sb.AppendLine("GO");
            sb.AppendLine();
        }

        if (hasIdentity)
        {
            sb.AppendLine($"SET IDENTITY_INSERT {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} OFF;");
            sb.AppendLine("GO");
        }

        return sb.ToString();
    }

    /// <summary>
    /// Generates an INSERT statement for a batch of rows
    /// </summary>
    private string GenerateInsertStatement(TableMetadata table, List<ColumnMetadata> columns, List<string> valueSets)
    {
        var sb = new StringBuilder();
        
        sb.Append($"INSERT INTO {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} ");
        sb.Append($"({string.Join(", ", columns.Select(c => SqlQuoter.QuoteIdentifier(c.ColumnName)))})");
        sb.AppendLine();
        sb.Append("VALUES ");
        sb.AppendLine();
        sb.Append(string.Join(",\n", valueSets));
        sb.Append(";");
        
        return sb.ToString();
    }

    /// <summary>
    /// Formats a value for SQL INSERT statement with proper escaping
    /// </summary>
    private string FormatValue(SqlDataReader reader, int ordinal, ColumnMetadata column)
    {
        if (reader.IsDBNull(ordinal))
            return "NULL";

        var dataType = column.DataType.ToLower();

        try
        {
            switch (dataType)
            {
                // Integer types
                case "bit":
                    return reader.GetBoolean(ordinal) ? "1" : "0";
                
                case "tinyint":
                    return reader.GetByte(ordinal).ToString();
                
                case "smallint":
                    return reader.GetInt16(ordinal).ToString();
                
                case "int":
                    return reader.GetInt32(ordinal).ToString();
                
                case "bigint":
                    return reader.GetInt64(ordinal).ToString();

                // Decimal types
                case "decimal":
                case "numeric":
                case "money":
                case "smallmoney":
                    return reader.GetDecimal(ordinal).ToString(System.Globalization.CultureInfo.InvariantCulture);

                // Float types
                case "real":
                    return reader.GetFloat(ordinal).ToString(System.Globalization.CultureInfo.InvariantCulture);
                
                case "float":
                    return reader.GetDouble(ordinal).ToString(System.Globalization.CultureInfo.InvariantCulture);

                // String types
                case "char":
                case "varchar":
                case "text":
                case "nchar":
                case "nvarchar":
                case "ntext":
                case "xml":
                    var stringValue = reader.GetString(ordinal);
                    return "N'" + EscapeString(stringValue) + "'";

                // Date/Time types
                case "date":
                case "datetime":
                case "datetime2":
                case "smalldatetime":
                    var dateValue = reader.GetDateTime(ordinal);
                    return "'" + dateValue.ToString("yyyy-MM-dd HH:mm:ss.fff") + "'";

                case "time":
                    var timeValue = reader.GetTimeSpan(ordinal);
                    return "'" + timeValue.ToString(@"hh\:mm\:ss\.fffffff") + "'";

                case "datetimeoffset":
                    var dtoValue = reader.GetDateTimeOffset(ordinal);
                    return "'" + dtoValue.ToString("yyyy-MM-dd HH:mm:ss.fffffff zzz") + "'";

                // Binary types
                case "binary":
                case "varbinary":
                case "image":
                case "timestamp":
                case "rowversion":
                    var bytes = (byte[])reader.GetValue(ordinal);
                    return "0x" + BitConverter.ToString(bytes).Replace("-", "");

                // GUID
                case "uniqueidentifier":
                    return "'" + reader.GetGuid(ordinal).ToString() + "'";

                // Spatial types (convert to WKT)
                case "geography":
                case "geometry":
                    var spatial = reader.GetValue(ordinal);
                    return $"'{spatial.ToString()}'";

                // Hierarchyid
                case "hierarchyid":
                    var hid = reader.GetValue(ordinal);
                    return $"'{hid.ToString()}'";

                // Default: get as string
                default:
                    var value = reader.GetValue(ordinal);
                    if (value is string str)
                        return "N'" + EscapeString(str) + "'";
                    return "'" + value.ToString()?.Replace("'", "''") + "'";
            }
        }
        catch (Exception)
        {
            // Fallback: try to get as string
            try
            {
                var fallbackValue = reader.GetValue(ordinal)?.ToString() ?? "NULL";
                if (fallbackValue == "NULL")
                    return "NULL";
                return "N'" + EscapeString(fallbackValue) + "'";
            }
            catch
            {
                return "NULL";
            }
        }
    }

    /// <summary>
    /// Escapes single quotes and special characters in strings
    /// </summary>
    private string EscapeString(string value)
    {
        if (string.IsNullOrEmpty(value))
            return value;

        return value.Replace("'", "''");
    }

    /// <summary>
    /// Generates scripts to disable foreign key constraints for a table (used for circular dependency handling).
    /// </summary>
    /// <param name="table">The table metadata containing foreign key information.</param>
    /// <returns>A SQL script to disable all foreign key constraints on the table.</returns>
    public string GenerateDisableForeignKeysScript(TableMetadata table)
    {
        var sb = new StringBuilder();
        
        foreach (var fk in table.ForeignKeys)
        {
            sb.AppendLine($"ALTER TABLE {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} NOCHECK CONSTRAINT {SqlQuoter.QuoteIdentifier(fk.ConstraintName)};");
        }
        
        return sb.ToString();
    }

    /// <summary>
    /// Generates scripts to re-enable foreign key constraints for a table (used after data import with circular dependencies).
    /// </summary>
    /// <param name="table">The table metadata containing foreign key information.</param>
    /// <returns>A SQL script to enable all foreign key constraints on the table.</returns>
    public string GenerateEnableForeignKeysScript(TableMetadata table)
    {
        var sb = new StringBuilder();
        
        foreach (var fk in table.ForeignKeys)
        {
            sb.AppendLine($"ALTER TABLE {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} CHECK CONSTRAINT {SqlQuoter.QuoteIdentifier(fk.ConstraintName)};");
        }
        
        return sb.ToString();
    }
}
