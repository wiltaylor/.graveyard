using Spectre.Console;

namespace MsSqlDump.Utils;

/// <summary>
/// Utility for reporting progress to the console using Spectre.Console.
/// </summary>
public class ProgressReporter
{
    /// <summary>
    /// Reports progress for a long-running operation with a progress bar.
    /// </summary>
    public async Task ReportAsync<T>(
        string description,
        IEnumerable<T> items,
        Func<T, ProgressTask, Task> action)
    {
        var itemList = items.ToList();
        
        await AnsiConsole.Progress()
            .Columns(
                new TaskDescriptionColumn(),
                new ProgressBarColumn(),
                new PercentageColumn(),
                new RemainingTimeColumn(),
                new SpinnerColumn())
            .StartAsync(async ctx =>
            {
                var task = ctx.AddTask(description, maxValue: itemList.Count);
                
                foreach (var item in itemList)
                {
                    await action(item, task);
                    task.Increment(1);
                }
            });
    }

    /// <summary>
    /// Reports status with a simple status message.
    /// </summary>
    public void Status(string message, Action action)
    {
        AnsiConsole.Status()
            .Start(message, ctx =>
            {
                ctx.Spinner(Spinner.Known.Dots);
                ctx.SpinnerStyle(Style.Parse("green"));
                action();
            });
    }

    /// <summary>
    /// Reports status with a simple status message (async version).
    /// </summary>
    public async Task StatusAsync(string message, Func<Task> action)
    {
        await AnsiConsole.Status()
            .StartAsync(message, async ctx =>
            {
                ctx.Spinner(Spinner.Known.Dots);
                ctx.SpinnerStyle(Style.Parse("green"));
                await action();
            });
    }

    /// <summary>
    /// Writes a success message to the console.
    /// </summary>
    public void Success(string message)
    {
        AnsiConsole.MarkupLine($"[green]✓[/] {Markup.Escape(message)}");
    }

    /// <summary>
    /// Writes an error message to the console.
    /// </summary>
    public void Error(string message)
    {
        AnsiConsole.MarkupLine($"[red]✗[/] {Markup.Escape(message)}");
    }

    /// <summary>
    /// Writes an info message to the console.
    /// </summary>
    public void Info(string message)
    {
        AnsiConsole.MarkupLine($"[blue]ℹ[/] {Markup.Escape(message)}");
    }

    /// <summary>
    /// Writes a warning message to the console.
    /// </summary>
    public void Warning(string message)
    {
        AnsiConsole.MarkupLine($"[yellow]⚠[/] {Markup.Escape(message)}");
    }

    /// <summary>
    /// Creates a table for displaying structured data.
    /// </summary>
    public Table CreateTable(params string[] headers)
    {
        var table = new Table();
        
        foreach (var header in headers)
        {
            table.AddColumn(new TableColumn(header).Centered());
        }
        
        return table;
    }

    /// <summary>
    /// Displays a table in the console.
    /// </summary>
    public void DisplayTable(Table table)
    {
        AnsiConsole.Write(table);
    }
}
