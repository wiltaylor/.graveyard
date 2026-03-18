using Microsoft.Data.SqlClient;
using MsSqlDump.Models;
using System;
using System.Collections.Generic;
using System.Data;
using System.Threading.Tasks;

namespace MsSqlDump.Core;

/// <summary>
/// Reads database schema metadata from SQL Server system tables and views
/// </summary>
public class SchemaReader
{
    private readonly DatabaseConnection _connection;
    private readonly SqlVersionDetector _versionDetector;

    public SchemaReader(DatabaseConnection connection, SqlVersionDetector versionDetector)
    {
        _connection = connection ?? throw new ArgumentNullException(nameof(connection));
        _versionDetector = versionDetector ?? throw new ArgumentNullException(nameof(versionDetector));
    }

    /// <summary>
    /// Reads all table metadata including columns, constraints, and indexes.
    /// </summary>
    /// <param name="filter">Optional filter to include/exclude specific tables based on pattern matching.</param>
    /// <returns>A list of table metadata objects with complete schema information.</returns>
    public async Task<List<TableMetadata>> ReadTableMetadataAsync(Utils.ObjectFilter? filter = null)
    {
        var tables = new List<TableMetadata>();
        
        // First, get all tables
        var tableList = await GetTablesAsync();
        
        // For each table, read complete metadata
        foreach (var table in tableList)
        {
            // Apply filter if provided
            if (filter != null && !filter.ShouldInclude(table.SchemaName, table.TableName))
            {
                continue;
            }

            var columns = await ReadColumnsAsync(table.SchemaName, table.TableName);
            var primaryKey = await ReadPrimaryKeyAsync(table.SchemaName, table.TableName);
            var foreignKeys = await ReadForeignKeysAsync(table.SchemaName, table.TableName);
            var uniqueConstraints = await ReadUniqueConstraintsAsync(table.SchemaName, table.TableName);
            var checkConstraints = await ReadCheckConstraintsAsync(table.SchemaName, table.TableName);
            var defaultConstraints = await ReadDefaultConstraintsAsync(table.SchemaName, table.TableName);
            var indexes = await ReadIndexesAsync(table.SchemaName, table.TableName);
            
            table.Columns = columns;
            table.PrimaryKey = primaryKey;
            table.ForeignKeys = foreignKeys;
            table.UniqueConstraints = uniqueConstraints;
            table.CheckConstraints = checkConstraints;
            table.DefaultConstraints = defaultConstraints;
            table.Indexes = indexes;
            
            tables.Add(table);
        }
        
        return tables;
    }

    private async Task<List<TableMetadata>> GetTablesAsync()
    {
        var tables = new List<TableMetadata>();
        
        const string query = @"
            SELECT 
                s.name AS SchemaName,
                t.name AS TableName,
                COALESCE(ps.[RowCount], 0) AS [RowCount]
            FROM sys.tables t
            INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
            LEFT JOIN (
                SELECT 
                    object_id,
                    SUM(row_count) AS [RowCount]
                FROM sys.dm_db_partition_stats
                WHERE index_id IN (0, 1)
                GROUP BY object_id
            ) ps ON t.object_id = ps.object_id
            ORDER BY s.name, t.name";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            tables.Add(new TableMetadata
            {
                SchemaName = reader.GetString(0),
                TableName = reader.GetString(1),
                RowCount = reader.GetInt64(2),
                Columns = new List<ColumnMetadata>()
            });
        }
        
        return tables;
    }

    private async Task<List<ColumnMetadata>> ReadColumnsAsync(string schemaName, string tableName)
    {
        var columns = new List<ColumnMetadata>();
        
        const string query = @"
            SELECT 
                c.name AS ColumnName,
                c.column_id AS OrdinalPosition,
                TYPE_NAME(c.user_type_id) AS DataType,
                c.max_length AS MaxLength,
                c.precision AS Precision,
                c.scale AS Scale,
                c.is_nullable AS IsNullable,
                dc.definition AS DefaultValue,
                c.is_identity AS IsIdentity,
                CAST(ISNULL(ic.seed_value, 0) AS BIGINT) AS IdentitySeed,
                CAST(ISNULL(ic.increment_value, 0) AS BIGINT) AS IdentityIncrement,
                c.is_computed AS IsComputed,
                cc.definition AS ComputedExpression,
                c.collation_name AS Collation
            FROM sys.columns c
            INNER JOIN sys.tables t ON c.object_id = t.object_id
            INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
            LEFT JOIN sys.default_constraints dc ON c.default_object_id = dc.object_id
            LEFT JOIN sys.identity_columns ic ON c.object_id = ic.object_id AND c.column_id = ic.column_id
            LEFT JOIN sys.computed_columns cc ON c.object_id = cc.object_id AND c.column_id = cc.column_id
            WHERE s.name = @SchemaName AND t.name = @TableName
            ORDER BY c.column_id";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        command.Parameters.AddWithValue("@SchemaName", schemaName);
        command.Parameters.AddWithValue("@TableName", tableName);
        
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            columns.Add(new ColumnMetadata
            {
                ColumnName = reader.GetString(0),
                OrdinalPosition = reader.GetInt32(1),
                DataType = reader.GetString(2),
                MaxLength = reader.GetInt16(3),
                Precision = reader.GetByte(4),
                Scale = reader.GetByte(5),
                IsNullable = reader.GetBoolean(6),
                DefaultValue = reader.IsDBNull(7) ? null : reader.GetString(7),
                IsIdentity = reader.GetBoolean(8),
                IdentitySeed = reader.GetInt64(9),
                IdentityIncrement = reader.GetInt64(10),
                IsComputed = reader.GetBoolean(11),
                ComputedExpression = reader.IsDBNull(12) ? null : reader.GetString(12),
                Collation = reader.IsDBNull(13) ? null : reader.GetString(13)
            });
        }
        
        return columns;
    }

    private async Task<ConstraintMetadata?> ReadPrimaryKeyAsync(string schemaName, string tableName)
    {
        const string query = @"
            SELECT 
                kc.name AS ConstraintName,
                STRING_AGG(c.name, ',') WITHIN GROUP (ORDER BY ic.key_ordinal) AS Columns
            FROM sys.key_constraints kc
            INNER JOIN sys.tables t ON kc.parent_object_id = t.object_id
            INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
            INNER JOIN sys.index_columns ic ON kc.parent_object_id = ic.object_id AND kc.unique_index_id = ic.index_id
            INNER JOIN sys.columns c ON ic.object_id = c.object_id AND ic.column_id = c.column_id
            WHERE s.name = @SchemaName AND t.name = @TableName AND kc.type = 'PK'
            GROUP BY kc.name";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        command.Parameters.AddWithValue("@SchemaName", schemaName);
        command.Parameters.AddWithValue("@TableName", tableName);
        
        await using var reader = await command.ExecuteReaderAsync();
        
        if (await reader.ReadAsync())
        {
            var columns = reader.GetString(1).Split(',');
            return new ConstraintMetadata
            {
                ConstraintName = reader.GetString(0),
                ConstraintType = ConstraintType.PrimaryKey,
                TableSchema = schemaName,
                TableName = tableName,
                Columns = new List<string>(columns)
            };
        }
        
        return null;
    }

    private async Task<List<ConstraintMetadata>> ReadForeignKeysAsync(string schemaName, string tableName)
    {
        var foreignKeys = new List<ConstraintMetadata>();
        
        const string query = @"
            SELECT 
                fk.name AS ConstraintName,
                STRING_AGG(c.name, ',') WITHIN GROUP (ORDER BY fkc.constraint_column_id) AS Columns,
                rs.name + '.' + rt.name AS ReferencedTable,
                STRING_AGG(rc.name, ',') WITHIN GROUP (ORDER BY fkc.constraint_column_id) AS ReferencedColumns,
                fk.delete_referential_action_desc AS OnDeleteAction,
                fk.update_referential_action_desc AS OnUpdateAction
            FROM sys.foreign_keys fk
            INNER JOIN sys.tables t ON fk.parent_object_id = t.object_id
            INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
            INNER JOIN sys.foreign_key_columns fkc ON fk.object_id = fkc.constraint_object_id
            INNER JOIN sys.columns c ON fkc.parent_object_id = c.object_id AND fkc.parent_column_id = c.column_id
            INNER JOIN sys.tables rt ON fk.referenced_object_id = rt.object_id
            INNER JOIN sys.schemas rs ON rt.schema_id = rs.schema_id
            INNER JOIN sys.columns rc ON fkc.referenced_object_id = rc.object_id AND fkc.referenced_column_id = rc.column_id
            WHERE s.name = @SchemaName AND t.name = @TableName
            GROUP BY fk.name, rs.name, rt.name, fk.delete_referential_action_desc, fk.update_referential_action_desc";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        command.Parameters.AddWithValue("@SchemaName", schemaName);
        command.Parameters.AddWithValue("@TableName", tableName);
        
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            var referencedTableFull = reader.GetString(2);
            var referencedParts = referencedTableFull.Split('.');
            
            foreignKeys.Add(new ConstraintMetadata
            {
                ConstraintName = reader.GetString(0),
                ConstraintType = ConstraintType.ForeignKey,
                TableSchema = schemaName,
                TableName = tableName,
                Columns = new List<string>(reader.GetString(1).Split(',')),
                ReferencedSchema = referencedParts[0],
                ReferencedTable = referencedParts[1],
                ReferencedColumns = new List<string>(reader.GetString(3).Split(',')),
                OnDeleteAction = reader.GetString(4),
                OnUpdateAction = reader.GetString(5)
            });
        }
        
        return foreignKeys;
    }

    private async Task<List<ConstraintMetadata>> ReadUniqueConstraintsAsync(string schemaName, string tableName)
    {
        var uniqueConstraints = new List<ConstraintMetadata>();
        
        const string query = @"
            SELECT 
                kc.name AS ConstraintName,
                STRING_AGG(c.name, ',') WITHIN GROUP (ORDER BY ic.key_ordinal) AS Columns
            FROM sys.key_constraints kc
            INNER JOIN sys.tables t ON kc.parent_object_id = t.object_id
            INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
            INNER JOIN sys.index_columns ic ON kc.parent_object_id = ic.object_id AND kc.unique_index_id = ic.index_id
            INNER JOIN sys.columns c ON ic.object_id = c.object_id AND ic.column_id = c.column_id
            WHERE s.name = @SchemaName AND t.name = @TableName AND kc.type = 'UQ'
            GROUP BY kc.name";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        command.Parameters.AddWithValue("@SchemaName", schemaName);
        command.Parameters.AddWithValue("@TableName", tableName);
        
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            uniqueConstraints.Add(new ConstraintMetadata
            {
                ConstraintName = reader.GetString(0),
                ConstraintType = ConstraintType.Unique,
                TableSchema = schemaName,
                TableName = tableName,
                Columns = new List<string>(reader.GetString(1).Split(','))
            });
        }
        
        return uniqueConstraints;
    }

    private async Task<List<ConstraintMetadata>> ReadCheckConstraintsAsync(string schemaName, string tableName)
    {
        var checkConstraints = new List<ConstraintMetadata>();
        
        const string query = @"
            SELECT 
                cc.name AS ConstraintName,
                cc.definition AS CheckExpression
            FROM sys.check_constraints cc
            INNER JOIN sys.tables t ON cc.parent_object_id = t.object_id
            INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
            WHERE s.name = @SchemaName AND t.name = @TableName AND cc.is_disabled = 0";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        command.Parameters.AddWithValue("@SchemaName", schemaName);
        command.Parameters.AddWithValue("@TableName", tableName);
        
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            checkConstraints.Add(new ConstraintMetadata
            {
                ConstraintName = reader.GetString(0),
                ConstraintType = ConstraintType.Check,
                TableSchema = schemaName,
                TableName = tableName,
                CheckExpression = reader.GetString(1),
                Columns = new List<string>() // Check constraints can span multiple columns
            });
        }
        
        return checkConstraints;
    }

    private async Task<List<ConstraintMetadata>> ReadDefaultConstraintsAsync(string schemaName, string tableName)
    {
        var defaultConstraints = new List<ConstraintMetadata>();
        
        const string query = @"
            SELECT 
                dc.name AS ConstraintName,
                c.name AS ColumnName,
                dc.definition AS DefaultExpression
            FROM sys.default_constraints dc
            INNER JOIN sys.tables t ON dc.parent_object_id = t.object_id
            INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
            INNER JOIN sys.columns c ON dc.parent_object_id = c.object_id AND dc.parent_column_id = c.column_id
            WHERE s.name = @SchemaName AND t.name = @TableName";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        command.Parameters.AddWithValue("@SchemaName", schemaName);
        command.Parameters.AddWithValue("@TableName", tableName);
        
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            defaultConstraints.Add(new ConstraintMetadata
            {
                ConstraintName = reader.GetString(0),
                ConstraintType = ConstraintType.Default,
                TableSchema = schemaName,
                TableName = tableName,
                Columns = new List<string> { reader.GetString(1) },
                DefaultExpression = reader.GetString(2)
            });
        }
        
        return defaultConstraints;
    }

    private async Task<List<IndexMetadata>> ReadIndexesAsync(string schemaName, string tableName)
    {
        var indexes = new List<IndexMetadata>();
        
        const string query = @"
            SELECT 
                i.name AS IndexName,
                i.is_unique AS IsUnique,
                i.type_desc AS IndexType,
                i.is_primary_key AS IsPrimaryKey,
                i.filter_definition AS FilterExpression,
                (
                    SELECT STRING_AGG(c.name + CASE WHEN ic.is_descending_key = 1 THEN ' DESC' ELSE '' END, ',') 
                    WITHIN GROUP (ORDER BY ic.key_ordinal)
                    FROM sys.index_columns ic
                    INNER JOIN sys.columns c ON ic.object_id = c.object_id AND ic.column_id = c.column_id
                    WHERE ic.object_id = i.object_id AND ic.index_id = i.index_id AND ic.is_included_column = 0
                ) AS KeyColumns,
                (
                    SELECT STRING_AGG(c.name, ',') 
                    WITHIN GROUP (ORDER BY ic.key_ordinal)
                    FROM sys.index_columns ic
                    INNER JOIN sys.columns c ON ic.object_id = c.object_id AND ic.column_id = c.column_id
                    WHERE ic.object_id = i.object_id AND ic.index_id = i.index_id AND ic.is_included_column = 1
                ) AS IncludedColumns
            FROM sys.indexes i
            INNER JOIN sys.tables t ON i.object_id = t.object_id
            INNER JOIN sys.schemas s ON t.schema_id = s.schema_id
            WHERE s.name = @SchemaName AND t.name = @TableName 
                AND i.type IN (1, 2) -- Clustered and Non-clustered only
                AND i.is_primary_key = 0 -- Exclude PK index (handled by constraint)
            ORDER BY i.name";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        command.Parameters.AddWithValue("@SchemaName", schemaName);
        command.Parameters.AddWithValue("@TableName", tableName);
        
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            var keyColumnsStr = reader.IsDBNull(5) ? "" : reader.GetString(5);
            var includedColumnsStr = reader.IsDBNull(6) ? "" : reader.GetString(6);
            
            var columns = new List<IndexColumnMetadata>();
            if (!string.IsNullOrEmpty(keyColumnsStr))
            {
                var columnParts = keyColumnsStr.Split(',');
                for (int i = 0; i < columnParts.Length; i++)
                {
                    var part = columnParts[i].Trim();
                    var isDescending = part.EndsWith(" DESC");
                    var columnName = isDescending ? part[..^5].Trim() : part;
                    
                    columns.Add(new IndexColumnMetadata
                    {
                        ColumnName = columnName,
                        OrdinalPosition = i + 1,
                        IsDescending = isDescending
                    });
                }
            }
            
            indexes.Add(new IndexMetadata
            {
                IndexName = reader.GetString(0),
                TableSchema = schemaName,
                TableName = $"{schemaName}.{tableName}",
                IsUnique = reader.GetBoolean(1),
                IsClustered = reader.GetString(2) == "CLUSTERED",
                IsPrimaryKey = reader.GetBoolean(3),
                FilterExpression = reader.IsDBNull(4) ? null : reader.GetString(4),
                Columns = columns,
                IncludedColumns = string.IsNullOrEmpty(includedColumnsStr) 
                    ? new List<string>() 
                    : new List<string>(includedColumnsStr.Split(',').Select(c => c.Trim()))
            });
        }
        
        return indexes;
    }

    /// <summary>
    /// <summary>
    /// Reads all views with their SQL definitions.
    /// </summary>
    /// <param name="filter">Optional filter to include/exclude specific views based on pattern matching.</param>
    /// <returns>A list of programmable objects representing views.</returns>
    public async Task<List<ProgrammableObject>> ReadViewsAsync(Utils.ObjectFilter? filter = null)
    {
        var views = new List<ProgrammableObject>();
        
        const string query = @"
            SELECT 
                s.name AS SchemaName,
                v.name AS ObjectName,
                m.definition AS Definition
            FROM sys.views v
            INNER JOIN sys.schemas s ON v.schema_id = s.schema_id
            INNER JOIN sys.sql_modules m ON v.object_id = m.object_id
            WHERE v.is_ms_shipped = 0
            ORDER BY s.name, v.name";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            var schemaName = reader.GetString(0);
            var objectName = reader.GetString(1);

            // Apply filter if provided
            if (filter != null && !filter.ShouldInclude(schemaName, objectName))
            {
                continue;
            }

            views.Add(new ProgrammableObject
            {
                SchemaName = schemaName,
                ObjectName = objectName,
                ObjectType = ProgrammableObjectType.View,
                Definition = reader.GetString(2)
            });
        }
        
        return views;
    }

    /// <summary>
    /// Reads all stored procedures with their SQL definitions.
    /// </summary>
    /// <param name="filter">Optional filter to include/exclude specific stored procedures based on pattern matching.</param>
    /// <returns>A list of programmable objects representing stored procedures.</returns>
    public async Task<List<ProgrammableObject>> ReadStoredProceduresAsync(Utils.ObjectFilter? filter = null)
    {
        var procedures = new List<ProgrammableObject>();
        
        const string query = @"
            SELECT 
                s.name AS SchemaName,
                p.name AS ObjectName,
                m.definition AS Definition
            FROM sys.procedures p
            INNER JOIN sys.schemas s ON p.schema_id = s.schema_id
            INNER JOIN sys.sql_modules m ON p.object_id = m.object_id
            WHERE p.is_ms_shipped = 0
            ORDER BY s.name, p.name";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            var schemaName = reader.GetString(0);
            var objectName = reader.GetString(1);

            // Apply filter if provided
            if (filter != null && !filter.ShouldInclude(schemaName, objectName))
            {
                continue;
            }

            procedures.Add(new ProgrammableObject
            {
                SchemaName = schemaName,
                ObjectName = objectName,
                ObjectType = ProgrammableObjectType.StoredProcedure,
                Definition = reader.GetString(2)
            });
        }
        
        return procedures;
    }

    /// <summary>
    /// Reads all user-defined functions with their SQL definitions (scalar, inline table-valued, and multi-statement table-valued).
    /// </summary>
    /// <param name="filter">Optional filter to include/exclude specific functions based on pattern matching.</param>
    /// <returns>A list of programmable objects representing functions.</returns>
    public async Task<List<ProgrammableObject>> ReadFunctionsAsync(Utils.ObjectFilter? filter = null)
    {
        var functions = new List<ProgrammableObject>();
        
        const string query = @"
            SELECT 
                s.name AS SchemaName,
                o.name AS ObjectName,
                o.type_desc AS ObjectTypeDesc,
                m.definition AS Definition
            FROM sys.objects o
            INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
            INNER JOIN sys.sql_modules m ON o.object_id = m.object_id
            WHERE o.type IN ('FN', 'IF', 'TF') -- Scalar, Inline Table-Valued, Multi-statement Table-Valued
                AND o.is_ms_shipped = 0
            ORDER BY s.name, o.name";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            var schemaName = reader.GetString(0);
            var objectName = reader.GetString(1);

            // Apply filter if provided
            if (filter != null && !filter.ShouldInclude(schemaName, objectName))
            {
                continue;
            }

            var typeDesc = reader.GetString(2);
            var objectType = typeDesc switch
            {
                "SQL_SCALAR_FUNCTION" => ProgrammableObjectType.ScalarFunction,
                "SQL_INLINE_TABLE_VALUED_FUNCTION" => ProgrammableObjectType.InlineTableValuedFunction,
                "SQL_TABLE_VALUED_FUNCTION" => ProgrammableObjectType.TableValuedFunction,
                _ => ProgrammableObjectType.ScalarFunction
            };

            functions.Add(new ProgrammableObject
            {
                SchemaName = schemaName,
                ObjectName = objectName,
                ObjectType = objectType,
                Definition = reader.GetString(3)
            });
        }
        
        return functions;
    }

    /// <summary>
    /// Reads all triggers with their SQL definitions and parent table associations.
    /// </summary>
    /// <param name="filter">Optional filter to include/exclude specific triggers based on pattern matching.</param>
    /// <returns>A list of programmable objects representing triggers with parent table information.</returns>
    public async Task<List<ProgrammableObject>> ReadTriggersAsync(Utils.ObjectFilter? filter = null)
    {
        var triggers = new List<ProgrammableObject>();
        
        const string query = @"
            SELECT 
                s.name AS SchemaName,
                tr.name AS ObjectName,
                m.definition AS Definition,
                OBJECT_SCHEMA_NAME(tr.parent_id) AS ParentSchema,
                OBJECT_NAME(tr.parent_id) AS ParentTable
            FROM sys.triggers tr
            INNER JOIN sys.schemas s ON SCHEMA_ID() = s.schema_id
            INNER JOIN sys.sql_modules m ON tr.object_id = m.object_id
            WHERE tr.is_ms_shipped = 0
                AND tr.parent_class = 1 -- Table triggers only
            ORDER BY s.name, tr.name";

        await using var connection = await _connection.GetOpenConnectionAsync();
        await using var command = new SqlCommand(query, connection);
        await using var reader = await command.ExecuteReaderAsync();
        
        while (await reader.ReadAsync())
        {
            var schemaName = reader.GetString(0);
            var objectName = reader.GetString(1);

            // Apply filter if provided
            if (filter != null && !filter.ShouldInclude(schemaName, objectName))
            {
                continue;
            }

            var parentSchema = reader.IsDBNull(3) ? null : reader.GetString(3);
            var parentTable = reader.IsDBNull(4) ? null : reader.GetString(4);

            triggers.Add(new ProgrammableObject
            {
                SchemaName = schemaName,
                ObjectName = objectName,
                ObjectType = ProgrammableObjectType.Trigger,
                Definition = reader.GetString(2),
                ParentTableSchema = parentSchema,
                ParentTableName = parentTable
            });
        }
        
        return triggers;
    }
}
