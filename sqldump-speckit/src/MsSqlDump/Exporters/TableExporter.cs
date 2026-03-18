using MsSqlDump.Models;
using MsSqlDump.Utils;
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;

namespace MsSqlDump.Exporters;

/// <summary>
/// Exports table schemas to SQL CREATE TABLE scripts
/// </summary>
public class TableExporter
{
    private readonly int _sqlVersion;

    public TableExporter(int sqlVersion)
    {
        _sqlVersion = sqlVersion;
    }

    /// <summary>
    /// Generates a complete CREATE TABLE script with all columns and inline constraints.
    /// </summary>
    /// <param name="table">The table metadata containing schema and column information.</param>
    /// <returns>A SQL script to create the table structure.</returns>
    public string GenerateCreateTableScript(TableMetadata table)
    {
        var sb = new StringBuilder();
        
        // Add DROP statement (version-aware)
        sb.AppendLine(GenerateDropScript(table));
        sb.AppendLine("GO");
        sb.AppendLine();
        
        // CREATE TABLE header
        sb.AppendLine($"CREATE TABLE {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)}");
        sb.AppendLine("(");
        
        // Column definitions
        var columnDefs = new List<string>();
        foreach (var column in table.Columns.OrderBy(c => c.OrdinalPosition))
        {
            columnDefs.Add("    " + GenerateColumnDefinition(column));
        }
        
        // Add primary key constraint inline if it exists
        if (table.PrimaryKey != null)
        {
            columnDefs.Add("    " + GeneratePrimaryKeyInline(table.PrimaryKey));
        }
        
        sb.AppendLine(string.Join(",\n", columnDefs));
        sb.AppendLine(");");
        
        return sb.ToString();
    }

    /// <summary>
    /// Generates column definition for CREATE TABLE
    /// </summary>
    private string GenerateColumnDefinition(ColumnMetadata column)
    {
        var sb = new StringBuilder();
        
        sb.Append(SqlQuoter.QuoteIdentifier(column.ColumnName));
        sb.Append(" ");
        
        // Data type with size/precision
        sb.Append(GetDataTypeDefinition(column));
        
        // Collation (for string types)
        if (!string.IsNullOrEmpty(column.Collation))
        {
            sb.Append($" COLLATE {column.Collation}");
        }
        
        // Computed column
        if (column.IsComputed && !string.IsNullOrEmpty(column.ComputedExpression))
        {
            sb.Append($" AS {column.ComputedExpression}");
            return sb.ToString();
        }
        
        // Identity
        if (column.IsIdentity)
        {
            sb.Append($" IDENTITY({column.IdentitySeed},{column.IdentityIncrement})");
        }
        
        // Nullability
        sb.Append(column.IsNullable ? " NULL" : " NOT NULL");
        
        // Default value (only if not a DEFAULT constraint - those are handled separately)
        if (!string.IsNullOrEmpty(column.DefaultValue) && !column.DefaultValue.Contains("CREATE DEFAULT"))
        {
            sb.Append($" DEFAULT {column.DefaultValue}");
        }
        
        return sb.ToString();
    }

    /// <summary>
    /// Gets the data type definition with appropriate length/precision/scale
    /// </summary>
    private string GetDataTypeDefinition(ColumnMetadata column)
    {
        var dataType = column.DataType.ToLower();
        
        // String types with length
        if (dataType is "char" or "varchar" or "nchar" or "nvarchar")
        {
            if (column.MaxLength == -1)
                return $"{column.DataType}(MAX)";
            
            // nchar and nvarchar use half the bytes
            var length = dataType.StartsWith("n") ? column.MaxLength / 2 : column.MaxLength;
            return $"{column.DataType}({length})";
        }
        
        // Binary types with length
        if (dataType is "binary" or "varbinary")
        {
            if (column.MaxLength == -1)
                return $"{column.DataType}(MAX)";
            
            return $"{column.DataType}({column.MaxLength})";
        }
        
        // Decimal/numeric types with precision and scale
        if (dataType is "decimal" or "numeric")
        {
            return $"{column.DataType}({column.Precision},{column.Scale})";
        }
        
        // Time types with scale
        if (dataType is "time" or "datetime2" or "datetimeoffset")
        {
            return $"{column.DataType}({column.Scale})";
        }
        
        // All other types (int, bigint, date, etc.)
        return column.DataType;
    }

    /// <summary>
    /// Generates inline primary key constraint
    /// </summary>
    private string GeneratePrimaryKeyInline(ConstraintMetadata pk)
    {
        var columns = string.Join(", ", pk.Columns.Select(SqlQuoter.QuoteIdentifier));
        return $"CONSTRAINT {SqlQuoter.QuoteIdentifier(pk.ConstraintName)} PRIMARY KEY ({columns})";
    }

    /// <summary>
    /// <summary>
    /// Generates ALTER TABLE ADD CONSTRAINT scripts for foreign keys, unique constraints, check constraints, and default constraints.
    /// </summary>
    /// <param name="table">The table metadata containing constraint information.</param>
    /// <returns>A list of SQL scripts to add table constraints.</returns>
    public List<string> GenerateConstraintScripts(TableMetadata table)
    {
        var scripts = new List<string>();
        
        // Foreign keys
        foreach (var fk in table.ForeignKeys)
        {
            var sb = new StringBuilder();
            sb.Append($"ALTER TABLE {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} ");
            sb.Append($"ADD CONSTRAINT {SqlQuoter.QuoteIdentifier(fk.ConstraintName)} ");
            sb.Append($"FOREIGN KEY ({string.Join(", ", fk.Columns.Select(SqlQuoter.QuoteIdentifier))}) ");
            sb.Append($"REFERENCES {SqlQuoter.QuoteIdentifier(fk.ReferencedSchema)}.{SqlQuoter.QuoteIdentifier(fk.ReferencedTable)} ");
            sb.Append($"({string.Join(", ", fk.ReferencedColumns!.Select(SqlQuoter.QuoteIdentifier))})");
            
            if (fk.OnDeleteAction != null && fk.OnDeleteAction != "NO_ACTION")
            {
                sb.Append($" ON DELETE {fk.OnDeleteAction.Replace("_", " ")}");
            }
            
            if (fk.OnUpdateAction != null && fk.OnUpdateAction != "NO_ACTION")
            {
                sb.Append($" ON UPDATE {fk.OnUpdateAction.Replace("_", " ")}");
            }
            
            sb.Append(";");
            scripts.Add(sb.ToString());
        }
        
        // Unique constraints
        foreach (var uc in table.UniqueConstraints)
        {
            var sb = new StringBuilder();
            sb.Append($"ALTER TABLE {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} ");
            sb.Append($"ADD CONSTRAINT {SqlQuoter.QuoteIdentifier(uc.ConstraintName)} ");
            sb.Append($"UNIQUE ({string.Join(", ", uc.Columns.Select(SqlQuoter.QuoteIdentifier))});");
            scripts.Add(sb.ToString());
        }
        
        // Check constraints
        foreach (var cc in table.CheckConstraints)
        {
            var sb = new StringBuilder();
            sb.Append($"ALTER TABLE {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} ");
            sb.Append($"ADD CONSTRAINT {SqlQuoter.QuoteIdentifier(cc.ConstraintName)} ");
            sb.Append($"CHECK {cc.CheckExpression};");
            scripts.Add(sb.ToString());
        }
        
        // Default constraints (only named defaults that weren't inline)
        foreach (var dc in table.DefaultConstraints)
        {
            var sb = new StringBuilder();
            sb.Append($"ALTER TABLE {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} ");
            sb.Append($"ADD CONSTRAINT {SqlQuoter.QuoteIdentifier(dc.ConstraintName)} ");
            sb.Append($"DEFAULT {dc.DefaultExpression} FOR {SqlQuoter.QuoteIdentifier(dc.Columns[0])};");
            scripts.Add(sb.ToString());
        }
        
        return scripts;
    }

    /// <summary>
    /// Generates CREATE INDEX scripts for all non-primary key indexes (clustered and non-clustered).
    /// </summary>
    /// <param name="table">The table metadata containing index information.</param>
    /// <returns>A list of SQL scripts to create table indexes.</returns>
    public List<string> GenerateIndexScripts(TableMetadata table)
    {
        var scripts = new List<string>();
        
        foreach (var index in table.Indexes.Where(i => !i.IsPrimaryKey))
        {
            var sb = new StringBuilder();
            
            // CREATE [UNIQUE] [CLUSTERED|NONCLUSTERED] INDEX
            sb.Append("CREATE ");
            if (index.IsUnique)
                sb.Append("UNIQUE ");
            
            sb.Append(index.IsClustered ? "CLUSTERED " : "NONCLUSTERED ");
            sb.Append($"INDEX {SqlQuoter.QuoteIdentifier(index.IndexName)} ");
            sb.Append($"ON {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)} ");
            
            // Key columns
            var keyColumns = string.Join(", ", index.Columns.Select(c => 
                SqlQuoter.QuoteIdentifier(c.ColumnName) + (c.IsDescending ? " DESC" : " ASC")));
            sb.Append($"({keyColumns})");
            
            // Included columns
            if (index.IncludedColumns.Any())
            {
                var includedColumns = string.Join(", ", index.IncludedColumns.Select(SqlQuoter.QuoteIdentifier));
                sb.Append($" INCLUDE ({includedColumns})");
            }
            
            // Filter expression (SQL 2008+)
            if (!string.IsNullOrEmpty(index.FilterExpression))
            {
                sb.Append($" WHERE {index.FilterExpression}");
            }
            
            sb.Append(";");
            scripts.Add(sb.ToString());
        }
        
        return scripts;
    }

    /// <summary>
    /// Generates a version-aware DROP TABLE script using IF EXISTS syntax when supported.
    /// </summary>
    /// <param name="table">The table metadata for the table to drop.</param>
    /// <returns>A SQL script to safely drop the table if it exists.</returns>
    public string GenerateDropScript(TableMetadata table)
    {
        // SQL 2016+ supports DROP TABLE IF EXISTS
        if (_sqlVersion >= 13)
        {
            return $"DROP TABLE IF EXISTS {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)};";
        }
        
        // SQL 2008-2014: Use IF EXISTS pattern
        var sb = new StringBuilder();
        sb.AppendLine($"IF OBJECT_ID('{table.SchemaName}.{table.TableName}', 'U') IS NOT NULL");
        sb.Append($"    DROP TABLE {SqlQuoter.QuoteIdentifier(table.SchemaName)}.{SqlQuoter.QuoteIdentifier(table.TableName)};");
        return sb.ToString();
    }
}
