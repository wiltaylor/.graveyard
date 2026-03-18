using MsSqlDump.Commands;
using System.CommandLine;
using System.Reflection;
using System.Threading.Tasks;

namespace MsSqlDump;

/// <summary>
/// Main entry point for the MSSQL Database Dump Tool
/// </summary>
class Program
{
    public static string Version => Assembly.GetExecutingAssembly()
        .GetCustomAttribute<AssemblyInformationalVersionAttribute>()
        ?.InformationalVersion ?? "1.0.0";

    static async Task<int> Main(string[] args)
    {
        var rootCommand = new RootCommand("MSSQL Database Dump Tool - Export SQL Server databases to organized SQL scripts")
        {
            Name = "mssqldump"
        };
        
        // Add dump command
        rootCommand.AddCommand(new DumpCommand());
        
        // Invoke and return exit code
        return await rootCommand.InvokeAsync(args);
    }
}
