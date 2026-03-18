using MsSqlDump.Core;
using MsSqlDump.Exporters;
using MsSqlDump.Models;
using MsSqlDump.Utils;
using MsSqlDump.Writers;
using Spectre.Console;
using System;
using System.CommandLine;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace MsSqlDump.Commands;

/// <summary>
/// Main dump command for exporting database schema and data
/// </summary>
public class DumpCommand : Command
{
    public DumpCommand() : base("dump", "Export a MSSQL database to SQL scripts")
    {
        // Required options
        var serverOption = new Option<string>(
            aliases: new[] { "--server", "-s" },
            description: "SQL Server hostname or IP address")
        { IsRequired = true };

        var databaseOption = new Option<string>(
            aliases: new[] { "--database", "-d" },
            description: "Name of the database to export")
        { IsRequired = true };

        // Authentication options
        var windowsAuthOption = new Option<bool>(
            aliases: new[] { "--windows-auth", "-w" },
            description: "Use Windows Authentication");

        var userOption = new Option<string?>(
            aliases: new[] { "--user", "-u" },
            description: "SQL Server authentication username");

        var passwordOption = new Option<string?>(
            aliases: new[] { "--password", "-p" },
            description: "SQL Server authentication password");

        // Output options
        var outputOption = new Option<string>(
            aliases: new[] { "--output", "-o" },
            getDefaultValue: () => "./output",
            description: "Output directory path");

        var schemaOnlyOption = new Option<bool>(
            "--schema-only",
            getDefaultValue: () => false,
            description: "Export schema without data");

        // Object type filters
        var tablesOption = new Option<bool>(
            "--tables",
            getDefaultValue: () => true,
            description: "Include tables");

        var viewsOption = new Option<bool>(
            "--views",
            getDefaultValue: () => true,
            description: "Include views");

        var proceduresOption = new Option<bool>(
            "--procedures",
            getDefaultValue: () => true,
            description: "Include stored procedures");

        var functionsOption = new Option<bool>(
            "--functions",
            getDefaultValue: () => true,
            description: "Include functions");

        var triggersOption = new Option<bool>(
            "--triggers",
            getDefaultValue: () => true,
            description: "Include triggers");

        // Pattern filters
        var includeOption = new Option<string[]>(
            "--include",
            getDefaultValue: () => Array.Empty<string>(),
            description: "Include objects matching regex patterns (e.g., --include 'dbo\\..*' 'sales\\..*')");

        var excludeOption = new Option<string[]>(
            "--exclude",
            getDefaultValue: () => Array.Empty<string>(),
            description: "Exclude objects matching regex patterns (e.g., --exclude 'temp.*' 'test.*')");

        // Advanced options
        var batchSizeOption = new Option<int>(
            "--batch-size",
            getDefaultValue: () => 1000,
            description: "Rows per INSERT statement (data export)");

        var timeoutOption = new Option<int>(
            "--timeout",
            getDefaultValue: () => 30,
            description: "Connection timeout in seconds");

        var retriesOption = new Option<int>(
            "--retries",
            getDefaultValue: () => 3,
            description: "Number of connection retry attempts");

        var verboseOption = new Option<bool>(
            "--verbose",
            getDefaultValue: () => false,
            description: "Enable verbose logging with detailed progress information");

        // Add all options
        AddOption(serverOption);
        AddOption(databaseOption);
        AddOption(windowsAuthOption);
        AddOption(userOption);
        AddOption(passwordOption);
        AddOption(outputOption);
        AddOption(schemaOnlyOption);
        AddOption(tablesOption);
        AddOption(viewsOption);
        AddOption(proceduresOption);
        AddOption(functionsOption);
        AddOption(triggersOption);
        AddOption(includeOption);
        AddOption(excludeOption);
        AddOption(batchSizeOption);
        AddOption(timeoutOption);
        AddOption(retriesOption);
        AddOption(verboseOption);

        // Set handler
        this.SetHandler(async (context) =>
        {
            var options = new CommandOptions
            {
                Server = context.ParseResult.GetValueForOption(serverOption)!,
                Database = context.ParseResult.GetValueForOption(databaseOption)!,
                WindowsAuth = context.ParseResult.GetValueForOption(windowsAuthOption),
                User = context.ParseResult.GetValueForOption(userOption),
                Password = context.ParseResult.GetValueForOption(passwordOption),
                Output = context.ParseResult.GetValueForOption(outputOption)!,
                SchemaOnly = context.ParseResult.GetValueForOption(schemaOnlyOption),
                Tables = context.ParseResult.GetValueForOption(tablesOption),
                Views = context.ParseResult.GetValueForOption(viewsOption),
                Procedures = context.ParseResult.GetValueForOption(proceduresOption),
                Functions = context.ParseResult.GetValueForOption(functionsOption),
                Triggers = context.ParseResult.GetValueForOption(triggersOption),
                Include = new List<string>(context.ParseResult.GetValueForOption(includeOption) ?? Array.Empty<string>()),
                Exclude = new List<string>(context.ParseResult.GetValueForOption(excludeOption) ?? Array.Empty<string>()),
                BatchSize = context.ParseResult.GetValueForOption(batchSizeOption),
                Timeout = context.ParseResult.GetValueForOption(timeoutOption),
                Retries = context.ParseResult.GetValueForOption(retriesOption),
                Verbose = context.ParseResult.GetValueForOption(verboseOption)
            };

            context.ExitCode = await ExecuteAsync(options);
        });
    }

    /// <summary>
    /// Executes the dump command
    /// </summary>
    public async Task<int> ExecuteAsync(CommandOptions options)
    {
        try
        {
            // Validate options
            if (!options.IsAuthenticationValid())
            {
                AnsiConsole.MarkupLine("[red]Error: Either --windows-auth or --user and --password are required[/]");
                return 1;
            }

            if (!options.IsObjectTypeFilterValid())
            {
                AnsiConsole.MarkupLine("[red]Error: At least one object type must be enabled[/]");
                return 1;
            }

            // Validate regex patterns
            if (!ObjectFilter.ValidatePatterns(options.Include, out var includeError))
            {
                AnsiConsole.MarkupLine($"[red]Error: {includeError}[/]");
                return 1;
            }

            if (!ObjectFilter.ValidatePatterns(options.Exclude, out var excludeError))
            {
                AnsiConsole.MarkupLine($"[red]Error: {excludeError}[/]");
                return 1;
            }

            // Create object filter
            var objectFilter = new ObjectFilter(options.Include, options.Exclude);

            // Display operation summary
            AnsiConsole.MarkupLine($"[bold]Database:[/] {options.Server}/{options.Database}");
            AnsiConsole.MarkupLine($"[bold]Output:[/] {Path.GetFullPath(options.Output)}");
            AnsiConsole.MarkupLine($"[bold]Mode:[/] {(options.SchemaOnly ? "Schema Only" : "Schema + Data")}");
            
            if (objectFilter.HasFilters)
            {
                if (objectFilter.IncludePatternCount > 0)
                    AnsiConsole.MarkupLine($"[bold]Include Patterns:[/] {objectFilter.IncludePatternCount}");
                if (objectFilter.ExcludePatternCount > 0)
                    AnsiConsole.MarkupLine($"[bold]Exclude Patterns:[/] {objectFilter.ExcludePatternCount}");
            }
            
            AnsiConsole.WriteLine();

            // Connect to database
            DatabaseConnection? connection = null;
            SqlVersionDetector? versionDetector = null;
            int sqlVersion = 11; // Default to SQL 2012

            if (options.Verbose)
            {
                AnsiConsole.MarkupLine($"[dim]Connecting to server: {options.Server}, database: {options.Database}[/]");
            }

            await AnsiConsole.Status()
                .StartAsync("Connecting to database...", async ctx =>
                {
                    connection = new DatabaseConnection(
                        options.Server,
                        options.Database,
                        options.User,
                        options.Password,
                        options.WindowsAuth,
                        options.Timeout,
                        options.Retries,
                        5);

                    versionDetector = new SqlVersionDetector();
                    sqlVersion = await versionDetector.DetectMajorVersionAsync(await connection!.GetOpenConnectionAsync());
                    
                    ctx.Status($"Connected to SQL Server (version {sqlVersion})");
                });

            AnsiConsole.MarkupLine($"[green]✓[/] Connected to SQL Server (version {sqlVersion})");
            
            if (options.Verbose)
            {
                AnsiConsole.MarkupLine($"[dim]SQL Server major version: {sqlVersion}[/]");
            }

            // Read schema metadata
            SchemaReader schemaReader;
            DatabaseSchema schema = null!;

            if (options.Verbose)
            {
                AnsiConsole.MarkupLine($"[dim]Reading schema for database: {options.Database}[/]");
                if (objectFilter != null)
                {
                    AnsiConsole.MarkupLine($"[dim]Include patterns: {string.Join(", ", options.Include)}[/]");
                    AnsiConsole.MarkupLine($"[dim]Exclude patterns: {string.Join(", ", options.Exclude)}[/]");
                }
            }

            await AnsiConsole.Status()
                .StartAsync("Reading database schema...", async ctx =>
                {
                    schemaReader = new SchemaReader(connection!, versionDetector!);
                    var tables = options.Tables ? await schemaReader.ReadTableMetadataAsync(objectFilter) : new List<TableMetadata>();
                    
                    ctx.Status("Reading views...");
                    var views = options.Views ? await schemaReader.ReadViewsAsync(objectFilter) : new List<ProgrammableObject>();
                    
                    ctx.Status("Reading stored procedures...");
                    var procedures = options.Procedures ? await schemaReader.ReadStoredProceduresAsync(objectFilter) : new List<ProgrammableObject>();
                    
                    ctx.Status("Reading functions...");
                    var functions = options.Functions ? await schemaReader.ReadFunctionsAsync(objectFilter) : new List<ProgrammableObject>();
                    
                    ctx.Status("Reading triggers...");
                    var triggers = options.Triggers ? await schemaReader.ReadTriggersAsync(objectFilter) : new List<ProgrammableObject>();
                    
                    schema = new DatabaseSchema
                    {
                        DatabaseName = options.Database,
                        ServerVersion = sqlVersion.ToString(),
                        ServerMajorVersion = sqlVersion,
                        Tables = tables,
                        Views = views,
                        Procedures = procedures,
                        Functions = functions,
                        Triggers = triggers
                    };
                    
                    ctx.Status($"Found {tables.Count} tables, {views.Count} views, {procedures.Count} procedures, {functions.Count} functions, {triggers.Count} triggers");
                });

            var objectCounts = new List<string>();
            if (options.Tables) objectCounts.Add($"{schema.Tables.Count} tables");
            if (options.Views) objectCounts.Add($"{schema.Views.Count} views");
            if (options.Procedures) objectCounts.Add($"{schema.Procedures.Count} procedures");
            if (options.Functions) objectCounts.Add($"{schema.Functions.Count} functions");
            if (options.Triggers) objectCounts.Add($"{schema.Triggers.Count} triggers");
            
            AnsiConsole.MarkupLine($"[green]✓[/] Found {string.Join(", ", objectCounts)}");
            
            if (objectFilter.HasFilters)
            {
                var totalObjects = schema.Tables.Count + schema.Views.Count + schema.Procedures.Count + schema.Functions.Count + schema.Triggers.Count;
                AnsiConsole.MarkupLine($"[dim]  ({totalObjects} objects after filtering)[/]");
            }

            // Build dependency graph
            DependencyGraph dependencyGraph;
            
            if (options.Verbose)
            {
                AnsiConsole.MarkupLine($"[dim]Building dependency graph for {schema.Tables.Count} tables...[/]");
            }
            
            await AnsiConsole.Status()
                .StartAsync("Analyzing dependencies...", async ctx =>
                {
                    await Task.Run(() =>
                    {
                        var resolver = new DependencyResolver();
                        dependencyGraph = resolver.BuildDependencyGraph(schema.Tables);
                        
                        var cycles = dependencyGraph.DetectCircularDependencies();
                        if (cycles.Any())
                        {
                            ctx.Status($"Warning: Found {cycles.Count} circular dependencies");
                        }
                    });
                });

            var resolver = new DependencyResolver();
            dependencyGraph = resolver.BuildDependencyGraph(schema.Tables);
            var circularDeps = dependencyGraph.DetectCircularDependencies();
            
            if (circularDeps.Any())
            {
                AnsiConsole.MarkupLine($"[yellow]⚠[/] Found {circularDeps.Count} circular dependency groups");
                
                if (options.Verbose)
                {
                    foreach (var cycle in circularDeps.Take(3))
                    {
                        AnsiConsole.MarkupLine($"[dim]  Cycle: {string.Join(" -> ", cycle)}[/]");
                    }
                    if (circularDeps.Count > 3)
                    {
                        AnsiConsole.MarkupLine($"[dim]  ... and {circularDeps.Count - 3} more[/]");
                    }
                }
            }
            else
            {
                AnsiConsole.MarkupLine("[green]✓[/] No circular dependencies detected");
            }

            // Create output directories
            var organizer = new DirectoryOrganizer(options.Output);
            organizer.CreateOutputStructure(includeData: !options.SchemaOnly);
            AnsiConsole.MarkupLine($"[green]✓[/] Created output directory structure");

            // Export tables in dependency order
            var exportOrder = dependencyGraph.GetTopologicalOrder();
            var exporter = new TableExporter(sqlVersion);
            var scriptWriter = new ScriptWriter();
            var progressReporter = new ProgressReporter();

            await AnsiConsole.Progress()
                .StartAsync(async ctx =>
                {
                    var task = ctx.AddTask("[green]Exporting tables[/]", maxValue: exportOrder.Count);

                    foreach (var tableName in exportOrder)
                    {
                        var table = schema.Tables.FirstOrDefault(t => t.FullName == tableName);
                        if (table == null) continue;

                        // Generate and write CREATE TABLE script
                        var createScript = exporter.GenerateCreateTableScript(table);
                        var tablePath = Path.Combine(organizer.TablesDirectory, $"{table.SchemaName}.{table.TableName}.sql");
                        await scriptWriter.WriteScriptAsync(tablePath, createScript);

                        // Generate and write index scripts
                        var indexScripts = exporter.GenerateIndexScripts(table);
                        if (indexScripts.Any())
                        {
                            var indexScript = string.Join("\nGO\n\n", indexScripts);
                            await scriptWriter.AppendScriptAsync(tablePath, "\nGO\n\n" + indexScript);
                        }

                        task.Increment(1);
                        task.Description = $"[green]Exported {table.SchemaName}.{table.TableName}[/]";
                    }
                });

            AnsiConsole.MarkupLine($"[green]✓[/] Exported {exportOrder.Count} tables");

            // Export constraints (foreign keys need all tables created first)
            await AnsiConsole.Progress()
                .StartAsync(async ctx =>
                {
                    var task = ctx.AddTask("[green]Exporting constraints[/]", maxValue: schema.Tables.Count);

                    foreach (var table in schema.Tables)
                    {
                        var constraintScripts = exporter.GenerateConstraintScripts(table);
                        if (constraintScripts.Any())
                        {
                            var constraintScript = string.Join("\nGO\n\n", constraintScripts);
                            var tablePath = Path.Combine(organizer.TablesDirectory, $"{table.SchemaName}.{table.TableName}.sql");
                            await scriptWriter.AppendScriptAsync(tablePath, "\nGO\n\n" + constraintScript);
                        }

                        task.Increment(1);
                    }
                });

            AnsiConsole.MarkupLine("[green]✓[/] Exported constraints");

            // Export data if not schema-only mode
            if (!options.SchemaOnly)
            {
                AnsiConsole.MarkupLine("\n[bold]Exporting table data...[/]");
                
                var dataExporter = new DataExporter(connection!, options.BatchSize);
                var tablesInCycles = circularDeps.SelectMany(c => c).ToHashSet();

                await AnsiConsole.Progress()
                    .StartAsync(async ctx =>
                    {
                        var task = ctx.AddTask("[green]Exporting data[/]", maxValue: exportOrder.Count);

                        foreach (var tableName in exportOrder)
                        {
                            var table = schema.Tables.FirstOrDefault(t => t.FullName == tableName);
                            if (table == null || table.RowCount == 0)
                            {
                                task.Increment(1);
                                continue;
                            }

                            var dataPath = Path.Combine(organizer.DataDirectory, $"{table.SchemaName}.{table.TableName}.sql");
                            var dataScript = new StringBuilder();

                            // Handle tables in circular dependency groups
                            if (tablesInCycles.Contains(tableName))
                            {
                                var disableFKScript = dataExporter.GenerateDisableForeignKeysScript(table);
                                if (!string.IsNullOrEmpty(disableFKScript))
                                {
                                    dataScript.AppendLine("-- Disable foreign key constraints for circular dependency");
                                    dataScript.AppendLine(disableFKScript);
                                    dataScript.AppendLine("GO");
                                    dataScript.AppendLine();
                                }
                            }

                            // Export table data
                            var insertScript = await dataExporter.ExportTableDataAsync(table);
                            dataScript.Append(insertScript);

                            // Re-enable constraints for circular dependency tables
                            if (tablesInCycles.Contains(tableName))
                            {
                                var enableFKScript = dataExporter.GenerateEnableForeignKeysScript(table);
                                if (!string.IsNullOrEmpty(enableFKScript))
                                {
                                    dataScript.AppendLine();
                                    dataScript.AppendLine("-- Re-enable foreign key constraints");
                                    dataScript.AppendLine(enableFKScript);
                                    dataScript.AppendLine("GO");
                                }
                            }

                            await scriptWriter.WriteScriptAsync(dataPath, dataScript.ToString());

                            task.Increment(1);
                            task.Description = $"[green]Exported {table.RowCount:N0} rows from {table.SchemaName}.{table.TableName}[/]";
                            
                            if (options.Verbose)
                            {
                                var fileSize = new FileInfo(dataPath).Length;
                                AnsiConsole.MarkupLine($"[dim]  Written {fileSize:N0} bytes to {Path.GetFileName(dataPath)}[/]");
                            }
                        }
                    });

                var totalRows = schema.Tables.Sum(t => t.RowCount);
                AnsiConsole.MarkupLine($"[green]✓[/] Exported {totalRows:N0} total rows from {schema.Tables.Count} tables");
            }

            // Export programmable objects
            if (options.Views && schema.Views.Any())
            {
                AnsiConsole.MarkupLine("\n[bold]Exporting views...[/]");
                var viewExporter = new ViewExporter(sqlVersion);
                
                await AnsiConsole.Progress()
                    .StartAsync(async ctx =>
                    {
                        var task = ctx.AddTask("[green]Exporting views[/]", maxValue: schema.Views.Count);

                        foreach (var view in schema.Views)
                        {
                            var viewScript = viewExporter.GenerateCreateViewScript(view);
                            var viewPath = Path.Combine(organizer.ViewsDirectory, $"{view.SchemaName}.{view.ObjectName}.sql");
                            await scriptWriter.WriteScriptAsync(viewPath, viewScript);
                            
                            task.Increment(1);
                            task.Description = $"[green]Exported {view.SchemaName}.{view.ObjectName}[/]";
                        }
                    });
                
                AnsiConsole.MarkupLine($"[green]✓[/] Exported {schema.Views.Count} views");
            }

            if (options.Procedures && schema.Procedures.Any())
            {
                AnsiConsole.MarkupLine("\n[bold]Exporting stored procedures...[/]");
                var procedureExporter = new ProcedureExporter(sqlVersion);
                
                await AnsiConsole.Progress()
                    .StartAsync(async ctx =>
                    {
                        var task = ctx.AddTask("[green]Exporting procedures[/]", maxValue: schema.Procedures.Count);

                        foreach (var procedure in schema.Procedures)
                        {
                            var procedureScript = procedureExporter.GenerateCreateProcedureScript(procedure);
                            var procedurePath = Path.Combine(organizer.ProceduresDirectory, $"{procedure.SchemaName}.{procedure.ObjectName}.sql");
                            await scriptWriter.WriteScriptAsync(procedurePath, procedureScript);
                            
                            task.Increment(1);
                            task.Description = $"[green]Exported {procedure.SchemaName}.{procedure.ObjectName}[/]";
                        }
                    });
                
                AnsiConsole.MarkupLine($"[green]✓[/] Exported {schema.Procedures.Count} stored procedures");
            }

            if (options.Functions && schema.Functions.Any())
            {
                AnsiConsole.MarkupLine("\n[bold]Exporting functions...[/]");
                var functionExporter = new FunctionExporter(sqlVersion);
                
                await AnsiConsole.Progress()
                    .StartAsync(async ctx =>
                    {
                        var task = ctx.AddTask("[green]Exporting functions[/]", maxValue: schema.Functions.Count);

                        foreach (var function in schema.Functions)
                        {
                            var functionScript = functionExporter.GenerateCreateFunctionScript(function);
                            var functionPath = Path.Combine(organizer.FunctionsDirectory, $"{function.SchemaName}.{function.ObjectName}.sql");
                            await scriptWriter.WriteScriptAsync(functionPath, functionScript);
                            
                            task.Increment(1);
                            task.Description = $"[green]Exported {function.SchemaName}.{function.ObjectName}[/]";
                        }
                    });
                
                AnsiConsole.MarkupLine($"[green]✓[/] Exported {schema.Functions.Count} functions");
            }

            if (options.Triggers && schema.Triggers.Any())
            {
                AnsiConsole.MarkupLine("\n[bold]Exporting triggers...[/]");
                var triggerExporter = new TriggerExporter(sqlVersion);
                
                await AnsiConsole.Progress()
                    .StartAsync(async ctx =>
                    {
                        var task = ctx.AddTask("[green]Exporting triggers[/]", maxValue: schema.Triggers.Count);

                        foreach (var trigger in schema.Triggers)
                        {
                            var triggerScript = triggerExporter.GenerateCreateTriggerScript(trigger);
                            var triggerPath = Path.Combine(organizer.TriggersDirectory, $"{trigger.SchemaName}.{trigger.ObjectName}.sql");
                            await scriptWriter.WriteScriptAsync(triggerPath, triggerScript);
                            
                            task.Increment(1);
                            task.Description = $"[green]Exported {trigger.SchemaName}.{trigger.ObjectName}[/]";
                        }
                    });
                
                AnsiConsole.MarkupLine($"[green]✓[/] Exported {schema.Triggers.Count} triggers");
            }

            // Write execution order file
            var executionOrderPath = Path.Combine(options.Output, "_execution_order.txt");
            var executionOrderList = new List<string>();
            
            // Add schema scripts in dependency order
            executionOrderList.Add("# Schema Creation (Tables, Indexes, Constraints)");
            foreach (var tableName in exportOrder)
            {
                var table = schema.Tables.FirstOrDefault(t => t.FullName == tableName);
                if (table != null)
                {
                    executionOrderList.Add($"tables/{table.SchemaName}.{table.TableName}.sql");
                }
            }
            
            // Add data scripts in dependency order (if not schema-only)
            if (!options.SchemaOnly)
            {
                executionOrderList.Add("");
                executionOrderList.Add("# Data Import");
                foreach (var tableName in exportOrder)
                {
                    var table = schema.Tables.FirstOrDefault(t => t.FullName == tableName);
                    if (table != null && table.RowCount > 0)
                    {
                        executionOrderList.Add($"data/{table.SchemaName}.{table.TableName}.sql");
                    }
                }
            }
            
            // Add programmable objects
            if (schema.Views.Any())
            {
                executionOrderList.Add("");
                executionOrderList.Add("# Views");
                foreach (var view in schema.Views)
                {
                    executionOrderList.Add($"views/{view.SchemaName}.{view.ObjectName}.sql");
                }
            }
            
            if (schema.Procedures.Any())
            {
                executionOrderList.Add("");
                executionOrderList.Add("# Stored Procedures");
                foreach (var procedure in schema.Procedures)
                {
                    executionOrderList.Add($"procedures/{procedure.SchemaName}.{procedure.ObjectName}.sql");
                }
            }
            
            if (schema.Functions.Any())
            {
                executionOrderList.Add("");
                executionOrderList.Add("# Functions");
                foreach (var function in schema.Functions)
                {
                    executionOrderList.Add($"functions/{function.SchemaName}.{function.ObjectName}.sql");
                }
            }
            
            if (schema.Triggers.Any())
            {
                executionOrderList.Add("");
                executionOrderList.Add("# Triggers");
                foreach (var trigger in schema.Triggers)
                {
                    executionOrderList.Add($"triggers/{trigger.SchemaName}.{trigger.ObjectName}.sql");
                }
            }
            
            await File.WriteAllTextAsync(executionOrderPath, string.Join("\n", executionOrderList));
            AnsiConsole.MarkupLine("[green]✓[/] Created execution order file");

            AnsiConsole.WriteLine();
            AnsiConsole.MarkupLine("[bold green]Export completed successfully![/]");
            AnsiConsole.MarkupLine($"[dim]Output location: {options.Output}[/]");
            
            if (options.Verbose)
            {
                // Calculate total directory size
                var outputDir = new DirectoryInfo(options.Output);
                var totalSize = outputDir.GetFiles("*", SearchOption.AllDirectories).Sum(f => f.Length);
                AnsiConsole.MarkupLine($"[dim]Total export size: {totalSize:N0} bytes ({totalSize / 1024.0:F2} KB)[/]");
                
                var totalObjects = schema.Tables.Count + schema.Views.Count + schema.Procedures.Count + schema.Functions.Count + schema.Triggers.Count;
                AnsiConsole.MarkupLine($"[dim]Total objects exported: {totalObjects}[/]");
            }
            
            connection?.Dispose();
            return 0;
        }
        catch (Exception ex)
        {
            AnsiConsole.WriteException(ex);
            return 1;
        }
    }
}
