using InterviewDemoApp.Core;
using Microsoft.AspNetCore.SignalR;

namespace InterviewDemoApp.Web.Hubs;

/// <summary>
/// SignalR hub used to communicate with the server in real time.
/// </summary>
public class NumberHub: Hub<INumberClientHub>
{
    private readonly INumberManager _numberManager;
    public NumberHub(INumberManager numberManager)
    {
        _numberManager = numberManager;
    }

    /// <summary>
    /// Determine if the Interval has been set or not.
    ///
    /// This needs to be stored in the context as a new instance of this class
    /// is created for each method call.
    /// </summary>
    private bool SetupWasCalled
    {
        get
        {
            if(Context?.Items != null && Context.Items.ContainsKey("setupWasCalled"))
            {
                return (bool) (Context.Items["setupWasCalled"] ?? false);
            }

            return false;
        }
    }
    
    /// <summary>
    /// Set the time interval for results to be sent to the client.
    /// </summary>
    /// <param name="interval">How long in seconds before each update is sent.</param>
    public async Task SetupIntervals(int interval)
    {
        _numberManager.SetInterval(interval);
        await Clients.Caller.SendMessage($"Setup intervals for {interval} seconds.");
        Context.Items["setupWasCalled"] = true;
        
        _numberManager.OnTick += async (_, list) =>
        {
            await Clients.Caller.SendMessage(KeyPairListToString(list));
        };
    }

    /// <summary>
    /// Converts a list of key pairs into a result string that is displayed on the client.
    /// </summary>
    /// <param name="list">List of key pairs to convert.</param>
    /// <returns>comma seperated list of key pairs.</returns>
    private static string KeyPairListToString(IEnumerable<KeyValuePair<ulong, ulong>> list)
    {
       return string.Join(",", list.Select(p => $"{p.Key}:{p.Value}"));
    }

    /// <summary>
    /// Adds a number to the number manager to count.
    /// </summary>
    /// <param name="number">Number to add.</param>
    public async Task AddNumber(ulong number)
    {
        if (!SetupWasCalled)
        {
            await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
        }

        _numberManager.AddNumber(number);
    }

    /// <summary>
    /// Halt updates from being sent to the client.
    /// </summary>
    public async Task Halt()
    {
        if (!SetupWasCalled)
        {
            await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
            return;
        }
        
        _numberManager.Halt();
    }

    /// <summary>
    /// Resume updates being sent to the client.
    /// </summary>
    public async Task Resume()
    {
        if (!SetupWasCalled)
        {
            await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
            return;
        }
        
        _numberManager.Resume();
    }

    /// <summary>
    /// Return the current frequencies of numbers and then drop the connection.
    /// </summary>
    public async Task Quit()
    {
        if (!SetupWasCalled)
        {
            await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
            return;
        }

        await Clients.Caller.SendMessage(KeyPairListToString(_numberManager.GetCounts()));
        Context.Abort();
    }
}